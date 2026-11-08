//! Entrenador neuroevolutivo de alto rendimiento para Super Mario Bros. (ROM real)
//! sobre el emulador NES en Rust (`smb_rs`).
//!
//! Mejoras principales:
//!   1. Snapshot instantáneo tras la pantalla de título (ahorra millones de frames).
//!   2. Sensores de colisión exactos desde el Block Buffer de RAM ($0500-$069F) y
//!      tablas de enemigos ($000F-$00CF): 0 falsos positivos con nubes/arbustos.
//!   3. Predicción balística de aterrizaje + altura de muros + anchura de pozos.
//!   4. Frame skipping inteligente con interrupción al aterrizar y liberación
//!      automática de 1 frame del botón A (flanco de subida exigido por la NES).
//!   5. Optimización Esfuerzo vs. Tiempo: corte temprano por estancamiento,
//!      horizonte progresivo, fitness de speedrun y mutación adaptativa.
//!
//! Uso:
//!   cargo run --release --bin train -- train --gens 300 --pop 64 --skip 4 \
//!       --seed-file C:\Users\USUARIO\mario-ai\rust\best_real.json \
//!       --out best_ai.json
//!   cargo run --release --bin train -- eval --weights best_ai.json
//!   cargo run --release --bin train -- watch --weights best_ai.json

use smb_rs::{
    Cartridge, Console, BUTTON_A, BUTTON_B, BUTTON_LEFT, BUTTON_RIGHT, BUTTON_START,
};
use std::sync::Arc;

const OBS: usize = 120;
const HID: usize = 48;
const ACT: usize = 7;
const WORLD_W: i32 = 212 * 16;
const MAX_FRAMES: usize = 3000;
const DEFAULT_SKIP: usize = 4;
const STUCK_TIMEOUT: usize = 150; // ~2.5s sin avanzar = cortar episodio
/// Frames que se fuerza A tras decidir "saltar". El salto de SMB depende de
/// cuántos frames se mantiene A (gravedad reducida). Sin esto, la red decide
/// cada 4 frames y suelta A enseguida: salta ~20 px y se estrella contra el
/// tubo. Con el compromiso el salto es completo (≈4 tiles) y pasa el tubo.
const JUMP_HOLD: i32 = 26;

const ROM_DEFAULT: &str =
    r"C:\Users\USUARIO\Downloads\Super Mario Bros. (World)\Super Mario Bros. (World).nes";

const ACT_BUTTONS: [u8; ACT] = [
    0,
    BUTTON_RIGHT,
    BUTTON_RIGHT | BUTTON_A,
    BUTTON_RIGHT | BUTTON_B,
    BUTTON_RIGHT | BUTTON_A | BUTTON_B,
    BUTTON_A,
    BUTTON_LEFT,
];

fn buttons_for(action: usize) -> u8 {
    ACT_BUTTONS[action.min(ACT - 1)]
}

// --------------------------------------------------------------------------- //
// Snapshot del emulador tras el warmup (evita repetir el título en cada run)
// --------------------------------------------------------------------------- //
#[derive(Clone)]
struct Snapshot {
    cpu: smb_rs::cpu::Cpu,
    ram: [u8; 2048],
    vram: [u8; 4096],
    palette: [u8; 32],
    oam: [u8; 256],
    oam_addr: u8,
    ctrl: u8,
    mask: u8,
    status: u8,
    v: u16,
    t: u16,
    fine_x: u8,
    w: bool,
    read_buffer: u8,
    scanline: u16,
    dot: u16,
    frame_ready: bool,
    nmi: bool,
    frame_count: u64,
    sprite0_hit_at: i32,
    frame: Vec<u32>,
}

impl Snapshot {
    fn capture(con: &Console) -> Self {
        let p = &con.bus.ppu;
        Snapshot {
            cpu: con.cpu.clone(),
            ram: con.bus.ram,
            vram: p.vram,
            palette: p.palette,
            oam: p.oam,
            oam_addr: p.oam_addr,
            ctrl: p.ctrl,
            mask: p.mask,
            status: p.status,
            v: p.v,
            t: p.t,
            fine_x: p.fine_x,
            w: p.w,
            read_buffer: p.read_buffer,
            scanline: p.scanline,
            dot: p.dot,
            frame_ready: p.frame_ready,
            nmi: p.nmi,
            frame_count: p.frame_count,
            sprite0_hit_at: p.sprite0_hit_at,
            frame: p.frame.clone(),
        }
    }

    fn restore(&self, con: &mut Console) {
        con.cpu = self.cpu.clone();
        con.bus.ram = self.ram;
        con.bus.buttons = [0; 2];
        con.bus.strobe = false;
        con.bus.shift = [0; 2];
        con.bus.open_bus = 0;
        con.bus.apu = smb_rs::apu::Apu::new();
        let p = &mut con.bus.ppu;
        p.vram = self.vram;
        p.palette = self.palette;
        p.oam = self.oam;
        p.oam_addr = self.oam_addr;
        p.ctrl = self.ctrl;
        p.mask = self.mask;
        p.status = self.status;
        p.v = self.v;
        p.t = self.t;
        p.fine_x = self.fine_x;
        p.w = self.w;
        p.read_buffer = self.read_buffer;
        // CLAVE: el snapshot se captura justo cuando el PPU acaba de levantar
        // frame_ready (scanline ~240). Si restauramos esa posición, el primer
        // step_frame vuelve a levantar frame_ready tras 1-2 instrucciones y el
        // juego queda congelado (Mario en x=40 para siempre). Rearrancamos el
        // PPU al principio del frame para que el siguiente step_frame ejecute
        // una frame completa de CPU.
        p.scanline = 0;
        p.dot = 0;
        p.frame_ready = false;
        p.nmi = false;
        con.cpu.nmi_pending = false;
        con.cpu.irq_line = false;
        p.frame_count = self.frame_count;
        p.sprite0_hit_at = self.sprite0_hit_at;
        p.frame.copy_from_slice(&self.frame);
    }
}

fn create_warmup_snapshot(rom: &[u8]) -> Snapshot {
    let cart = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut con = Console::new(cart);
    // DIAGNÓSTICO (bootdiag): sin input, el juego entra solo al DEMO del
    // título (op $0770 = 0x00, timer 4xx) donde el mando se IGNORA. Hay que
    // salir del demo con START y esperar al NIVEL REAL, cuya firma es:
    //   op == 0x01  &&  timer (centenas) == 3  &&  st == 0x08  &&  en el suelo
    // Pulsar START en bucle DESPUÉS de entrar al nivel PAUSA la partida
    // (Mario congelado en x=40, PC en el bucle de pausa) — eso era lo que
    // hacía que la IA "no se moviera".
    let mut in_level = false;
    for f in 0..2400usize {
        let x = (con.bus.ram[0x6D] as i32) * 256 + (con.bus.ram[0x86] as i32);
        let op = con.bus.ram[0x0770];
        let timer = bcd_byte(con.bus.ram[0x07F8]);
        let state = con.bus.ram[0x000E];
        let on_ground = con.bus.ram[0x001D] == 0;
        if op == 0x01 && (1..=3).contains(&timer) && state == 0x08 && x > 30 && on_ground {
            in_level = true;
            break;
        }
        // START solo mientras NO estemos en el nivel real (demo/título).
        let press = f % 30 < 2;
        con.set_buttons(0, if press { BUTTON_START } else { 0 });
        con.step_frame();
        con.bus.apu.samples.clear();
    }
    // Confirmar control real: avanzar con RIGHT|B. Si no se mueve, el juego
    // quedó en pausa: un toque de START lo despausa.
    let mut last_x = -1i32;
    let mut no_move = 0usize;
    for _ in 0..200usize {
        con.set_buttons(0, BUTTON_RIGHT | BUTTON_B);
        con.step_frame();
        con.bus.apu.samples.clear();
        let x = (con.bus.ram[0x6D] as i32) * 256 + (con.bus.ram[0x86] as i32);
        if x > last_x {
            no_move = 0;
            last_x = x;
        } else {
            no_move += 1;
        }
        if no_move >= 24 {
            con.set_buttons(0, BUTTON_START);
            con.step_frame();
            con.bus.apu.samples.clear();
            no_move = 0;
        }
        if x > 80 {
            break;
        }
    }
    if !in_level {
        eprintln!("AVISO: no se detectó el nivel real durante el warmup");
    }
    Snapshot::capture(&con)
}

// --------------------------------------------------------------------------- //
// RNG rápido (xorshift64* + Box-Muller + Cauchy)
// --------------------------------------------------------------------------- //
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(if seed == 0 { 0x9E3779B97F4A7C15 } else { seed })
    }
    #[inline]
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    #[inline]
    fn f32(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }
    #[inline]
    fn range(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    #[inline]
    fn gauss(&mut self) -> f32 {
        let u1 = self.f32().max(1e-7);
        let u2 = self.f32();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f32::consts::PI * u2).cos()
    }
    #[inline]
    fn cauchy(&mut self) -> f32 {
        let u = self.f32().clamp(0.01, 0.99);
        (std::f32::consts::PI * (u - 0.5)).tan()
    }
}

// --------------------------------------------------------------------------- //
// Red neuronal 120 -> 48 -> 7 (100% compatible con el JSON de Python/Rust)
// --------------------------------------------------------------------------- //
const NW: usize = OBS * HID + HID + HID * ACT + ACT; // 6151

#[derive(Clone)]
struct Brain {
    w: Vec<f32>,
}

impl Brain {
    fn random(rng: &mut Rng) -> Self {
        Brain {
            w: (0..NW).map(|_| rng.gauss() * 0.35).collect(),
        }
    }

    /// Crea un individuo inicial con un ligero sesgo hacia correr a la derecha
    /// y saltar cuando los sensores de pared/pozo/enemigo se activan.
    fn smart_prior(rng: &mut Rng) -> Self {
        let mut b = Self::random(rng);
        let b2_start = OBS * HID + HID + HID * ACT;
        // Sesgo inicial hacia RIGHT_B (acción 3) y RIGHT_AB (acción 4)
        b.w[b2_start + 3] += 0.6;
        b.w[b2_start + 4] += 0.25;
        b.w[b2_start + 6] -= 0.5; // desincentivar LEFT puro al inicio
        b
    }

    #[inline]
    fn act(&self, obs: &[f32; OBS]) -> usize {
        let (w1, rest) = self.w.split_at(OBS * HID);
        let (b1, rest) = rest.split_at(HID);
        let (w2, b2) = rest.split_at(HID * ACT);
        let mut h = [0f32; HID];
        h.copy_from_slice(b1);

        // Multiplicación cache-friendly (recorrido lineal sobre w1)
        for i in 0..OBS {
            let x = obs[i];
            if x == 0.0 {
                continue;
            }
            let row = &w1[i * HID..(i + 1) * HID];
            for j in 0..HID {
                h[j] += row[j] * x;
            }
        }
        for j in 0..HID {
            h[j] = h[j].tanh();
        }

        let mut best = 0usize;
        let mut bv = f32::NEG_INFINITY;
        for k in 0..ACT {
            let mut s = b2[k];
            for j in 0..HID {
                s += w2[j * ACT + k] * h[j];
            }
            if s > bv {
                bv = s;
                best = k;
            }
        }
        best
    }

    fn save(&self, path: &str, fitness: f32) {
        let nums: Vec<String> = self.w.iter().map(|v| format!("{v:.6}")).collect();
        let s = format!(
            "{{\"obs\":{OBS},\"hidden\":{HID},\"acts\":{ACT},\"fitness\":{fitness:.3},\"w\":[{}]}}\n",
            nums.join(",")
        );
        std::fs::write(path, s).expect("no pude escribir pesos");
    }

    fn load(path: &str) -> Brain {
        let s = std::fs::read_to_string(path).expect("no pude leer pesos");
        let a = s.find("\"w\":").map(|i| i + 4).expect("formato inválido");
        let a = s[a..].find('[').map(|i| a + i).expect("formato inválido");
        let b = s[a..].find(']').expect("formato inválido") + a;
        let body = &s[a + 1..b];
        let w: Vec<f32> = body
            .split(',')
            .filter(|p| !p.trim().is_empty())
            .map(|p| p.trim().parse().expect("número inválido"))
            .collect();
        assert_eq!(w.len(), NW, "se esperaban {NW} pesos, hay {}", w.len());
        Brain { w }
    }
}

// --------------------------------------------------------------------------- //
// Heurísticas de Colisión y Extracción de Estado (RAM + Tilemap exacto)
// --------------------------------------------------------------------------- //
struct Prev {
    x: i32,
    y: i32,
    vx: f32,
    vy: f32,
    facing: f32,
    og_avg: f32,
    best_x: i32,
    stuck: usize,
    prev_action: usize,
    prev_buttons: u8,
    jump_hold: usize,
    wall_dist: f32,
    gap_dist: f32,
    enemy_dist: f32,
}

impl Prev {
    fn fresh() -> Self {
        Prev {
            x: 40,
            y: 176,
            vx: 0.0,
            vy: 0.0,
            facing: 1.0,
            og_avg: 1.0,
            best_x: 40,
            stuck: 0,
            prev_action: 0,
            prev_buttons: 0,
            jump_hold: 0,
            wall_dist: 999.0,
            gap_dist: 999.0,
            enemy_dist: 999.0,
        }
    }
}

#[inline]
fn bcd_byte(b: u8) -> i32 {
    ((b >> 4) as i32) * 10 + (b & 0x0F) as i32
}

/// Consulta el Block Buffer de colisiones de SMB1 en RAM ($0500..$069F).
/// Devuelve: 0 = aire, 1 = bloque/tubería/suelo sólido, 2 = moneda.
#[inline]
fn world_tile_kind(ram: &[u8; 2048], wx: i32, wy: i32) -> u8 {
    if wx < 0 || wy < 32 || wy >= 240 {
        return 0;
    }
    let mt_x = (wx / 16) as usize;
    let page = (mt_x / 16) & 1;
    let col = mt_x & 15;
    let row = ((wy - 32) / 16) as usize;
    if row >= 13 {
        return 0;
    }
    let addr = 0x0500 + page * 208 + row * 16 + col;
    let t = ram[addr];
    if t == 0 {
        0
    } else if t == 0xC2 || t == 0xC3 {
        2 // Moneda
    } else {
        1 // Sólido (suelo, ladrillo, bloque ?, tubería, escalera)
    }
}

#[inline]
fn is_solid_world(ram: &[u8; 2048], wx: i32, wy: i32) -> bool {
    world_tile_kind(ram, wx, wy) == 1
}

/// Comprueba si la columna `wx` es un pozo mortal (sin suelo en las filas inferiores).
#[inline]
fn is_pit_column(ram: &[u8; 2048], wx: i32) -> bool {
    !is_solid_world(ram, wx, 208) && !is_solid_world(ram, wx, 224)
}

fn extract(con: &Console, prev: &mut Prev, steps: usize) -> [f32; OBS] {
    let ram = &con.bus.ram;
    let x = (ram[0x6D] as i32) * 256 + (ram[0x86] as i32);
    let ypix = ram[0x03B8] as i32;
    let lx = ((ram[0x86] as i32) - (ram[0x071C] as i32)).rem_euclid(256);

    // Velocidades reales de física en RAM ($0057 = vx signed, $009F = vy signed)
    let dx = x - prev.x;
    let dy = ypix - prev.y;
    prev.x = x;
    prev.y = ypix;

    let ram_vx = (ram[0x0057] as i8) as f32 / 16.0;
    let ram_vy = (ram[0x009F] as i8) as f32;
    // $0057 va en 1/16 px por frame y $009F en px por frame: los uso TAL CUAL.
    // Antes se mezclaban con dx/dy (medidos sobre `skip` frames) y la velocidad
    // salía 4x inflada, corrompiendo la predicción de aterrizaje.
    prev.vx = 0.6 * prev.vx + 0.4 * ram_vx;
    prev.vy = 0.6 * prev.vy + 0.4 * ram_vy;
    if dx != 0 {
        prev.facing = if dx > 0 { 1.0 } else { -1.0 };
    }

    // $001D == 0 significa que Mario pisa suelo firme en la física interna de la ROM
    let on_ground = if ram[0x001D] == 0x00 { 1.0 } else { 0.0 };
    prev.og_avg = 0.9 * prev.og_avg + 0.1 * on_ground;

    // NOTA: `prev.stuck` lo cuenta SOLO run_episode (por frame); aquí solo se
    // registra el marcador de progreso (antes se contaba dos veces y truncaba
    // episodios válidos a los 120 frames).

    let power = ram[0x0756];
    let feet = (ypix + if power == 0 { 32 } else { 48 }).clamp(32, 239);
    let front_x = x + 16; // Borde derecho del hitbox de Mario

    let mut o = [0f32; OBS];
    o[0] = x as f32 / WORLD_W as f32;
    o[1] = ypix as f32 / 240.0;
    o[2] = (prev.vx / 3.0).clamp(-1.0, 1.0);
    o[3] = (prev.vy / 6.0).clamp(-1.0, 1.0);
    o[4] = on_ground;
    o[5] = prev.facing;
    o[6] = (WORLD_W - x) as f32 / WORLD_W as f32;
    o[7] = steps as f32 / MAX_FRAMES as f32;

    // ---- Enemigos exactos desde los slots de RAM ($000F..$00CF) ---------- //
    // (wx, wy, vx, state, etype)
    let mut enemies: Vec<(i32, i32, f32, u8, u8)> = Vec::with_capacity(5);
    for slot in 0..5usize {
        if ram[0x000F + slot] == 0 {
            continue;
        }
        let etype = ram[0x0016 + slot];
        // Ignorar objetos de fin de nivel (bandera = 0x2D / 0x31)
        if etype == 0x2D || etype == 0x31 {
            continue;
        }
        let estate = ram[0x001E + slot];
        // Ignorar enemigos ya derrotados cayendo fuera de pantalla
        if estate >= 0x20 {
            continue;
        }
        let ewx = (ram[0x006E + slot] as i32) * 256 + (ram[0x0087 + slot] as i32);
        let ewy = ram[0x00CF + slot] as i32;
        let evx = (ram[0x0058 + slot] as i8) as f32 / 16.0;
        if ewx >= x - 20 && ewx <= x + 220 {
            enemies.push((ewx, ewy, evx, estate, etype));
        }
    }
    enemies.sort_by_key(|e| (e.0 - x).max(-32));

    for (placed, &(ewx, ewy, _evx, _estate, _etype)) in enemies.iter().take(3).enumerate() {
        o[8 + placed * 2] = (((ewx - x) as f32 / 16.0) / 14.0).clamp(-1.0, 1.0);
        o[9 + placed * 2] = (((ewy - ypix) as f32 / 16.0) / 12.0).clamp(-1.0, 1.0);
    }
    prev.enemy_dist = enemies
        .iter()
        .find(|e| e.0 >= x - 4)
        .map(|e| (e.0 - front_x).max(0) as f32)
        .unwrap_or(999.0);

    // Score, monedas y tiempo en BCD
    let score =
        bcd_byte(ram[0x07DE]) * 10000 + bcd_byte(ram[0x07DF]) * 100 + bcd_byte(ram[0x07E0]);
    let coins = bcd_byte(ram[0x07ED]) * 10 + bcd_byte(ram[0x07EE]);
    let time = bcd_byte(ram[0x07F8]) * 100 + bcd_byte(ram[0x07F9]) * 10 + bcd_byte(ram[0x07FA]);
    o[14] = (coins as f32 / 20.0).min(1.0);
    o[15] = (score as f32 / 5000.0).min(1.0);

    // ---- Rejilla 4x10 delante de Mario usando el Block Buffer real -------- //
    let mty = (feet / 16).clamp(2, 12);
    let mut idx = 16;
    let rows = [mty - 2, mty - 1, mty, (mty + 1).min(14)];
    let mut sol_count = 0usize;
    for &row in rows.iter() {
        let wy = (row * 16).clamp(32, 232);
        for k in 1..=10i32 {
            let wx = x + k * 16;
            let kind = world_tile_kind(ram, wx, wy);
            o[idx] = match kind {
                1 => {
                    sol_count += 1;
                    1.0
                }
                2 => 0.35, // moneda
                _ => 0.0,
            };
            idx += 1;
        }
    }

    // ---- Sensores de Muro (distancia + altura) y Pozo (distancia + ancho) - //
    let row_wall_low = (feet - 6).clamp(32, 236);
    let row_wall_mid = (feet - 18).clamp(32, 236);
    let mut wall_dist = 999.0f32;
    let mut wall_height = 0.0f32;

    for d in 0..=96i32 {
        let wx = front_x + d;
        if is_solid_world(ram, wx, row_wall_low) || is_solid_world(ram, wx, row_wall_mid) {
            wall_dist = d as f32;
            // Medir la altura del obstáculo hacia arriba (hasta 5 tiles)
            let mut h_px = 0;
            for step_y in (0..=80).step_by(8) {
                if is_solid_world(ram, wx, (feet - 4 - step_y).max(32)) {
                    h_px = step_y + 8;
                } else if h_px > 0 {
                    break;
                }
            }
            wall_height = h_px as f32;
            break;
        }
    }

    let mut gap_dist = 999.0f32;
    let mut gap_width = 0.0f32;
    for d in 0..=96i32 {
        let wx = front_x + d;
        let under_empty = !is_solid_world(ram, wx, (feet + 6).min(232));
        if under_empty && is_pit_column(ram, wx) {
            gap_dist = d as f32;
            let mut w_px = 0;
            for wd in (0..=80).step_by(4) {
                if is_pit_column(ram, wx + wd) {
                    w_px = wd + 4;
                } else {
                    break;
                }
            }
            gap_width = w_px as f32;
            break;
        }
    }

    prev.wall_dist = wall_dist;
    prev.gap_dist = gap_dist;

    o[56] = time as f32 / 400.0;
    o[57] = power as f32 / 2.0;
    // Sensores de ritmo y subpíxel (BUGS_Y_TRUCOS.md): la física de SMB
    // está cuantizada a 21 frames y la fracción persiste entre saltos.
    o[79] = ram[0x077F] as f32 / 20.0;
    o[84] = (ram[0x0705] as f32) / 256.0;
    o[58] = ram[0x000E] as f32 / 15.0;
    o[59] = ram[0x00B5] as f32 / 5.0;
    o[60] = lx as f32 / 256.0;
    o[61] = (x - lx) as f32 / WORLD_W as f32;
    o[62] = prev.prev_action as f32 / 6.0;
    o[63] = prev.og_avg;

    // ---- Slots 64..87: Vectores detallados de enemigos y velocidad relativa //
    for (n, &(ewx, ewy, evx, estate, etype)) in enemies.iter().take(5).enumerate() {
        let rel_x = (ewx - front_x) as f32;
        let rel_y = (ewy - ypix) as f32;
        let closing_v = (prev.vx - evx).max(0.25);
        let ttc = if rel_x >= -8.0 {
            (rel_x.max(0.0) / closing_v / 60.0).clamp(0.0, 1.0)
        } else {
            1.0
        };
        o[64 + n * 3] = (rel_x / 128.0).clamp(-1.0, 1.0);
        o[65 + n * 3] = (rel_y / 128.0).clamp(-1.0, 1.0);
        o[66 + n * 3] = 1.0 - ttc; // Urgencia de impacto (1.0 = inminente)
        o[80 + n] = (etype as f32 / 32.0).clamp(0.0, 1.0);
        o[85 + n.min(2)] = if estate != 0 { 1.0 } else { 0.0 }; // Caparazón / estado especial
    }

    o[88] = (wall_dist / 96.0).min(1.0);
    o[89] = (gap_dist / 96.0).min(1.0);
    o[90] = (prev.enemy_dist / 96.0).min(1.0);
    o[91] = (prev.stuck as f32 / 60.0).min(1.0);

    for i in 0..5 {
        o[92 + i] = if ram[0x000F + i] != 0 {
            (ram[0x0016 + i] as f32 / 64.0).min(1.0)
        } else {
            0.0
        };
    }
    o[97] = enemies.len() as f32 / 5.0;
    o[98] = (ram[0x075A] as f32).min(3.0) / 3.0;
    o[99] = (dx as f32 / 3.0).clamp(-1.0, 1.0);
    o[100] = (dy as f32 / 6.0).clamp(-1.0, 1.0);
    o[101] = (prev.vx.abs() / 3.0).clamp(0.0, 1.0);
    o[102] = if prev.vx * prev.facing > 0.0 { 1.0 } else { 0.0 };
    o[103] = if prev.vy > 0.0 { 1.0 } else { 0.0 };

    // Estado real de los botones pulsados en el paso anterior (corregido)
    o[104] = if prev.prev_buttons & BUTTON_A != 0 { 1.0 } else { 0.0 };
    o[105] = if prev.prev_buttons & BUTTON_B != 0 { 1.0 } else { 0.0 };
    o[106] = if prev.prev_buttons & BUTTON_LEFT != 0 { 1.0 } else { 0.0 };
    o[107] = if prev.prev_buttons & BUTTON_RIGHT != 0 { 1.0 } else { 0.0 };

    o[108] = sol_count as f32 / 40.0;
    o[109] = (gap_width / 64.0).min(1.0); // Ancho real del pozo
    o[110] = (x % 16) as f32 / 16.0;
    o[111] = (ypix % 16) as f32 / 16.0;

    // Techo libre sobre la cabeza de Mario (importante para no chocar con ladrillos bajos)
    let mut head_clear = 4.0f32;
    for t in 1..=4i32 {
        if is_solid_world(ram, x + 8, ypix - t * 16) {
            head_clear = (t - 1) as f32;
            break;
        }
    }
    o[112] = head_clear / 4.0;
    o[113] = (wall_height / 64.0).min(1.0); // Altura real del muro/tubería frontal

    // ---- Predicción balística: DÓNDE SALTAR y DÓNDE VA A CAER ----------- //
    // Arco medido en el emulador (jumptest): v0 = 5.0 px/frame y gravedad
    // ≈ 0.15 px/frame² con A mantenido → salto de 83 px (~66 frames).
    const V0: f32 = 5.0;
    const G: f32 = 0.15;
    let vx_eff = prev.vx.max(0.4);

    // Distancia horizontal que recorre Mario hasta que sus pies superan `h`.
    let rise_dist = |h: f32| -> f32 {
        let mut y = 0.0f32;
        let mut vy = V0;
        for t in 0..40 {
            y += vy;
            vy -= G;
            if y >= h {
                return vx_eff * (t as f32 + 1.0);
            }
            if vy < 0.0 {
                break;
            }
        }
        vx_eff * 40.0
    };

    // Muro/tubería: margen hasta el punto PERFECTO de despegue.
    // >0 = aún sobran píxeles; ≈0 = hay que saltar YA; <0 = llega tarde.
    let need_wall = rise_dist(wall_height.max(16.0));
    let wall_margin = if wall_dist < 96.0 {
        wall_dist - need_wall
    } else {
        99.0
    };
    o[114] = (wall_margin / 32.0).clamp(-1.0, 1.0);
    o[115] = if wall_dist < 96.0 && (-20.0..=6.0).contains(&wall_margin) {
        1.0 // ventana de salto: salta AHORA
    } else {
        0.0
    };

    // Pozo: sólo hay que ir un poco alto para cruzar el borde.
    let need_gap = rise_dist(10.0);
    let gap_margin = if gap_dist < 96.0 {
        gap_dist - need_gap
    } else {
        99.0
    };
    o[116] = (gap_margin / 32.0).clamp(-1.0, 1.0);

    // Aterrizaje si salta AHORA (vuelo completo del salto comprometido).
    let flight = (2.0 * V0 / G).min(70.0);
    let landing_x = x + 16 + (vx_eff * flight) as i32;
    let landing_in_pit = is_pit_column(ram, landing_x);
    let landing_on_enemy = enemies.iter().any(|e| (e.0 - landing_x).abs() <= 14);
    o[117] = ((landing_x - x) as f32 / 160.0).clamp(0.0, 1.0);
    o[118] = if landing_in_pit { 1.0 } else { 0.0 };
    o[119] = if landing_on_enemy { 1.0 } else { 0.0 };
    o
}

#[inline]
fn is_stage_over(ram: &[u8; 2048]) -> bool {
    // Gana solo al tocar el mástil (estado 3 = bajando por el mástil).
    // NO usar los tipos 0x2D/0x31: la bandera existe desde que el mástil
    // aparece en pantalla (~12 bloques antes) y eso premiaría llegar a penar.
    ram[0x001D] == 0x03
}

#[inline]
fn is_dead(ram: &[u8; 2048]) -> bool {
    ram[0x000E] == 0x06 || ram[0x000E] == 0x0B || ram[0x00B5] > 1
}

/// Calcula el fitness priorizando Esfuerzo vs. Tiempo (progreso + hitos + velocidad).
fn compute_fitness(x_max: i32, x_max_frame: usize, score: i32, frames: usize, won: bool) -> f32 {
    let tiles = x_max as f32 / 16.0;
    // Velocidad media hasta alcanzar el récord (no hasta morir o atascars�se),
    // as� el fitness no premia quedarse quieto.
    let avg_speed = x_max as f32 / (x_max_frame.max(30) as f32);

    // Bonificaciones por superar obstáculos clave de 1-1 (tuberías altas, pozos y escaleras)
    const MILESTONES: [i32; 8] = [480, 640, 768, 944, 1152, 1424, 2200, 2950];
    let milestone_bonus: f32 = MILESTONES
        .iter()
        .filter(|&&m| x_max >= m)
        .count() as f32
        * 45.0;

    if won {
        // Si toca la bandera: recompensa masiva + fuerte bonus por hacerlo en menos frames
        10_000.0 + (MAX_FRAMES.saturating_sub(frames)) as f32 * 3.0 + score as f32 * 0.05
    } else {
        tiles * 18.0 + milestone_bonus + avg_speed * 60.0 + score as f32 * 0.02
    }
}

/// Ejecuta un episodio con Snapshot instantáneo, Frame Skipping e interrupción al aterrizar.
fn run_episode(
    con: &mut Console,
    snap: &Snapshot,
    brain: &Brain,
    frame_skip: usize,
    frame_limit: usize,
) -> (f32, f32, i32, usize, bool) {
    snap.restore(con);

    let mut prev = Prev::fresh();
    // x de arranque real (el warmup ya avanza a Mario hasta ~80 px).
    let start_x = (con.bus.ram[0x6D] as i32) * 256 + (con.bus.ram[0x86] as i32);
    let mut x_max: i32 = start_x;
    let mut x_max_frame: usize = 0;
    let mut won = false;
    let mut frames = 0usize;
    let skip = frame_skip.max(1);
    let mut jump_lock: i32 = 0;
    let mut airborne = false;

    while frames < frame_limit {
        let obs = extract(con, &mut prev, frames);
        let action = brain.act(&obs);
        let desired_buttons = buttons_for(action);
        prev.prev_action = action;

        let was_airborne = con.bus.ram[0x001D] != 0;

        for sub in 0..skip {
            if frames >= frame_limit {
                break;
            }
            let on_ground_now = con.bus.ram[0x001D] == 0;

            // ---- SALTO COMPROMETIDO ------------------------------------- //
            // Si Mario está en el aire, `airborne` queda armado. Al volver al
            // suelo se libera el compromiso.
            if !on_ground_now {
                airborne = true;
            } else if airborne {
                airborne = false;
                jump_lock = 0;
            }
            let want_a = desired_buttons & BUTTON_A != 0;
            // A se mantiene SIEMPRE durante el compromiso; soltarlo un frame
            // al arrancar hace que la ROM use gravedad alta (~28 px de salto).
            let mut a_on = jump_lock > 0;
            if !a_on && want_a {
                if on_ground_now && (prev.prev_buttons & BUTTON_A != 0) {
                    a_on = false; // rearmar flanco
                } else {
                    a_on = true;
                    if on_ground_now {
                        jump_lock = JUMP_HOLD;
                    }
                }
            }
            let mut hw_buttons = desired_buttons & !BUTTON_A;
            if a_on {
                hw_buttons |= BUTTON_A;
            }
            if jump_lock > 0 && airborne {
                jump_lock -= 1;
            }

            prev.prev_buttons = hw_buttons;
            if hw_buttons & BUTTON_A != 0 {
                prev.jump_hold += 1;
            } else {
                prev.jump_hold = 0;
            }

            con.set_buttons(0, hw_buttons);
            con.step_frame();
            con.bus.apu.samples.clear(); // Evita acumular audio en RAM
            frames += 1;

            let x_now = (con.bus.ram[0x6D] as i32) * 256 + (con.bus.ram[0x86] as i32);
            if x_now > x_max {
                x_max = x_now;
                x_max_frame = frames;
                prev.stuck = 0;
            } else {
                prev.stuck += 1;
            }

            if is_stage_over(&con.bus.ram) {
                won = true;
                break;
            }
            if is_dead(&con.bus.ram) || (prev.x > 60 && x_now < prev.x - 100) {
                frames = frame_limit; // Salir de ambos bucles
                break;
            }

            // Si acaba de aterrizar en mitad de un frame-skip, devolver el control
            // a la red neuronal inmediatamente para no perder ventana de reacción
            if sub > 0 && was_airborne && con.bus.ram[0x001D] == 0 {
                break;
            }
        }

        // Corte temprano por estancamiento (Esfuerzo vs. Tiempo)
        if won || prev.stuck >= STUCK_TIMEOUT {
            break;
        }
    }

    let ram = &con.bus.ram;
    let score =
        bcd_byte(ram[0x07DE]) * 10000 + bcd_byte(ram[0x07DF]) * 100 + bcd_byte(ram[0x07E0]);
    let tiles = x_max as f32 / 16.0;
    let fitness = compute_fitness(x_max, x_max_frame, score, frames, won);
    (fitness, tiles, score, frames, won)
}

// --------------------------------------------------------------------------- //
// CLI Principal
// --------------------------------------------------------------------------- //
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("train");
    let get = |name: &str, def: &str| -> String {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
            .unwrap_or_else(|| def.to_string())
    };

    let skip: usize = get("--skip", &DEFAULT_SKIP.to_string())
        .parse()
        .unwrap_or(DEFAULT_SKIP);

    match cmd {
        "train" => {
            let gens: usize = get("--gens", "300").parse().unwrap_or(300);
            let pop: usize = get("--pop", "64").parse().unwrap_or(64);
            let seed: u64 = get("--seed", "42").parse().unwrap_or(42);
            let seed_file = get("--seed-file", "");
            let out = get("--out", "best_ai.json");
            let threads: usize = get("--threads", "0").parse().unwrap_or(0);
            let rom = get("--rom", ROM_DEFAULT);
            let seed_file = if seed_file.is_empty() {
                None
            } else {
                Some(seed_file.as_str())
            };
            train_parallel(&rom, gens, pop, seed, skip, seed_file, &out, threads);
        }
        "eval" => {
            let rom = get("--rom", ROM_DEFAULT);
            let weights = get("--weights", "best_ai.json");
            let raw = std::fs::read(&rom).expect("no se pudo leer la ROM");
            let snap = create_warmup_snapshot(&raw);
            let cart = Cartridge::from_bytes(&raw).expect("cartucho inválido");
            let mut con = Console::new(cart);
            let brain = Brain::load(&weights);
            let (f, tiles, score, frames, won) =
                run_episode(&mut con, &snap, &brain, skip, MAX_FRAMES);
            println!(
                "fitness={f:.2}  x_max={tiles:.1} tiles  score={score}  frames={frames}  skip={skip}  {}",
                if won { "¡BANDERA!" } else { "sin bandera" }
            );
        }
        "watch" => {
            let rom = get("--rom", ROM_DEFAULT);
            let weights = get("--weights", "best_ai.json");
            watch(&rom, &weights, skip);
        }
        "snaptest" => {
            let rom = get("--rom", ROM_DEFAULT);
            let raw = std::fs::read(&rom).expect("no se pudo leer la ROM");
            let snap = create_warmup_snapshot(&raw);
            let cart2 = Cartridge::from_bytes(&raw).expect("cartucho inválido");
            let mut con2 = Console::new(cart2);
            snap.restore(&mut con2);
            println!(
                "RESTORE x={} st={:02X} t={:02X} pc={:04X}",
                (con2.bus.ram[0x6D] as i32) * 256 + (con2.bus.ram[0x86] as i32),
                con2.bus.ram[0x000E],
                con2.bus.ram[0x07F8],
                con2.cpu.pc
            );
            for f in 0..300usize {
                con2.set_buttons(0, BUTTON_RIGHT | BUTTON_B);
                con2.step_frame();
                con2.bus.apu.samples.clear();
                if f % 50 == 0 {
                    let x = (con2.bus.ram[0x6D] as i32) * 256 + (con2.bus.ram[0x86] as i32);
                    println!("  RESTORE f={f} x={x} pc={:04X}", con2.cpu.pc);
                }
            }
        }
        "pipetest" => {
            // Prueba determinista de percepción + salto: política reactiva
            // "si hay muro cerca, salta" usando EXACTAMENTE los sensores de
            // extract() y el salto comprometido de run_episode.
            let rom = get("--rom", ROM_DEFAULT);
            let raw = std::fs::read(&rom).expect("no se pudo leer la ROM");
            let snap = create_warmup_snapshot(&raw);
            let cart = Cartridge::from_bytes(&raw).expect("cartucho inválido");
            let mut con = Console::new(cart);
            snap.restore(&mut con);
            let mut prev = Prev::fresh();
            let mut jump_lock: i32 = 0;
            let mut airborne = false;
            for f in 0..1400usize {
                let obs = extract(&con, &mut prev, f);
                let wall = obs[88] * 96.0;
                let enemy = obs[90] * 96.0;
                let grounded = con.bus.ram[0x001D] == 0;
                // 2 = RIGHT+A (saltar), 3 = RIGHT+B (correr)
                let action = if obs[115] > 0.5 || enemy < 40.0 { 2 } else { 3 };

                if !grounded {
                    airborne = true;
                } else if airborne {
                    airborne = false;
                    jump_lock = 0;
                }
                let want_a = buttons_for(action) & BUTTON_A != 0;
                // A se mantiene SIEMPRE durante el compromiso; soltarlo un
                // frame al arrancar hace que la ROM use gravedad alta y el
                // salto se quede en ~28 px (no pasa el tubo).
                let mut a_on = jump_lock > 0;
                if !a_on && want_a {
                    if grounded && (prev.prev_buttons & BUTTON_A != 0) {
                        a_on = false; // rearmar flanco
                    } else {
                        a_on = true;
                        if grounded {
                            jump_lock = JUMP_HOLD;
                        }
                    }
                }
                let mut hw = buttons_for(action) & !BUTTON_A;
                if a_on {
                    hw |= BUTTON_A;
                }
                if jump_lock > 0 && airborne {
                    jump_lock -= 1;
                }
                prev.prev_buttons = hw;
                prev.prev_action = action;
                con.set_buttons(0, hw);
                con.step_frame();
                con.bus.apu.samples.clear();
                if f % 40 == 0 {
                    let x = (con.bus.ram[0x6D] as i32) * 256 + (con.bus.ram[0x86] as i32);
                    println!(
                        "f={f:4} x={x:4} ({:.1}t) y={:3} vy={:4} wall={wall:5.1} h={:.0} g={} st={:02X}",
                        x as f32 / 16.0,
                        con.bus.ram[0x03B8],
                        (con.bus.ram[0x009F] as i8),
                        obs[113] * 64.0,
                        if grounded { 1 } else { 0 },
                        con.bus.ram[0x000E]
                    );
                }
            }
        }
        "jumptest" => {
            let rom = get("--rom", ROM_DEFAULT);
            let raw = std::fs::read(&rom).expect("no se pudo leer la ROM");
            let snap = create_warmup_snapshot(&raw);
            for hold in [60usize, 26, 10, 3] {
                let cart = Cartridge::from_bytes(&raw).expect("cartucho inválido");
                let mut con = Console::new(cart);
                snap.restore(&mut con);
                let y0 = con.bus.ram[0x03B8];
                let mut peak = y0;
                for f in 0..90usize {
                    let mut b = BUTTON_RIGHT;
                    if f < hold {
                        b |= BUTTON_A;
                    }
                    con.set_buttons(0, b);
                    con.step_frame();
                    con.bus.apu.samples.clear();
                    let y = con.bus.ram[0x03B8];
                    peak = peak.min(y);
                }
                println!(
                    "A durante {hold:2} frames -> salto = {} px (y {} -> {})",
                    y0 - peak,
                    y0,
                    peak
                );
            }
        }
        other => {
            eprintln!("comando desconocido: {other} (usa train, eval, watch o snaptest)");
            std::process::exit(2);
        }
    }
}

fn watch(rom_path: &str, weights: &str, skip: usize) {
    let raw = std::fs::read(rom_path).expect("no se pudo leer la ROM");
    let snap = create_warmup_snapshot(&raw);
    let brain = Brain::load(weights);
    let cart = Cartridge::from_bytes(&raw).expect("cartucho inválido");
    let mut con = Console::new(cart);
    snap.restore(&mut con);

    let mut prev = Prev::fresh();
    let mut win = minifb::Window::new(
        &format!("SMB Rust AI — {}", weights),
        256,
        240,
        minifb::WindowOptions {
            scale: minifb::Scale::X4,
            resize: true,
            ..Default::default()
        },
    )
    .expect("ventana");
    win.set_target_fps(60);

    let mut frame_no = 0usize;
    let mut paused = false;
    let mut current_action = 0usize;
    let skip = skip.max(1);

    while win.is_open() && !win.is_key_down(minifb::Key::Escape) {
        if win.is_key_pressed(minifb::Key::Space, minifb::KeyRepeat::No) {
            paused = !paused;
        }
        if win.is_key_pressed(minifb::Key::R, minifb::KeyRepeat::No) {
            snap.restore(&mut con);
            prev = Prev::fresh();
            frame_no = 0;
        }
        if paused {
            win.update_with_buffer(con.framebuffer(), 256, 240).unwrap();
            continue;
        }

        if frame_no % skip == 0 || con.bus.ram[0x001D] == 0 {
            let obs = extract(&con, &mut prev, frame_no);
            current_action = brain.act(&obs);
            prev.prev_action = current_action;
        }

        let on_ground_now = con.bus.ram[0x001D] == 0;
        let mut hw_buttons = buttons_for(current_action);
        if on_ground_now
            && (hw_buttons & BUTTON_A != 0)
            && (prev.prev_buttons & BUTTON_A != 0)
        {
            hw_buttons &= !BUTTON_A;
        }
        prev.prev_buttons = hw_buttons;
        if hw_buttons & BUTTON_A != 0 {
            prev.jump_hold += 1;
        } else {
            prev.jump_hold = 0;
        }

        con.set_buttons(0, hw_buttons);
        con.step_frame();
        con.bus.apu.samples.clear();
        frame_no += 1;

        if frame_no % 15 == 0 {
            let x = (con.bus.ram[0x6D] as i32) * 256 + (con.bus.ram[0x86] as i32);
            win.set_title(&format!(
                "SMB AI | tile={:.1} | f={} | act={} | [Space]=Pausa [R]=Reset",
                x as f32 / 16.0,
                frame_no,
                current_action
            ));
        }

        if is_dead(&con.bus.ram) || is_stage_over(&con.bus.ram) || prev.stuck > 240 {
            snap.restore(&mut con);
            prev = Prev::fresh();
            frame_no = 0;
        }

        win.update_with_buffer(con.framebuffer(), 256, 240).unwrap();
    }
}

// --------------------------------------------------------------------------- //
// Entrenamiento Neuroevolutivo Paralelo (Elitismo + Mutación Adaptativa)
// --------------------------------------------------------------------------- //
fn mutate_weights(w: &mut [f32], rng: &mut Rng, rate: f32, sigma: f32) {
    for v in w.iter_mut() {
        if rng.f32() < rate {
            // 8% de las mutaciones usan cola pesada (Cauchy acotada) para escapar de óptimos locales
            let delta = if rng.f32() < 0.08 {
                rng.cauchy().clamp(-4.0, 4.0) * sigma * 0.5
            } else {
                rng.gauss() * sigma
            };
            *v = (*v + delta).clamp(-6.0, 6.0);
        }
    }
}

fn train_parallel(
    rom_path: &str,
    gens: usize,
    pop: usize,
    seed: u64,
    skip: usize,
    seed_file: Option<&str>,
    out: &str,
    threads: usize,
) {
    let raw = std::fs::read(rom_path).expect("no se pudo leer la ROM");
    let snap = Arc::new(create_warmup_snapshot(&raw));
    let rom = Arc::new(raw);
    let mut rng = Rng::new(seed);

    let mut brains: Vec<Brain> = Vec::with_capacity(pop);
    if let Some(sf) = seed_file {
        if std::path::Path::new(sf).exists() {
            let loaded = Brain::load(sf);
            brains.push(loaded.clone());
            // Sembrar variantes cercanas a la semilla cargada
            for _ in 0..(pop / 4).max(2) {
                let mut c = loaded.clone();
                mutate_weights(&mut c.w, &mut rng, 0.05, 0.10);
                brains.push(c);
            }
        }
    }
    while brains.len() < pop / 2 {
        brains.push(Brain::smart_prior(&mut rng));
    }
    while brains.len() < pop {
        brains.push(Brain::random(&mut rng));
    }

    let nthreads = if threads > 0 {
        threads
    } else {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    };
    let chunk = (pop + nthreads - 1) / nthreads;
    println!(
        "rom={rom_path}\ngens={gens} pop={pop} skip={skip} hilos={nthreads} semilla={seed} -> {out}"
    );

    let mut best_fit = f32::NEG_INFINITY;
    let mut best_brain = brains[0].clone();
    let mut best_tiles_ever = 0.0f32;
    let mut stagnation = 0usize;
    let mut hall: Vec<(f32, Brain)> = Vec::with_capacity(3);
    let mut best_flag_brain: Option<Brain> = None;
    let mut best_time_flag: usize = usize::MAX;
    let t0 = std::time::Instant::now();
    let mut total_frames_sim = 0usize;

    for gen in 1..=gens {
        // Horizonte dinámico: escala con el máximo avance alcanzado hasta MAX_FRAMES
        let frame_limit = ((450.0 + best_tiles_ever * 16.0) as usize).clamp(600, MAX_FRAMES);

        let mut results: Vec<(f32, f32, i32, usize, bool)> = Vec::with_capacity(pop);
        let brains_ref = &brains;
        std::thread::scope(|s| {
            let mut handles = Vec::new();
            for c in brains_ref.chunks(chunk) {
                let rom_ref = rom.clone();
                let snap_ref = snap.clone();
                handles.push(s.spawn(move || {
                    // Un único Console reutilizado por hilo worker
                    let cart = Cartridge::from_bytes(&rom_ref).expect("cartucho inválido");
                    let mut con = Console::new(cart);
                    c.iter()
                        .map(|b| run_episode(&mut con, &snap_ref, b, skip, frame_limit))
                        .collect::<Vec<_>>()
                }));
            }
            for h in handles {
                results.extend(h.join().unwrap());
            }
        });

        let mut order: Vec<usize> = (0..pop).collect();
        order.sort_by(|&a, &b| results[b].0.partial_cmp(&results[a].0).unwrap());

        let (bf, bx, bscore, bframes, bwon) = results[order[0]];
        best_tiles_ever = best_tiles_ever.max(bx);

        // Actualizar Hall of Fame con los 3 mejores de esta generación.
        for &i in order.iter().take(3) {
            let b = brains[i].clone();
            if b.w == best_brain.w {
                continue;
            }
            if !hall.iter().any(|h| h.1.w == b.w) {
                hall.push((results[i].0, b));
            }
        }
        hall.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        hall.truncate(3);

        let improved = bf > best_fit;
        if improved {
            best_fit = bf;
            best_brain = brains[order[0]].clone();
            best_brain.save(out, best_fit);
            stagnation = 0;
        } else {
            stagnation += 1;
        }

        // Si esta generación tocó la bandera, guardar el cerebro y el tiempo.
        if bwon {
            if bframes < best_time_flag {
                best_time_flag = bframes;
                best_flag_brain = Some(brains[order[0]].clone());
                brains[order[0]].save("best_flag.json", bf);
            }
        }

        let gen_frames: usize = results.iter().map(|r| r.3).sum();
        total_frames_sim += gen_frames;
        let mean: f32 = results.iter().map(|r| r.0).sum::<f32>() / pop as f32;
        let wins = results.iter().filter(|r| r.4).count();

        if gen % 5 == 0 || gen == 1 || improved {
            let elapsed = t0.elapsed().as_secs_f64();
            println!(
                "gen {gen:>4}  best={best_fit:8.1}  gen={bf:8.1}  mean={mean:7.1}  \
                 x={bx:5.1}t  score={bscore:<4}  f={bframes:<4}  {}  wins={}/{}  \
                 MEJOR_TIEMPO={}  {:.0} fps  {:.1}s{}",
                if bwon { "BANDERA" } else { "en_ruta" },
                wins,
                pop,
                if best_time_flag == usize::MAX {
                    "-".to_string()
                } else {
                    format!("{}", best_time_flag)
                },
                total_frames_sim as f64 / elapsed.max(0.001),
                elapsed,
                if improved { " *" } else { "" }
            );
        }

        // ---- Construcción de la siguiente generación --------------------- //
        let elite = (pop / 4).max(4);
        let mut next: Vec<Brain> = Vec::with_capacity(pop);

        // 0. Campeón de bandera (nunca se muta, nunca se pierde): si algún
        //    cerebro ha tocado la bandera, se conserva intacto para siempre.
        if let Some(bf) = &best_flag_brain {
            if !next.iter().any(|n| n.w == bf.w) {
                next.push(bf.clone());
            }
        }

        // 1. Elitismo puro: conservar al campeón global y los mejores.
        next.push(best_brain.clone());
        for &i in order.iter().take(elite) {
            if next.len() >= pop {
                break;
            }
            if brains[i].w == best_brain.w {
                continue;
            }
            next.push(brains[i].clone());
        }

        // 1b. Hall of Fame: conservar los 3 mejores cerebros de toda la historia
        //     (evita el "olvido" cuando la mutación degrada al campeón).
        for h in hall.iter() {
            if next.len() >= pop {
                break;
            }
            if h.1.w == best_brain.w || next.iter().any(|n| n.w == h.1.w) {
                continue;
            }
            next.push(h.1.clone());
        }

        // 1c. Aprendizaje cruzado entre instancias: leer cerebros guardados
        //     en disco por otros entrenadores (best_ai*.json, best_seed.json)
        //     e inyectar el mejor que no esté ya en la población.
        for cand_path in ["best_ai1.json", "best_ai2.json", "best_seed.json"] {
            if next.len() >= pop {
                break;
            }
            if !std::path::Path::new(cand_path).exists() {
                continue;
            }
            let b = Brain::load(cand_path);
            if b.w != best_brain.w
                && !next.iter().any(|n| n.w == b.w)
                && !hall.iter().any(|h| h.1.w == b.w)
            {
                next.push(b);
            }
        }

        // 2. Clones de ajuste fino (Fine Hill-Climbing sobre el campeón actual)
        //    Muchas copias con mutación diminuta para NO olvidar el movimiento
        //    que hizo ir más rápido a la bandera.
        for &(rate, sigma) in &[
            (0.005, 0.01),
            (0.01, 0.02),
            (0.02, 0.04),
            (0.04, 0.08),
            (0.08, 0.15),
        ] {
            for _ in 0..2 {
                if next.len() >= pop {
                    break;
                }
                let mut child = best_brain.clone();
                mutate_weights(&mut child.w, &mut rng, rate, sigma);
                next.push(child);
            }
        }

        // 3. Adaptación del radio de exploración según estancamiento
        let stag_boost: f32 = if stagnation > 12 {
            1.8
        } else if stagnation > 6 {
            1.35
        } else {
            1.0
        };

        // 4. Torneo + Cruce por neurona/uniforme + Mutación multiescala
        while next.len() < pop {
            // Si hay mucho estancamiento, inyectar un 10% de inmigrantes frescos
            if stagnation > 15 && rng.f32() < 0.10 {
                next.push(Brain::smart_prior(&mut rng));
                continue;
            }

            let pick = |rng: &mut Rng| -> usize {
                let mut bi = rng.range(pop);
                for _ in 0..3 {
                    let c = rng.range(pop);
                    if results[c].0 > results[bi].0 {
                        bi = c;
                    }
                }
                bi
            };
            let pa = &brains[pick(&mut rng)];
            let pb = &brains[pick(&mut rng)];

            let mut child = pa.w.clone();
            if rng.f32() < 0.75 {
                // Cruce aritmético interpolado (BLX) o uniforme
                let blend = rng.f32() < 0.4;
                for i in 0..NW {
                    if blend {
                        let alpha = rng.f32();
                        child[i] = alpha * pa.w[i] + (1.0 - alpha) * pb.w[i];
                    } else if rng.f32() < 0.5 {
                        child[i] = pb.w[i];
                    }
                }
            }

            let rate = (0.10 * stag_boost).min(0.35);
            let sigma = 0.18 * stag_boost;
            mutate_weights(&mut child, &mut rng, rate, sigma);
            next.push(Brain { w: child });
        }

        brains = next;
    }

    println!("guardado {out} (fitness={best_fit:.2})");
}
