//! Ventana multinstancia en cuadrícula 2D: ejecuta varios emuladores NES en
//! paralelo dentro de la misma ventana, cada uno con su propio cerebro.
//!
//! Mejoras clave:
//!   1. Paridad 100% exacta de los 120 sensores y frame-skipping con `train.rs`.
//!   2. Reinicio instantáneo por Snapshot en RAM (0 tirones al morir/ganar).
//!   3. Cuadrícula 2D automática + HUD pixel-art integrado en cada subpantalla
//!      (barra de progreso, récord, victorias, fitness y borde dorado al líder).
//!   4. Hot-reload cada segundo incluso si el archivo JSON aún no existía al abrir.
//!   5. Controles en vivo: [R] Carrera sincronizada, [Tab] Turbo 1x/2x/4x,
//!      [M] Modo carrera por rondas, [Espacio] Pausa.
//!
//! Uso:
//!   cargo run --release --bin watchall -- \
//!       --weights best_ai1.json best_ai2.json best_ai3.json best_ai4.json

use smb_rs::{
    Cartridge, Console, BUTTON_A, BUTTON_B, BUTTON_LEFT, BUTTON_RIGHT, BUTTON_START,
};
use std::time::SystemTime;

const W: usize = 256;
const H: usize = 240;
const OBS: usize = 120;
const HID: usize = 48;
const ACT: usize = 7;

// --- Apex GRRN-v5 (Colab) ---
const OBS_V5: usize = 120;
const MEM_V5: usize = 32;
const IN_DIM_V5: usize = OBS_V5 + MEM_V5 + 7; // 159
const H1_V5: usize = 128;
const H2_V5: usize = 128;
const OUT_DIM_V5: usize = 7 + MEM_V5; // 39
const NW_V5: usize = IN_DIM_V5 * H1_V5 + H1_V5 + H1_V5
                   + H1_V5 * H2_V5 + H2_V5 + H2_V5
                   + H2_V5 * OUT_DIM_V5 + OUT_DIM_V5;
const NW: usize = OBS * HID + HID + HID * ACT + ACT;
const WORLD_W: i32 = 212 * 16;
const MAX_FRAMES: usize = 3000;
const DEFAULT_SKIP: usize = 4;
/// Frames que se fuerza A tras decidir "saltar" (mismo valor que train.rs).
const JUMP_HOLD: i32 = 26;
const STUCK_TIMEOUT: usize = 180;

const ROM_DEFAULT: &str =
    "C:/Users/USUARIO/Downloads/Super Mario Bros. (World)/Super Mario Bros. (World).nes";

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
// Snapshot instantáneo del emulador tras pasar la pantalla de título
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
        p.scanline = self.scanline;
        p.dot = self.dot;
        p.frame_ready = self.frame_ready;
        p.nmi = self.nmi;
        p.frame_count = self.frame_count;
        p.sprite0_hit_at = self.sprite0_hit_at;
        p.frame.copy_from_slice(&self.frame);
    }
}

fn create_warmup_snapshot(rom: &[u8]) -> Snapshot {
    let cart = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut con = Console::new(cart);
    // Sin input el juego entra solo al DEMO del título (op=0, timer 4xx) donde
    // el mando se ignora. Salimos con START y esperamos al NIVEL REAL:
    // op==1 && timer(centenas)==3 && st==8 && en el suelo. Pulsar START en
    // bucle dentro del nivel PAUSA la partida (Mario congelado en x=40).
    for f in 0..2400usize {
        let x = (con.bus.ram[0x6D] as i32) * 256 + (con.bus.ram[0x86] as i32);
        let op = con.bus.ram[0x0770];
        let timer = bcd_byte(con.bus.ram[0x07F8]);
        let state = con.bus.ram[0x000E];
        let on_ground = con.bus.ram[0x001D] == 0;
        if op == 0x01 && (1..=3).contains(&timer) && state == 0x08 && x > 30 && on_ground {
            break;
        }
        let press = f % 30 < 2;
        con.set_buttons(0, if press { BUTTON_START } else { 0 });
        con.step_frame();
        con.bus.apu.samples.clear();
    }
    // Confirmar control real (y despausar si quedó en pausa).
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
    Snapshot::capture(&con)
}

// --------------------------------------------------------------------------- //
// Red neuronal 120 -> 48 -> 7 (con lectura de fitness y fallback inicial)
// --------------------------------------------------------------------------- //
struct Brain {
    w: Vec<f32>,
    fitness: f32,
}

impl Brain {
    fn fallback(seed: u64) -> Self {
        let mut s = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
        let mut w = Vec::with_capacity(NW);
        for _ in 0..NW {
            s ^= s >> 12;
            s ^= s << 25;
            s ^= s >> 27;
            let u = ((s.wrapping_mul(0x2545F4914F6CDD1D) >> 40) as f32) / ((1u64 << 24) as f32);
            w.push((u - 0.5) * 0.4);
        }
        let b2_start = OBS * HID + HID + HID * ACT;
        w[b2_start + 3] += 0.6; // sesgo hacia RIGHT_B
        w[b2_start + 4] += 0.2;
        Brain { w, fitness: 0.0 }
    }

    fn load(path: &str) -> Option<Brain> {
        let s = std::fs::read_to_string(path).ok()?;
        let fitness = s
            .find(r#""fitness":"#)
            .and_then(|i| {
                let rest = &s[i + 10..];
                let end = rest.find(|c: char| c == ',' || c == '}')?;
                rest[..end].trim().parse::<f32>().ok()
            })
            .unwrap_or(0.0);

        let a = s.find(r#""w":"#).map(|i| i + 4)?;
        let a = s[a..].find('[').map(|i| a + i)?;
        let b = s[a..].find(']').map(|i| a + i)?;
        let body = &s[a + 1..b];
        let w: Vec<f32> = body
            .split(',')
            .filter(|p| !p.trim().is_empty())
            .map(|p| p.trim().parse::<f32>().unwrap_or(0.0))
            .collect();
        (w.len() == NW).then_some(Brain { w, fitness })
    }

    #[inline]
    fn act(&self, obs: &[f32; OBS]) -> usize {
        let (w1, rest) = self.w.split_at(OBS * HID);
        let (b1, rest) = rest.split_at(HID);
        let (w2, b2) = rest.split_at(HID * ACT);
        let mut h = [0f32; HID];
        h.copy_from_slice(b1);
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
}

// --------------------------------------------------------------------------- //
// Sensores idénticos a train.rs (120 entradas completas)
// --------------------------------------------------------------------------- //

enum BrainKind { Old(Brain), V5(BrainV5) }

struct BrainV5 {
    w1: Vec<f32>, b1: Vec<f32>, g1: Vec<f32>,
    w2: Vec<f32>, b2: Vec<f32>, g2: Vec<f32>,
    w3: Vec<f32>, b3: Vec<f32>,
    mem: [f32; MEM_V5],
    last_act: usize,
}
impl BrainV5 {
    fn load(path: &str) -> Option<Self> {
        let s = std::fs::read_to_string(path).ok()?;
        let a = s.find(r#""w":"#).map(|i| i + 4)?;
        let a = s[a..].find('[').map(|i| a + i)?;
        let b = s[a..].find(']').map(|i| a + i)?;
        let body = &s[a + 1..b];
        let w: Vec<f32> = body.split(',').filter(|p| !p.trim().is_empty())
            .map(|p| p.trim().parse::<f32>().unwrap_or(0.0)).collect();
        if w.len() != NW_V5 { eprintln!("V5 load: len={} expected={}", w.len(), NW_V5); return None; }
        let mut k = 0usize;
        let mut take = |n: usize| -> Vec<f32> { let v = w[k..k+n].to_vec(); k += n; v };
        let w1 = take(IN_DIM_V5 * H1_V5);
        let b1 = take(H1_V5);
        let g1 = take(H1_V5);
        let w2 = take(H1_V5 * H2_V5);
        let b2 = take(H2_V5);
        let g2 = take(H2_V5);
        let w3 = take(H2_V5 * OUT_DIM_V5);
        let b3 = take(OUT_DIM_V5);
        Some(Self { w1, b1, g1, w2, b2, g2, w3, b3, mem: [0.0; MEM_V5], last_act: 0 })
    }
    fn rms_silu(x: &[f32], g: &[f32]) -> Vec<f32> {
        let n = x.len();
        let ms: f32 = x.iter().map(|v| v * v).sum::<f32>() / n as f32;
        let rms = (ms + 1e-6).sqrt();
        (0..n).map(|i| (x[i] / rms) * (g[i] / (1.0 + (-g[i]).exp()))).collect()
    }
    fn act(&mut self, obs: &[f32]) -> usize {
        let mut in_vec = vec![0.0f32; IN_DIM_V5];
        for i in 0..OBS_V5 { if i < obs.len() { in_vec[i] = obs[i]; } }
        for i in 0..MEM_V5 { in_vec[OBS_V5 + i] = self.mem[i]; }
        in_vec[OBS_V5 + MEM_V5 + self.last_act] = 1.0;
        let mut h1 = vec![0.0f32; H1_V5];
        for j in 0..H1_V5 {
            let mut s = self.b1[j];
            for i in 0..IN_DIM_V5 { s += in_vec[i] * self.w1[i * H1_V5 + j]; }
            h1[j] = s;
        }
        let h1 = Self::rms_silu(&h1, &self.g1);
        let mut h2lin = vec![0.0f32; H2_V5];
        for j in 0..H2_V5 {
            let mut s = self.b2[j];
            for i in 0..H1_V5 { s += h1[i] * self.w2[i * H2_V5 + j]; }
            h2lin[j] = s;
        }
        let h2add = Self::rms_silu(&h2lin, &self.g2);
        let h2: Vec<f32> = (0..H2_V5).map(|i| h1[i] + h2add[i]).collect();
        let mut out = vec![0.0f32; OUT_DIM_V5];
        for j in 0..OUT_DIM_V5 {
            let mut s = self.b3[j];
            for i in 0..H2_V5 { s += h2[i] * self.w3[i * OUT_DIM_V5 + j]; }
            out[j] = s;
        }
        let a = (0..7).max_by(|&i, &j| out[i].partial_cmp(&out[j]).unwrap()).unwrap();
        for i in 0..MEM_V5 {
            let t = out[7 + i];
            self.mem[i] = 0.65 * self.mem[i] + 0.35 * t.tanh();
        }
        self.last_act = a;
        a
    }
}
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
    let t = ram[0x0500 + page * 208 + row * 16 + col];
    if t == 0 {
        0
    } else if t == 0xC2 || t == 0xC3 {
        2
    } else {
        1
    }
}

#[inline]
fn is_solid_world(ram: &[u8; 2048], wx: i32, wy: i32) -> bool {
    world_tile_kind(ram, wx, wy) == 1
}

#[inline]
fn is_pit_column(ram: &[u8; 2048], wx: i32) -> bool {
    !is_solid_world(ram, wx, 208) && !is_solid_world(ram, wx, 224)
}

fn extract(con: &Console, prev: &mut Prev, steps: usize) -> [f32; OBS] {
    let ram = &con.bus.ram;
    let x = (ram[0x6D] as i32) * 256 + (ram[0x86] as i32);
    let ypix = ram[0x03B8] as i32;
    let lx = ((ram[0x86] as i32) - (ram[0x071C] as i32)).rem_euclid(256);

    let dx = x - prev.x;
    let dy = ypix - prev.y;
    prev.x = x;
    prev.y = ypix;

    let ram_vx = (ram[0x0057] as i8) as f32 / 16.0;
    let ram_vy = (ram[0x009F] as i8) as f32;
    prev.vx = 0.6 * prev.vx + 0.4 * ram_vx;
    prev.vy = 0.6 * prev.vy + 0.4 * ram_vy;
    if dx != 0 {
        prev.facing = if dx > 0 { 1.0 } else { -1.0 };
    }

    let on_ground = if ram[0x001D] == 0x00 { 1.0 } else { 0.0 };
    prev.og_avg = 0.9 * prev.og_avg + 0.1 * on_ground;

    let power = ram[0x0756];
    let feet = (ypix + if power == 0 { 32 } else { 48 }).clamp(32, 239);
    let front_x = x + 16;

    let mut o = [0f32; OBS];
    o[0] = x as f32 / WORLD_W as f32;
    o[1] = ypix as f32 / 240.0;
    o[2] = (prev.vx / 3.0).clamp(-1.0, 1.0);
    o[3] = (prev.vy / 6.0).clamp(-1.0, 1.0);
    o[4] = on_ground;
    o[5] = prev.facing;
    o[6] = (WORLD_W - x) as f32 / WORLD_W as f32;
    o[7] = steps as f32 / MAX_FRAMES as f32;

    let mut enemies: Vec<(i32, i32, f32, u8, u8)> = Vec::with_capacity(5);
    for slot in 0..5usize {
        if ram[0x000F + slot] == 0 {
            continue;
        }
        let etype = ram[0x0016 + slot];
        if etype == 0x2D || etype == 0x31 {
            continue;
        }
        let estate = ram[0x001E + slot];
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

    for (placed, &(ewx, ewy, _, _, _)) in enemies.iter().take(3).enumerate() {
        o[8 + placed * 2] = (((ewx - x) as f32 / 16.0) / 14.0).clamp(-1.0, 1.0);
        o[9 + placed * 2] = (((ewy - ypix) as f32 / 16.0) / 12.0).clamp(-1.0, 1.0);
    }
    prev.enemy_dist = enemies
        .iter()
        .find(|e| e.0 >= x - 4)
        .map(|e| (e.0 - front_x).max(0) as f32)
        .unwrap_or(999.0);

    let score =
        bcd_byte(ram[0x07DE]) * 10000 + bcd_byte(ram[0x07DF]) * 100 + bcd_byte(ram[0x07E0]);
    let coins = bcd_byte(ram[0x07ED]) * 10 + bcd_byte(ram[0x07EE]);
    let time = bcd_byte(ram[0x07F8]) * 100 + bcd_byte(ram[0x07F9]) * 10 + bcd_byte(ram[0x07FA]);
    o[14] = (coins as f32 / 20.0).min(1.0);
    o[15] = (score as f32 / 5000.0).min(1.0);

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
                2 => 0.35,
                _ => 0.0,
            };
            idx += 1;
        }
    }

    let row_wall_low = (feet - 6).clamp(32, 236);
    let row_wall_mid = (feet - 18).clamp(32, 236);
    let mut wall_dist = 999.0f32;
    let mut wall_height = 0.0f32;

    for d in 0..=96i32 {
        let wx = front_x + d;
        if is_solid_world(ram, wx, row_wall_low) || is_solid_world(ram, wx, row_wall_mid) {
            wall_dist = d as f32;
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
    o[79] = ram[0x077F] as f32 / 20.0;
    o[84] = (ram[0x0705] as f32) / 256.0;
    o[58] = ram[0x000E] as f32 / 15.0;
    o[59] = ram[0x00B5] as f32 / 5.0;
    o[60] = lx as f32 / 256.0;
    o[61] = (x - lx) as f32 / WORLD_W as f32;
    o[62] = prev.prev_action as f32 / 6.0;
    o[63] = prev.og_avg;

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
        o[66 + n * 3] = 1.0 - ttc;
        o[80 + n] = (etype as f32 / 32.0).clamp(0.0, 1.0);
        o[85 + n.min(2)] = if estate != 0 { 1.0 } else { 0.0 };
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

    o[104] = if prev.prev_buttons & BUTTON_A != 0 { 1.0 } else { 0.0 };
    o[105] = if prev.prev_buttons & BUTTON_B != 0 { 1.0 } else { 0.0 };
    o[106] = if prev.prev_buttons & BUTTON_LEFT != 0 { 1.0 } else { 0.0 };
    o[107] = if prev.prev_buttons & BUTTON_RIGHT != 0 { 1.0 } else { 0.0 };

    o[108] = sol_count as f32 / 40.0;
    o[109] = (gap_width / 64.0).min(1.0);
    o[110] = (x % 16) as f32 / 16.0;
    o[111] = (ypix % 16) as f32 / 16.0;

    let mut head_clear = 4.0f32;
    for t in 1..=4i32 {
        if is_solid_world(ram, x + 8, ypix - t * 16) {
            head_clear = (t - 1) as f32;
            break;
        }
    }
    o[112] = head_clear / 4.0;
    o[113] = (wall_height / 64.0).min(1.0);

    // ---- Predicción balística: DÓNDE SALTAR y DÓNDE VA A CAER ----------- //
    // (idéntico a train.rs; v0 = 5.0 px/frame, g ≈ 0.15 px/frame² con A)
    const V0: f32 = 5.0;
    const G: f32 = 0.15;
    let vx_eff = prev.vx.max(0.4);

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

    let need_wall = rise_dist(wall_height.max(16.0));
    let wall_margin = if wall_dist < 96.0 {
        wall_dist - need_wall
    } else {
        99.0
    };
    o[114] = (wall_margin / 32.0).clamp(-1.0, 1.0);
    o[115] = if wall_dist < 96.0 && (-20.0..=6.0).contains(&wall_margin) {
        1.0
    } else {
        0.0
    };

    let need_gap = rise_dist(10.0);
    let gap_margin = if gap_dist < 96.0 {
        gap_dist - need_gap
    } else {
        99.0
    };
    o[116] = (gap_margin / 32.0).clamp(-1.0, 1.0);

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
fn is_dead(ram: &[u8; 2048]) -> bool {
    ram[0x000E] == 0x06 || ram[0x000E] == 0x0B || ram[0x00B5] > 1
}

#[inline]
fn is_stage_over(ram: &[u8; 2048]) -> bool {
    // Solo el mástil real (estado 3 = bajando). Los tipos 0x2D/0x31 se cargan
    // ~12 bloques antes y premiaban llegar sin tocar bandera.
    ram[0x001D] == 0x03
}

// --------------------------------------------------------------------------- //
// Mini fuente bitmap 3x5 para dibujar HUD nítido dentro de cada viewport
// --------------------------------------------------------------------------- //
fn glyph_3x5(ch: char) -> [u8; 5] {
    match ch.to_ascii_uppercase() {
        '0' => [0b111, 0b101, 0b101, 0b101, 0b111],
        '1' => [0b010, 0b110, 0b010, 0b010, 0b111],
        '2' => [0b111, 0b001, 0b111, 0b100, 0b111],
        '3' => [0b111, 0b001, 0b111, 0b001, 0b111],
        '4' => [0b101, 0b101, 0b111, 0b001, 0b001],
        '5' => [0b111, 0b100, 0b111, 0b001, 0b111],
        '6' => [0b111, 0b100, 0b111, 0b101, 0b111],
        '7' => [0b111, 0b001, 0b001, 0b010, 0b010],
        '8' => [0b111, 0b101, 0b111, 0b101, 0b111],
        '9' => [0b111, 0b101, 0b111, 0b001, 0b111],
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'B' => [0b110, 0b101, 0b110, 0b101, 0b110],
        'C' => [0b111, 0b100, 0b100, 0b100, 0b111],
        'D' => [0b110, 0b101, 0b101, 0b101, 0b110],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'F' => [0b111, 0b100, 0b110, 0b100, 0b100],
        'G' => [0b111, 0b100, 0b101, 0b101, 0b111],
        'H' => [0b101, 0b101, 0b111, 0b101, 0b101],
        'I' => [0b111, 0b010, 0b010, 0b010, 0b111],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'M' => [0b101, 0b111, 0b101, 0b101, 0b101],
        'N' => [0b111, 0b101, 0b101, 0b101, 0b101],
        'O' => [0b111, 0b101, 0b101, 0b101, 0b111],
        'P' => [0b111, 0b101, 0b111, 0b100, 0b100],
        'R' => [0b110, 0b101, 0b110, 0b101, 0b101],
        'S' => [0b111, 0b100, 0b111, 0b001, 0b111],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        'U' => [0b101, 0b101, 0b101, 0b101, 0b111],
        'V' => [0b101, 0b101, 0b101, 0b101, 0b010],
        'W' => [0b101, 0b101, 0b101, 0b111, 0b101],
        'X' => [0b101, 0b101, 0b010, 0b101, 0b101],
        'Y' => [0b101, 0b101, 0b010, 0b010, 0b010],
        '.' => [0b000, 0b000, 0b000, 0b000, 0b010],
        ':' => [0b000, 0b010, 0b000, 0b010, 0b000],
        '#' => [0b101, 0b111, 0b101, 0b111, 0b101],
        '%' => [0b101, 0b001, 0b010, 0b100, 0b101],
        '-' => [0b000, 0b000, 0b111, 0b000, 0b000],
        _ => [0b000, 0b000, 0b000, 0b000, 0b000],
    }
}

fn draw_text(
    comp: &mut [u32],
    stride: usize,
    x0: usize,
    y0: usize,
    text: &str,
    color: u32,
    font_scale: usize,
) {
    let mut cx = x0;
    for ch in text.chars() {
        let g = glyph_3x5(ch);
        for (ry, &bits) in g.iter().enumerate() {
            for rx in 0..3usize {
                if (bits >> (2 - rx)) & 1 != 0 {
                    for sy in 0..font_scale {
                        for sx in 0..font_scale {
                            let px = cx + rx * font_scale + sx;
                            let py = y0 + ry * font_scale + sy;
                            let idx = py * stride + px;
                            if idx < comp.len() {
                                comp[idx] = color;
                            }
                        }
                    }
                }
            }
        }
        cx += 4 * font_scale;
    }
}

// --------------------------------------------------------------------------- //
// Estructura por Agente
// --------------------------------------------------------------------------- //
struct Agent {
    con: Console,
    brain: BrainKind,
    prev: Prev,
    current_action: usize,
    ep_frames: usize,
    max_x_ever: i32,
    wins: usize,
    deaths: usize,
    reloads: usize,
    flash_reload: usize,
    finished_round: bool,
    loaded_path: String,
    mtime: Option<SystemTime>,
    name: String,
    jump_lock: i32,
    airborne: bool,
}

impl Agent {
    fn reset_episode(&mut self, snap: &Snapshot) {
        snap.restore(&mut self.con);
        self.prev = Prev::fresh();
        self.current_action = 0;
        self.ep_frames = 0;
        self.finished_round = false;
        self.jump_lock = 0;
        self.airborne = false;
        if let BrainKind::V5(b) = &mut self.brain { b.mem = [0.0; MEM_V5]; b.last_act = 0; }
    }
}

fn compute_grid(n: usize) -> (usize, usize, usize) {
    let cols = match n {
        1 => 1,
        2 => 2,
        3 | 4 => 2,
        5 | 6 => 3,
        7..=9 => 3,
        _ => 4,
    };
    let rows = (n + cols - 1) / cols;
    // Escala adaptativa para no exceder ~1600x960 en pantalla
    let scale = if rows <= 1 && cols <= 2 {
        3
    } else if rows <= 2 && cols <= 3 {
        2
    } else {
        1
    };
    (cols, rows, scale)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut rom = None;
    let mut skip = DEFAULT_SKIP;
    let mut scale_override: Option<usize> = None;
    let mut paths: Vec<String> = Vec::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--rom" if i + 1 < args.len() => {
                rom = Some(args[i + 1].clone());
                i += 2;
            }
            "--skip" if i + 1 < args.len() => {
                skip = args[i + 1].parse().unwrap_or(DEFAULT_SKIP);
                i += 2;
            }
            "--scale" if i + 1 < args.len() => {
                scale_override = args[i + 1].parse().ok();
                i += 2;
            }
            "--weights" => {
                i += 1;
                while i < args.len() && !args[i].starts_with("--") {
                    paths.push(args[i].clone());
                    i += 1;
                }
            }
            other if !other.starts_with("--") => {
                paths.push(other.to_string());
                i += 1;
            }
            _ => i += 1,
        }
    }

    if paths.is_empty() {
        paths = vec![
            r"C:\Users\USUARIO\mario-ai\rust\best_real.json".to_string(),
            "best_ai1.json".to_string(),
            "best_ai2.json".to_string(),
            "best_ai3.json".to_string(),
        ];
    }

    let rom_path = rom.unwrap_or_else(|| ROM_DEFAULT.to_string());
    let raw = std::fs::read(&rom_path).expect("no se pudo leer la ROM");
    let snap = create_warmup_snapshot(&raw);

    let mut agents: Vec<Agent> = Vec::new();
    for (idx, p) in paths.iter().enumerate() {
        let mtime = std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let brain = if let Some(b) = Brain::load(p) { BrainKind::Old(b) }
        else if let Some(b) = BrainV5::load(p) { BrainKind::V5(b) }
        else {
            eprintln!("Aviso: '{p}' aun no existe o es invalido; esperando hot-reload...");
            BrainKind::Old(Brain::fallback(idx as u64 + 1))
        };
        let cart = Cartridge::from_bytes(&raw).expect("cartucho inválido");
        let mut con = Console::new(cart);
        snap.restore(&mut con);

        let name = std::path::Path::new(p)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("AI_{}", idx + 1));

        agents.push(Agent {
            con,
            brain,
            prev: Prev::fresh(),
            current_action: 0,
            ep_frames: 0,
            max_x_ever: 40,
            wins: 0,
            deaths: 0,
            reloads: 0,
            flash_reload: 0,
            finished_round: false,
            loaded_path: p.clone(),
            mtime,
            name,
            jump_lock: 0,
            airborne: false,
        });
    }

    let n = agents.len();
    let (cols, rows, auto_scale) = compute_grid(n);
    let scale = scale_override.unwrap_or(auto_scale).max(1);
    let vw = W * scale;
    let vh = H * scale;
    let total_w = vw * cols;
    let total_h = vh * rows;

    let mut win = minifb::Window::new(
        "SMB Multinstancia — [R] Carrera Sync | [Tab] Turbo | [M] Modo Carrera | [Space] Pausa",
        total_w,
        total_h,
        minifb::WindowOptions {
            scale: minifb::Scale::X1,
            resize: true,
            ..Default::default()
        },
    )
    .expect("ventana");
    win.set_target_fps(60);

    let mut comp = vec![0xFF101014u32; total_w * total_h];
    let mut global_frame = 0usize;
    let mut paused = false;
    let mut race_mode = false;
    let mut speed_mult = 1usize;
    let skip = skip.max(1);

    while win.is_open() && !win.is_key_down(minifb::Key::Escape) {
        if win.is_key_pressed(minifb::Key::Space, minifb::KeyRepeat::No) {
            paused = !paused;
        }
        if win.is_key_pressed(minifb::Key::R, minifb::KeyRepeat::No) {
            for a in agents.iter_mut() {
                a.reset_episode(&snap);
            }
        }
        if win.is_key_pressed(minifb::Key::M, minifb::KeyRepeat::No) {
            race_mode = !race_mode;
            for a in agents.iter_mut() {
                a.reset_episode(&snap);
            }
        }
        if win.is_key_pressed(minifb::Key::Tab, minifb::KeyRepeat::No) {
            speed_mult = match speed_mult {
                1 => 2,
                2 => 4,
                _ => 1,
            };
        }

        if !paused {
            for _ in 0..speed_mult {
                global_frame += 1;

                // 1) Hot-reload de cerebros en disco cada 60 frames (~1 s)
                if global_frame % 60 == 0 {
                    for a in agents.iter_mut() {
                        let mt = std::fs::metadata(&a.loaded_path)
                            .and_then(|m| m.modified())
                            .ok();
                        if mt.is_some() && mt != a.mtime {
                            let reloaded = if let Some(b) = Brain::load(&a.loaded_path) {
                                Some(BrainKind::Old(b))
                            } else {
                                BrainV5::load(&a.loaded_path).map(BrainKind::V5)
                            };
                            if let Some(brain) = reloaded {
                                a.brain = brain;
                                a.mtime = mt;
                                a.reloads += 1;
                                a.flash_reload = 45;
                            }
                        }
                    }
                }

                // 2) Avanzar cada agente con la misma física y frame-skip que train.rs
                for a in agents.iter_mut() {
                    // Para la visualización, usamos la misma política de DECISIÓN
                    // que el entrenamiento: decide cada `skip` frames y solo
                    // re-decide al aterrizar (antes: cada frame en el suelo,
                    // que deformaba dx/dy x4 y rompía la percepción).
                    let was_air = a.prev.y != 0 && a.con.bus.ram[0x001D] != 0;
                    let now_air = a.con.bus.ram[0x001D] != 0;
                    let just_landed = was_air && !now_air;
                    if a.ep_frames % skip == 0 || just_landed {
                        let obs = extract(&a.con, &mut a.prev, a.ep_frames);
                        a.current_action = match &mut a.brain {
         BrainKind::Old(b) => b.act(&obs),
         BrainKind::V5(b) => b.act(&obs),
     };
                        a.prev.prev_action = a.current_action;
                    }

                    let grounded = now_air == false;

                    // ---- SALTO COMPROMETIDO (igual que en train.rs) -------- //
                    if now_air {
                        a.airborne = true;
                    } else if a.airborne {
                        a.airborne = false;
                        a.jump_lock = 0;
                    }
                    let want_a = buttons_for(a.current_action) & BUTTON_A != 0;
                    // A se mantiene SIEMPRE durante el compromiso (misma regla
                    // que train.rs: soltarlo un frame = salto corto de ~28 px).
                    let mut a_on = a.jump_lock > 0;
                    if !a_on && want_a {
                        if grounded && (a.prev.prev_buttons & BUTTON_A != 0) {
                            a_on = false;
                        } else {
                            a_on = true;
                            if grounded {
                                a.jump_lock = JUMP_HOLD;
                            }
                        }
                    }
                    let mut hw = buttons_for(a.current_action) & !BUTTON_A;
                    if a_on {
                        hw |= BUTTON_A;
                    }
                    if a.jump_lock > 0 && a.airborne {
                        a.jump_lock -= 1;
                    }

                    a.prev.prev_buttons = hw;
                    if hw & BUTTON_A != 0 {
                        a.prev.jump_hold += 1;
                    } else {
                        a.prev.jump_hold = 0;
                    }

                    a.con.set_buttons(0, hw);
                    a.con.step_frame();
                    a.con.bus.apu.samples.clear();
                    a.ep_frames += 1;
                    if a.flash_reload > 0 {
                        a.flash_reload -= 1;
                    }

                    let x_now =
                        (a.con.bus.ram[0x6D] as i32) * 256 + (a.con.bus.ram[0x86] as i32);
                    if x_now > a.prev.best_x {
                        a.prev.best_x = x_now;
                        a.prev.stuck = 0;
                    } else {
                        a.prev.stuck += 1;
                    }
                    a.max_x_ever = a.max_x_ever.max(x_now);

                    let won = is_stage_over(&a.con.bus.ram);
                    let dead = is_dead(&a.con.bus.ram)
                        || (a.prev.x > 60 && x_now < a.prev.x - 100)
                        || a.prev.stuck >= STUCK_TIMEOUT
                        || a.ep_frames >= MAX_FRAMES;

                    if won {
                        a.wins += 1;
                    }
                    if won || dead {
                        if dead && !won {
                            a.deaths += 1;
                        }
                        if race_mode {
                            a.finished_round = true;
                        } else {
                            a.reset_episode(&snap);
                        }
                    }
                }

                // En modo carrera sincronizada, cuando todos terminan la ronda se reinician juntos
                if race_mode && agents.iter().all(|a| a.finished_round) {
                    for a in agents.iter_mut() {
                        a.reset_episode(&snap);
                    }
                }
            }
        }

        // 3) Determinar líder actual (mayor X en la carrera actual) y récord global
        let leader = agents
            .iter()
            .enumerate()
            .max_by_key(|(_, a)| {
                (a.con.bus.ram[0x6D] as i32) * 256 + (a.con.bus.ram[0x86] as i32)
            })
            .map(|(i, _)| i)
            .unwrap_or(0);

        let record_holder = agents
            .iter()
            .enumerate()
            .max_by_key(|(_, a)| a.max_x_ever)
            .map(|(i, _)| i)
            .unwrap_or(0);

        // 4) Componer cuadrícula 2D + barras de progreso + HUD pixel-art
        for (idx, a) in agents.iter().enumerate() {
            let col = idx % cols;
            let row = idx / cols;
            let x_off = col * vw;
            let y_off = row * vh;

            let fb = a.con.framebuffer();
            for yy in 0..H {
                let src_row = &fb[yy * W..(yy + 1) * W];
                for oy in 0..scale {
                    let gy = y_off + yy * scale + oy;
                    let dst_row_start = gy * total_w + x_off;
                    for xx in 0..W {
                        let c = src_row[xx];
                        let base = dst_row_start + xx * scale;
                        for ox in 0..scale {
                            comp[base + ox] = c;
                        }
                    }
                }
            }

            // Franja superior oscura translúcida para el HUD y la barra de progreso
            let hud_h = (12 * scale).min(vh);
            for yy in 0..hud_h {
                let row_start = (y_off + yy) * total_w + x_off;
                for xx in 0..vw {
                    let p = comp[row_start + xx];
                    let r = ((p >> 16) & 0xFF) / 3;
                    let g = ((p >> 8) & 0xFF) / 3;
                    let b = (p & 0xFF) / 3;
                    comp[row_start + xx] = (r << 16) | (g << 8) | b;
                }
            }

            // Barra de progreso del nivel (verde = x actual, cian = récord personal)
            let cur_x = (a.con.bus.ram[0x6D] as i32) * 256 + (a.con.bus.ram[0x86] as i32);
            let cur_w = ((cur_x as f32 / WORLD_W as f32).clamp(0.0, 1.0) * vw as f32) as usize;
            let best_w =
                ((a.max_x_ever as f32 / WORLD_W as f32).clamp(0.0, 1.0) * vw as f32) as usize;
            let bar_y0 = y_off + hud_h.saturating_sub(2 * scale);
            for yy in bar_y0..y_off + hud_h {
                let rs = yy * total_w + x_off;
                for xx in 0..cur_w.min(vw) {
                    comp[rs + xx] = if idx == leader { 0xFFFFD700 } else { 0xFF38E048 };
                }
                if best_w > 0 && best_w < vw {
                    for ox in 0..scale.min(vw - best_w) {
                        comp[rs + best_w + ox] = 0xFF00FFFF;
                    }
                }
            }

            // Texto HUD dentro de la celda
            let pct = (cur_x as f32 / WORLD_W as f32 * 100.0).min(100.0);
            let hud_line = format!(
                "#{}:{} {:>4.0}T {:>2.0}% W:{} F:{:.0}",
                idx + 1,
                &a.name[..a.name.len().min(8)],
                cur_x as f32 / 16.0,
                pct,
                a.wins,
                match &a.brain {
                    BrainKind::Old(b) => b.fitness,
                    BrainKind::V5(_) => 0.0,
                }
            );
            let text_color = if idx == leader {
                0xFFFFE040
            } else if a.flash_reload > 0 {
                0xFF40FF80
            } else {
                0xFFFFFFFF
            };
            draw_text(
                &mut comp,
                total_w,
                x_off + 4 * scale,
                y_off + 2 * scale,
                &hud_line,
                text_color,
                scale.max(1),
            );

            // Borde coloreado (Dorado = Líder actual, Verde = Recién actualizado, Cian = Récord)
            let (border_color, thickness) = if a.flash_reload > 0 {
                (0xFF00FF66, 2 * scale)
            } else if idx == leader {
                (0xFFFFD700, 2 * scale)
            } else if idx == record_holder {
                (0xFF00D8FF, scale)
            } else {
                (0xFF303038, 1)
            };

            for t in 0..thickness {
                let top = (y_off + t) * total_w + x_off;
                let bot = (y_off + vh - 1 - t) * total_w + x_off;
                for xx in 0..vw {
                    comp[top + xx] = border_color;
                    comp[bot + xx] = border_color;
                }
                for yy in 0..vh {
                    let rs = (y_off + yy) * total_w + x_off;
                    comp[rs + t] = border_color;
                    comp[rs + vw - 1 - t] = border_color;
                }
            }
        }

        win.update_with_buffer(&comp, total_w, total_h).unwrap();

        if global_frame % 15 == 0 {
            let lead_x = (agents[leader].con.bus.ram[0x6D] as i32) * 256
                + (agents[leader].con.bus.ram[0x86] as i32);
            win.set_title(&format!(
                "SMB [{}x{}] | LÍDER #{}: {} ({:.1}t) | Vel: {}x | Modo: {} | [R] Reset [Tab] Turbo [M] Carrera",
                cols,
                rows,
                leader + 1,
                agents[leader].name,
                lead_x as f32 / 16.0,
                speed_mult,
                if race_mode { "CARRERA" } else { "CONTINUO" }
            ));
        }
    }
}
