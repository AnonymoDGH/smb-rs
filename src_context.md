# src/ — contexto para IA

Mensaje para la IA: below is the full source tree. Analyze for bugs
in emulation, physics and RL. Consider the fitness/objective first.

Contenido: subset: tiny (train.rs + watchall.rs + lib + console + cpu) (6 archivos)

---

## train.rs

```rust

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
const STUCK_TIMEOUT: usize = 150;

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
    for f in 0..1200usize {
        let pulse = (5..=6).contains(&f)
            || (80..=81).contains(&f)
            || (200..=201).contains(&f)
            || (400..=401).contains(&f)
            || (700..=701).contains(&f);
        con.set_buttons(0, if pulse { BUTTON_START } else { 0 });
        con.step_frame();
        con.bus.apu.samples.clear();
        let x = (con.bus.ram[0x6D] as i32) * 256 + (con.bus.ram[0x86] as i32);
        if x > 30 && con.bus.ram[0x001D] == 0 {
            break;
        }
    }
    Snapshot::capture(&con)
}

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

const NW: usize = OBS * HID + HID + HID * ACT + ACT;

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

    fn smart_prior(rng: &mut Rng) -> Self {
        let mut b = Self::random(rng);
        let b2_start = OBS * HID + HID + HID * ACT;
        b.w[b2_start + 3] += 0.6;
        b.w[b2_start + 4] += 0.25;
        b.w[b2_start + 6] -= 0.5;
        b
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
    let addr = 0x0500 + page * 208 + row * 16 + col;
    let t = ram[addr];
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
    prev.vx = 0.5 * prev.vx + 0.5 * (dx as f32).max(ram_vx);
    prev.vy = 0.5 * prev.vy + 0.5 * (dy as f32 + ram_vy * 0.5);
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

    for (placed, &(ewx, ewy, _evx, _estate, _etype)) in enemies.iter().take(3).enumerate() {
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

    let air_frames_rem = if on_ground > 0.5 {
        0.0
    } else {
        ((208 - feet).max(0) as f32 / 3.5).clamp(0.0, 35.0)
    };
    let landing_x = x + 8 + (prev.vx * air_frames_rem) as i32;
    let landing_in_pit = is_pit_column(ram, landing_x);
    let landing_on_enemy = enemies.iter().any(|e| (e.0 - landing_x).abs() <= 14);

    o[114] = if landing_in_pit { 1.0 } else { 0.0 };
    o[115] = if landing_on_enemy { 1.0 } else { 0.0 };
    o[116] = (prev.jump_hold as f32 / 24.0).min(1.0);
    o[117] = (prev.facing * prev.vx.abs() / 3.0).clamp(-1.0, 1.0);
    o[118] = if wall_dist < 40.0 && (feet - wall_height as i32) <= ypix + 24 {
        1.0
    } else {
        0.0
    };
    o[119] = (prev.stuck as f32 / STUCK_TIMEOUT as f32).min(1.0);
    o
}

#[inline]
fn is_stage_over(ram: &[u8; 2048]) -> bool {
    ram[0x001D] == 0x03 || (0x0016..=0x001A).any(|a| ram[a] == 0x2D || ram[a] == 0x31)
}

#[inline]
fn is_dead(ram: &[u8; 2048]) -> bool {
    ram[0x000E] == 0x06 || ram[0x000E] == 0x0B || ram[0x00B5] > 1
}

fn compute_fitness(x_max: i32, score: i32, frames: usize, won: bool) -> f32 {
    let tiles = x_max as f32 / 16.0;
    let avg_speed = x_max as f32 / (frames.max(30) as f32);

    const MILESTONES: [i32; 8] = [480, 640, 768, 944, 1152, 1424, 2200, 2950];
    let milestone_bonus: f32 = MILESTONES
        .iter()
        .filter(|&&m| x_max >= m)
        .count() as f32
        * 45.0;

    if won {
        10_000.0 + (MAX_FRAMES.saturating_sub(frames)) as f32 * 3.0 + score as f32 * 0.05
    } else {
        tiles * 18.0 + milestone_bonus + avg_speed * 60.0 + score as f32 * 0.02
    }
}

fn run_episode(
    con: &mut Console,
    snap: &Snapshot,
    brain: &Brain,
    frame_skip: usize,
    frame_limit: usize,
) -> (f32, f32, i32, usize, bool) {
    snap.restore(con);

    let mut prev = Prev::fresh();
    let mut x_max: i32 = 40;
    let mut won = false;
    let mut frames = 0usize;
    let skip = frame_skip.max(1);

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
            let mut hw_buttons = desired_buttons;

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
            frames += 1;

            let x_now = (con.bus.ram[0x6D] as i32) * 256 + (con.bus.ram[0x86] as i32);
            if x_now > x_max {
                x_max = x_now;
                prev.stuck = 0;
            } else {
                prev.stuck += 1;
            }

            if is_stage_over(&con.bus.ram) {
                won = true;
                break;
            }
            if is_dead(&con.bus.ram) || (prev.x > 60 && x_now < prev.x - 100) {
                frames = frame_limit;
                break;
            }

            if sub > 0 && was_airborne && con.bus.ram[0x001D] == 0 {
                break;
            }
        }

        if won || prev.stuck >= STUCK_TIMEOUT {
            break;
        }
    }

    let ram = &con.bus.ram;
    let score =
        bcd_byte(ram[0x07DE]) * 10000 + bcd_byte(ram[0x07DF]) * 100 + bcd_byte(ram[0x07E0]);
    let tiles = x_max as f32 / 16.0;
    let fitness = compute_fitness(x_max, score, frames, won);
    (fitness, tiles, score, frames, won)
}

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
            let rom = get("--rom", ROM_DEFAULT);
            let seed_file = if seed_file.is_empty() {
                None
            } else {
                Some(seed_file.as_str())
            };
            train_parallel(&rom, gens, pop, seed, skip, seed_file, &out);
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
        other => {
            eprintln!("comando desconocido: {other} (usa train, eval o watch)");
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

fn mutate_weights(w: &mut [f32], rng: &mut Rng, rate: f32, sigma: f32) {
    for v in w.iter_mut() {
        if rng.f32() < rate {
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

    let nthreads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .min(2);
    let chunk = (pop + nthreads - 1) / nthreads;
    println!(
        "rom={rom_path}\ngens={gens} pop={pop} skip={skip} hilos={nthreads} semilla={seed} -> {out}"
    );

    let mut best_fit = f32::NEG_INFINITY;
    let mut best_brain = brains[0].clone();
    let mut best_tiles_ever = 0.0f32;
    let mut stagnation = 0usize;
    let t0 = std::time::Instant::now();
    let mut total_frames_sim = 0usize;

    for gen in 1..=gens {
        let frame_limit = ((450.0 + best_tiles_ever * 16.0) as usize).clamp(600, MAX_FRAMES);

        let mut results: Vec<(f32, f32, i32, usize, bool)> = Vec::with_capacity(pop);
        let brains_ref = &brains;
        std::thread::scope(|s| {
            let mut handles = Vec::new();
            for c in brains_ref.chunks(chunk) {
                let rom_ref = rom.clone();
                let snap_ref = snap.clone();
                handles.push(s.spawn(move || {
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

        let improved = bf > best_fit;
        if improved {
            best_fit = bf;
            best_brain = brains[order[0]].clone();
            best_brain.save(out, best_fit);
            stagnation = 0;
        } else {
            stagnation += 1;
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
                 {:.0} fps  {:.1}s{}",
                if bwon { "BANDERA" } else { "en_ruta" },
                wins,
                pop,
                total_frames_sim as f64 / elapsed.max(0.001),
                elapsed,
                if improved { " *" } else { "" }
            );
        }

        let elite = (pop / 12).max(2);
        let mut next: Vec<Brain> = Vec::with_capacity(pop);

        next.push(best_brain.clone());
        for &i in order.iter().take(elite) {
            if next.len() < pop {
                next.push(brains[i].clone());
            }
        }

        for &(rate, sigma) in &[(0.02, 0.04), (0.04, 0.08), (0.08, 0.15)] {
            if next.len() < pop {
                let mut child = best_brain.clone();
                mutate_weights(&mut child.w, &mut rng, rate, sigma);
                next.push(child);
            }
        }

        let stag_boost: f32 = if stagnation > 12 {
            1.8
        } else if stagnation > 6 {
            1.35
        } else {
            1.0
        };

        while next.len() < pop {
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
```

## watchall.rs

```rust

use smb_rs::{
    Cartridge, Console, BUTTON_A, BUTTON_B, BUTTON_LEFT, BUTTON_RIGHT, BUTTON_START,
};
use std::time::SystemTime;

const W: usize = 256;
const H: usize = 240;
const OBS: usize = 120;
const HID: usize = 48;
const ACT: usize = 7;
const NW: usize = OBS * HID + HID + HID * ACT + ACT;
const WORLD_W: i32 = 212 * 16;
const MAX_FRAMES: usize = 3000;
const DEFAULT_SKIP: usize = 4;
const STUCK_TIMEOUT: usize = 180;

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
    for f in 0..1200usize {
        let pulse = (5..=6).contains(&f)
            || (80..=81).contains(&f)
            || (200..=201).contains(&f)
            || (400..=401).contains(&f)
            || (700..=701).contains(&f);
        con.set_buttons(0, if pulse { BUTTON_START } else { 0 });
        con.step_frame();
        con.bus.apu.samples.clear();
        let x = (con.bus.ram[0x6D] as i32) * 256 + (con.bus.ram[0x86] as i32);
        if x > 30 && con.bus.ram[0x001D] == 0 {
            break;
        }
    }
    Snapshot::capture(&con)
}

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
        w[b2_start + 3] += 0.6;
        w[b2_start + 4] += 0.2;
        Brain { w, fitness: 0.0 }
    }

    fn load(path: &str) -> Option<Brain> {
        let s = std::fs::read_to_string(path).ok()?;
        let fitness = s
            .find("\"fitness\":")
            .and_then(|i| {
                let rest = &s[i + 10..];
                let end = rest.find(|c: char| c == ',' || c == '}')?;
                rest[..end].trim().parse::<f32>().ok()
            })
            .unwrap_or(0.0);

        let a = s.find("\"w\":").map(|i| i + 4)?;
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
    prev.vx = 0.5 * prev.vx + 0.5 * (dx as f32).max(ram_vx);
    prev.vy = 0.5 * prev.vy + 0.5 * (dy as f32 + ram_vy * 0.5);
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

    let air_frames_rem = if on_ground > 0.5 {
        0.0
    } else {
        ((208 - feet).max(0) as f32 / 3.5).clamp(0.0, 35.0)
    };
    let landing_x = x + 8 + (prev.vx * air_frames_rem) as i32;
    let landing_in_pit = is_pit_column(ram, landing_x);
    let landing_on_enemy = enemies.iter().any(|e| (e.0 - landing_x).abs() <= 14);

    o[114] = if landing_in_pit { 1.0 } else { 0.0 };
    o[115] = if landing_on_enemy { 1.0 } else { 0.0 };
    o[116] = (prev.jump_hold as f32 / 24.0).min(1.0);
    o[117] = (prev.facing * prev.vx.abs() / 3.0).clamp(-1.0, 1.0);
    o[118] = if wall_dist < 40.0 && (feet - wall_height as i32) <= ypix + 24 {
        1.0
    } else {
        0.0
    };
    o[119] = (prev.stuck as f32 / STUCK_TIMEOUT as f32).min(1.0);
    o
}

#[inline]
fn is_dead(ram: &[u8; 2048]) -> bool {
    ram[0x000E] == 0x06 || ram[0x000E] == 0x0B || ram[0x00B5] > 1
}

#[inline]
fn is_stage_over(ram: &[u8; 2048]) -> bool {
    ram[0x001D] == 0x03 || (0x0016..=0x001A).any(|a| ram[a] == 0x2D || ram[a] == 0x31)
}

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

struct Agent {
    con: Console,
    brain: Brain,
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
}

impl Agent {
    fn reset_episode(&mut self, snap: &Snapshot) {
        snap.restore(&mut self.con);
        self.prev = Prev::fresh();
        self.current_action = 0;
        self.ep_frames = 0;
        self.finished_round = false;
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
        let brain = Brain::load(p).unwrap_or_else(|| {
            eprintln!("Aviso: '{p}' aún no existe o es inválido; esperando hot-reload...");
            Brain::fallback(idx as u64 + 1)
        });
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

                if global_frame % 60 == 0 {
                    for a in agents.iter_mut() {
                        let mt = std::fs::metadata(&a.loaded_path)
                            .and_then(|m| m.modified())
                            .ok();
                        if mt.is_some() && mt != a.mtime {
                            if let Some(b) = Brain::load(&a.loaded_path) {
                                a.brain = b;
                                a.mtime = mt;
                                a.reloads += 1;
                                a.flash_reload = 45;
                            }
                        }
                    }
                }

                for a in agents.iter_mut() {
                    if race_mode && a.finished_round {
                        continue;
                    }

                    let grounded = a.con.bus.ram[0x001D] == 0;
                    if a.ep_frames % skip == 0 || grounded {
                        let obs = extract(&a.con, &mut a.prev, a.ep_frames);
                        a.current_action = a.brain.act(&obs);
                        a.prev.prev_action = a.current_action;
                    }

                    let mut hw = buttons_for(a.current_action);
                    if grounded
                        && (hw & BUTTON_A != 0)
                        && (a.prev.prev_buttons & BUTTON_A != 0)
                    {
                        hw &= !BUTTON_A;
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

                if race_mode && agents.iter().all(|a| a.finished_round) {
                    for a in agents.iter_mut() {
                        a.reset_episode(&snap);
                    }
                }
            }
        }

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

            let pct = (cur_x as f32 / WORLD_W as f32 * 100.0).min(100.0);
            let hud_line = format!(
                "#{}:{} {:>4.0}T {:>2.0}% W:{} F:{:.0}",
                idx + 1,
                &a.name[..a.name.len().min(8)],
                cur_x as f32 / 16.0,
                pct,
                a.wins,
                a.brain.fitness
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
```

## src/lib.rs

```rust

pub mod apu;
#[cfg(feature = "audio")]
pub mod audio;
pub mod cartridge;
pub mod console;
pub mod cpu;
pub mod pngout;
pub mod ppu;

pub use cartridge::Cartridge;
pub use console::{
    Console, BUTTON_A, BUTTON_B, BUTTON_DOWN, BUTTON_LEFT, BUTTON_RIGHT, BUTTON_SELECT,
    BUTTON_START, BUTTON_UP,
};
```

## src/console.rs

```rust

use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::cpu::{Bus as CpuBus, Cpu};
use crate::ppu::Ppu;

pub const BUTTON_A: u8 = 0x01;
pub const BUTTON_B: u8 = 0x02;
pub const BUTTON_SELECT: u8 = 0x04;
pub const BUTTON_START: u8 = 0x08;
pub const BUTTON_UP: u8 = 0x10;
pub const BUTTON_DOWN: u8 = 0x20;
pub const BUTTON_LEFT: u8 = 0x40;
pub const BUTTON_RIGHT: u8 = 0x80;

pub struct Bus {
    pub ram: [u8; 2048],
    pub ppu: Ppu,
    pub apu: Apu,
    pub cart: Cartridge,
    pub buttons: [u8; 2],
    pub strobe: bool,
    pub shift: [u8; 2],
    pub open_bus: u8,
}

impl Bus {
    pub fn read_debug(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram[(addr as usize) & 0x7FF],
            0x8000..=0xFFFF => self.cart.cpu_read(addr),
            _ => 0,
        }
    }

    pub fn new(cart: Cartridge) -> Bus {
        let mirroring = cart.mirroring;
        Bus {
            ram: [0; 2048],
            ppu: Ppu::new(mirroring),
            apu: Apu::new(),
            cart,
            buttons: [0; 2],
            strobe: false,
            shift: [0; 2],
            open_bus: 0,
        }
    }
}

impl CpuBus for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        let v = match addr {
            0x0000..=0x1FFF => self.ram[(addr as usize) & 0x7FF],
            0x2000..=0x3FFF => self.ppu.cpu_read(addr & 0x2007, &self.cart),
            0x4014 => self.open_bus,
            0x4015 => {
                let mut s = 0u8;
                if self.apu.irq() {
                    s |= 0x40;
                }
                self.apu.clear_frame_irq();
                s
            }
            0x4016 | 0x4017 => {
                let i = (addr as usize) & 1;
                if self.strobe {
                    self.buttons[i] & 1
                } else {
                    let b = self.shift[i] & 1;
                    self.shift[i] = (self.shift[i] >> 1) | 0x80;
                    b
                }
            }
            0x4000..=0x401F => self.open_bus,
            0x4020..=0x7FFF => self.open_bus,
            _ => self.cart.cpu_read(addr),
        };
        self.open_bus = v;
        v
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        match addr {
            0x0000..=0x1FFF => self.ram[(addr as usize) & 0x7FF] = val,
            0x2000..=0x3FFF => self.ppu.cpu_write(addr & 0x2007, val, &mut self.cart),
            0x4014 => {
                let base = (val as u16) << 8;
                for i in 0..256u16 {
                    let b = self.read(base + i);
                    self.ppu.oam[i as usize] = b;
                }
            }
            0x4016 => {
                self.strobe = val & 1 != 0;
                if self.strobe {
                    self.shift = self.buttons;
                }
            }
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write(addr, val),
            0x8000..=0xFFFF => self.cart.cpu_write(addr, val),
            _ => {}
        }
    }
}

pub struct Console {
    pub cpu: Cpu,
    pub bus: Bus,
    pub frames: u64,
}

impl Console {
    pub fn new(cart: Cartridge) -> Console {
        let mut bus = Bus::new(cart);
        let cpu = Cpu::new(&mut bus);
        Console { cpu, bus, frames: 0 }
    }

    pub fn reset(&mut self) {
        self.bus.ppu = Ppu::new(self.bus.cart.mirroring);
        self.bus.apu = Apu::new();
        self.cpu = Cpu::new(&mut self.bus);
    }

    pub fn set_buttons(&mut self, pad: usize, buttons: u8) {
        self.bus.buttons[pad & 1] = buttons;
    }

    pub fn step_frame(&mut self) -> bool {
        self.bus.ppu.frame_ready = false;
        let mut guard: u64 = 0;
        loop {
            let cyc = self.cpu.step(&mut self.bus);
            self.bus.ppu.step(cyc * 3, &self.bus.cart);
            self.bus.apu.step(cyc);
            if self.bus.ppu.nmi {
                self.bus.ppu.nmi = false;
                self.cpu.nmi_pending = true;
            }
            self.cpu.irq_line = self.bus.apu.irq();
            if self.bus.ppu.frame_ready {
                self.bus.ppu.frame_ready = false;
                break;
            }
            guard += cyc as u64;
            if self.cpu.jammed || guard > 300_000 {
                return false;
            }
        }
        self.frames += 1;
        true
    }

    pub fn framebuffer(&self) -> &[u32] {
        &self.bus.ppu.frame
    }
}
```

## src/cpu.rs

```rust

pub trait Bus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
}

pub const FLAG_C: u8 = 0x01;
pub const FLAG_Z: u8 = 0x02;
pub const FLAG_I: u8 = 0x04;
pub const FLAG_D: u8 = 0x08;
pub const FLAG_B: u8 = 0x10;
pub const FLAG_U: u8 = 0x20;
pub const FLAG_V: u8 = 0x40;
pub const FLAG_N: u8 = 0x80;

#[derive(Clone)]
pub struct Cpu {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub pc: u16,
    pub p: u8,
    pub nmi_pending: bool,
    pub irq_line: bool,
    pub cycles: u64,
    pub jammed: bool,
}

impl Cpu {
    pub fn new<B: Bus>(bus: &mut B) -> Cpu {
        let lo = bus.read(0xFFFC) as u16;
        let hi = bus.read(0xFFFD) as u16;
        Cpu {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: (hi << 8) | lo,
            p: FLAG_I | FLAG_U,
            nmi_pending: false,
            irq_line: false,
            cycles: 0,
            jammed: false,
        }
    }

    #[inline]
    fn read<B: Bus>(&mut self, bus: &mut B, addr: u16) -> u8 {
        bus.read(addr)
    }

    #[inline]
    fn write<B: Bus>(&mut self, bus: &mut B, addr: u16, v: u8) {
        bus.write(addr, v);
    }

    #[inline]
    fn fetch<B: Bus>(&mut self, bus: &mut B) -> u8 {
        let v = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }

    #[inline]
    fn fetch16<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.fetch(bus) as u16;
        let hi = self.fetch(bus) as u16;
        (hi << 8) | lo
    }

    #[inline]
    fn set_flag(&mut self, f: u8, on: bool) {
        if on {
            self.p |= f;
        } else {
            self.p &= !f;
        }
    }

    #[inline]
    fn flag(&self, f: u8) -> bool {
        self.p & f != 0
    }

    #[inline]
    fn nz(&mut self, v: u8) {
        self.set_flag(FLAG_Z, v == 0);
        self.set_flag(FLAG_N, v & 0x80 != 0);
    }

    fn push<B: Bus>(&mut self, bus: &mut B, v: u8) {
        bus.write(0x0100 | self.sp as u16, v);
        self.sp = self.sp.wrapping_sub(1);
    }

    fn pop<B: Bus>(&mut self, bus: &mut B) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        bus.read(0x0100 | self.sp as u16)
    }

    fn adc(&mut self, v: u8) {
        let a = self.a as u16;
        let carry = if self.flag(FLAG_C) { 1 } else { 0 };
        let sum = a + v as u16 + carry;
        let result = sum as u8;
        self.set_flag(FLAG_C, sum > 0xFF);
        let overflow = (!(self.a ^ v) & (self.a ^ result) & 0x80) != 0;
        self.set_flag(FLAG_V, overflow);
        self.a = result;
        self.nz(result);
    }

    fn sbc(&mut self, v: u8) {
        self.adc(v ^ 0xFF);
    }

    fn compare(&mut self, reg: u8, v: u8) {
        let r = reg.wrapping_sub(v);
        self.set_flag(FLAG_C, reg >= v);
        self.nz(r);
    }

    fn asl(&mut self, v: u8) -> u8 {
        self.set_flag(FLAG_C, v & 0x80 != 0);
        let r = v << 1;
        self.nz(r);
        r
    }

    fn lsr(&mut self, v: u8) -> u8 {
        self.set_flag(FLAG_C, v & 1 != 0);
        let r = v >> 1;
        self.nz(r);
        r
    }

    fn rol(&mut self, v: u8) -> u8 {
        let c = if self.flag(FLAG_C) { 1 } else { 0 };
        self.set_flag(FLAG_C, v & 0x80 != 0);
        let r = (v << 1) | c;
        self.nz(r);
        r
    }

    fn ror(&mut self, v: u8) -> u8 {
        let c = if self.flag(FLAG_C) { 0x80 } else { 0 };
        self.set_flag(FLAG_C, v & 1 != 0);
        let r = (v >> 1) | c;
        self.nz(r);
        r
    }

    fn branch<B: Bus>(&mut self, bus: &mut B, cond: bool, base: u32) -> u32 {
        if cond {
            let off = self.fetch(bus) as i8;
            let target = self.pc.wrapping_add(off as u16);
            let crossed = (target & 0xFF00) != (self.pc & 0xFF00);
            self.pc = target;
            base + 1 + if crossed { 1 } else { 0 }
        } else {
            self.pc = self.pc.wrapping_add(1);
            base
        }
    }

    fn am_zp<B: Bus>(&mut self, bus: &mut B) -> u16 {
        self.fetch(bus) as u16
    }
    fn am_zpx<B: Bus>(&mut self, bus: &mut B) -> u16 {
        (self.fetch(bus).wrapping_add(self.x)) as u16
    }
    fn am_zpy<B: Bus>(&mut self, bus: &mut B) -> u16 {
        (self.fetch(bus).wrapping_add(self.y)) as u16
    }
    fn am_abs<B: Bus>(&mut self, bus: &mut B) -> u16 {
        self.fetch16(bus)
    }
    fn am_absx<B: Bus>(&mut self, bus: &mut B) -> (u16, bool) {
        let base = self.fetch16(bus);
        let addr = base.wrapping_add(self.x as u16);
        (addr, (base & 0xFF00) != (addr & 0xFF00))
    }
    fn am_absy<B: Bus>(&mut self, bus: &mut B) -> (u16, bool) {
        let base = self.fetch16(bus);
        let addr = base.wrapping_add(self.y as u16);
        (addr, (base & 0xFF00) != (addr & 0xFF00))
    }
    fn am_indx<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let zp = self.fetch(bus).wrapping_add(self.x);
        let lo = bus.read(zp as u16) as u16;
        let hi = bus.read(zp.wrapping_add(1) as u16) as u16;
        (hi << 8) | lo
    }
    fn am_indy<B: Bus>(&mut self, bus: &mut B) -> (u16, bool) {
        let zp = self.fetch(bus);
        let lo = bus.read(zp as u16) as u16;
        let hi = bus.read(zp.wrapping_add(1) as u16) as u16;
        let base = (hi << 8) | lo;
        let addr = base.wrapping_add(self.y as u16);
        (addr, (base & 0xFF00) != (addr & 0xFF00))
    }
    fn am_ind<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let ptr = self.fetch16(bus);
        let lo = bus.read(ptr) as u16;
        let hi = bus.read((ptr & 0xFF00) | ((ptr + 1) & 0x00FF)) as u16;
        (hi << 8) | lo
    }

    fn interrupt<B: Bus>(&mut self, bus: &mut B, vector: u16, brk: bool) {
        let pc = self.pc;
        self.push(bus, (pc >> 8) as u8);
        self.push(bus, pc as u8);
        let mut flags = self.p | FLAG_U;
        if brk {
            flags |= FLAG_B;
        } else {
            flags &= !FLAG_B;
        }
        self.push(bus, flags);
        self.set_flag(FLAG_I, true);
        let lo = bus.read(vector) as u16;
        let hi = bus.read(vector + 1) as u16;
        self.pc = (hi << 8) | lo;
    }

    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        if self.nmi_pending {
            self.nmi_pending = false;
            self.interrupt(bus, 0xFFFA, false);
            self.cycles += 7;
            return 7;
        }
        if self.irq_line && !self.flag(FLAG_I) {
            self.interrupt(bus, 0xFFFE, false);
            self.cycles += 7;
            return 7;
        }

        let op = self.fetch(bus);
        let cyc = self.execute(bus, op);
        self.cycles += cyc as u64;
        cyc
    }

    fn execute<B: Bus>(&mut self, bus: &mut B, op: u8) -> u32 {
        match op {
            0xA9 => { let v = self.fetch(bus); self.a = v; self.nz(v); 2 }
            0xA5 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 3 }
            0xB5 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 4 }
            0xAD => { let a = self.am_abs(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 4 }
            0xBD => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 4 + p as u32 }
            0xB9 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 4 + p as u32 }
            0xA1 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 6 }
            0xB1 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 5 + p as u32 }
            0xA2 => { let v = self.fetch(bus); self.x = v; self.nz(v); 2 }
            0xA6 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.x = v; self.nz(v); 3 }
            0xB6 => { let a = self.am_zpy(bus); let v = self.read(bus, a); self.x = v; self.nz(v); 4 }
            0xAE => { let a = self.am_abs(bus); let v = self.read(bus, a); self.x = v; self.nz(v); 4 }
            0xBE => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.x = v; self.nz(v); 4 + p as u32 }
            0xA0 => { let v = self.fetch(bus); self.y = v; self.nz(v); 2 }
            0xA4 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.y = v; self.nz(v); 3 }
            0xB4 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.y = v; self.nz(v); 4 }
            0xAC => { let a = self.am_abs(bus); let v = self.read(bus, a); self.y = v; self.nz(v); 4 }
            0xBC => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.y = v; self.nz(v); 4 + p as u32 }
            0x85 => { let a = self.am_zp(bus); self.write(bus, a, self.a); 3 }
            0x95 => { let a = self.am_zpx(bus); self.write(bus, a, self.a); 4 }
            0x8D => { let a = self.am_abs(bus); self.write(bus, a, self.a); 4 }
            0x9D => { let a = self.am_absx(bus).0; self.write(bus, a, self.a); 5 }
            0x99 => { let a = self.am_absy(bus).0; self.write(bus, a, self.a); 5 }
            0x81 => { let a = self.am_indx(bus); self.write(bus, a, self.a); 6 }
            0x91 => { let a = self.am_indy(bus).0; self.write(bus, a, self.a); 6 }
            0x86 => { let a = self.am_zp(bus); self.write(bus, a, self.x); 3 }
            0x96 => { let a = self.am_zpy(bus); self.write(bus, a, self.x); 4 }
            0x8E => { let a = self.am_abs(bus); self.write(bus, a, self.x); 4 }
            0x84 => { let a = self.am_zp(bus); self.write(bus, a, self.y); 3 }
            0x94 => { let a = self.am_zpx(bus); self.write(bus, a, self.y); 4 }
            0x8C => { let a = self.am_abs(bus); self.write(bus, a, self.y); 4 }
            0xAA => { self.x = self.a; self.nz(self.x); 2 }
            0xA8 => { self.y = self.a; self.nz(self.y); 2 }
            0x8A => { self.a = self.x; self.nz(self.a); 2 }
            0x98 => { self.a = self.y; self.nz(self.a); 2 }
            0xBA => { self.x = self.sp; self.nz(self.x); 2 }
            0x9A => { self.sp = self.x; 2 }
            0x48 => { let v = self.a; self.push(bus, v); 3 }
            0x68 => { let v = self.pop(bus); self.a = v; self.nz(v); 4 }
            0x08 => { let v = self.p | FLAG_B | FLAG_U; self.push(bus, v); 3 }
            0x28 => { let v = self.pop(bus); self.p = (v | FLAG_U) & !FLAG_B; 4 }
            0x69 => { let v = self.fetch(bus); self.adc(v); 2 }
            0x65 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.adc(v); 3 }
            0x75 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.adc(v); 4 }
            0x6D => { let a = self.am_abs(bus); let v = self.read(bus, a); self.adc(v); 4 }
            0x7D => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.adc(v); 4 + p as u32 }
            0x79 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.adc(v); 4 + p as u32 }
            0x61 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.adc(v); 6 }
            0x71 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.adc(v); 5 + p as u32 }
            0xE9 => { let v = self.fetch(bus); self.sbc(v); 2 }
            0xE5 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.sbc(v); 3 }
            0xF5 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.sbc(v); 4 }
            0xED => { let a = self.am_abs(bus); let v = self.read(bus, a); self.sbc(v); 4 }
            0xFD => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.sbc(v); 4 + p as u32 }
            0xF9 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.sbc(v); 4 + p as u32 }
            0xE1 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.sbc(v); 6 }
            0xF1 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.sbc(v); 5 + p as u32 }
            0x29 => { let v = self.fetch(bus); self.a &= v; self.nz(self.a); 2 }
            0x25 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 3 }
            0x35 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 4 }
            0x2D => { let a = self.am_abs(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 4 }
            0x3D => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 4 + p as u32 }
            0x39 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 4 + p as u32 }
            0x21 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 6 }
            0x31 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 5 + p as u32 }
            0x09 => { let v = self.fetch(bus); self.a |= v; self.nz(self.a); 2 }
            0x05 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 3 }
            0x15 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 4 }
            0x0D => { let a = self.am_abs(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 4 }
            0x1D => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 4 + p as u32 }
            0x19 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 4 + p as u32 }
            0x01 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 6 }
            0x11 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 5 + p as u32 }
            0x49 => { let v = self.fetch(bus); self.a ^= v; self.nz(self.a); 2 }
            0x45 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 3 }
            0x55 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 4 }
            0x4D => { let a = self.am_abs(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 4 }
            0x5D => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 4 + p as u32 }
            0x59 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 4 + p as u32 }
            0x41 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 6 }
            0x51 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 5 + p as u32 }
            0xC9 => { let v = self.fetch(bus); self.compare(self.a, v); 2 }
            0xC5 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.compare(self.a, v); 3 }
            0xD5 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.compare(self.a, v); 4 }
            0xCD => { let a = self.am_abs(bus); let v = self.read(bus, a); self.compare(self.a, v); 4 }
            0xDD => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.compare(self.a, v); 4 + p as u32 }
            0xD9 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.compare(self.a, v); 4 + p as u32 }
            0xC1 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.compare(self.a, v); 6 }
            0xD1 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.compare(self.a, v); 5 + p as u32 }
            0xE0 => { let v = self.fetch(bus); self.compare(self.x, v); 2 }
            0xE4 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.compare(self.x, v); 3 }
            0xEC => { let a = self.am_abs(bus); let v = self.read(bus, a); self.compare(self.x, v); 4 }
            0xC0 => { let v = self.fetch(bus); self.compare(self.y, v); 2 }
            0xC4 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.compare(self.y, v); 3 }
            0xCC => { let a = self.am_abs(bus); let v = self.read(bus, a); self.compare(self.y, v); 4 }
            0x24 => {
                let a = self.am_zp(bus); let v = self.read(bus, a);
                self.set_flag(FLAG_Z, self.a & v == 0);
                self.set_flag(FLAG_N, v & 0x80 != 0);
                self.set_flag(FLAG_V, v & 0x40 != 0);
                3
            }
            0x2C => {
                let a = self.am_abs(bus); let v = self.read(bus, a);
                self.set_flag(FLAG_Z, self.a & v == 0);
                self.set_flag(FLAG_N, v & 0x80 != 0);
                self.set_flag(FLAG_V, v & 0x40 != 0);
                4
            }
            0x06 => { let a = self.am_zp(bus); let v = self.read(bus, a); let r = self.asl(v); self.write(bus, a, r); 5 }
            0x16 => { let a = self.am_zpx(bus); let v = self.read(bus, a); let r = self.asl(v); self.write(bus, a, r); 6 }
            0x0E => { let a = self.am_abs(bus); let v = self.read(bus, a); let r = self.asl(v); self.write(bus, a, r); 6 }
            0x1E => { let a = self.am_absx(bus).0; let v = self.read(bus, a); let r = self.asl(v); self.write(bus, a, r); 7 }
            0x46 => { let a = self.am_zp(bus); let v = self.read(bus, a); let r = self.lsr(v); self.write(bus, a, r); 5 }
            0x56 => { let a = self.am_zpx(bus); let v = self.read(bus, a); let r = self.lsr(v); self.write(bus, a, r); 6 }
            0x4E => { let a = self.am_abs(bus); let v = self.read(bus, a); let r = self.lsr(v); self.write(bus, a, r); 6 }
            0x5E => { let a = self.am_absx(bus).0; let v = self.read(bus, a); let r = self.lsr(v); self.write(bus, a, r); 7 }
            0x26 => { let a = self.am_zp(bus); let v = self.read(bus, a); let r = self.rol(v); self.write(bus, a, r); 5 }
            0x36 => { let a = self.am_zpx(bus); let v = self.read(bus, a); let r = self.rol(v); self.write(bus, a, r); 6 }
            0x2E => { let a = self.am_abs(bus); let v = self.read(bus, a); let r = self.rol(v); self.write(bus, a, r); 6 }
            0x3E => { let a = self.am_absx(bus).0; let v = self.read(bus, a); let r = self.rol(v); self.write(bus, a, r); 7 }
            0x66 => { let a = self.am_zp(bus); let v = self.read(bus, a); let r = self.ror(v); self.write(bus, a, r); 5 }
            0x76 => { let a = self.am_zpx(bus); let v = self.read(bus, a); let r = self.ror(v); self.write(bus, a, r); 6 }
            0x6E => { let a = self.am_abs(bus); let v = self.read(bus, a); let r = self.ror(v); self.write(bus, a, r); 6 }
            0x7E => { let a = self.am_absx(bus).0; let v = self.read(bus, a); let r = self.ror(v); self.write(bus, a, r); 7 }
            0x0A => { self.a = self.asl(self.a); 2 }
            0x4A => { self.a = self.lsr(self.a); 2 }
            0x2A => { self.a = self.rol(self.a); 2 }
            0x6A => { self.a = self.ror(self.a); 2 }
            0xE6 => { let a = self.am_zp(bus); let v = self.read(bus, a).wrapping_add(1); self.write(bus, a, v); self.nz(v); 5 }
            0xF6 => { let a = self.am_zpx(bus); let v = self.read(bus, a).wrapping_add(1); self.write(bus, a, v); self.nz(v); 6 }
            0xEE => { let a = self.am_abs(bus); let v = self.read(bus, a).wrapping_add(1); self.write(bus, a, v); self.nz(v); 6 }
            0xFE => { let a = self.am_absx(bus).0; let v = self.read(bus, a).wrapping_add(1); self.write(bus, a, v); self.nz(v); 7 }
            0xC6 => { let a = self.am_zp(bus); let v = self.read(bus, a).wrapping_sub(1); self.write(bus, a, v); self.nz(v); 5 }
            0xD6 => { let a = self.am_zpx(bus); let v = self.read(bus, a).wrapping_sub(1); self.write(bus, a, v); self.nz(v); 6 }
            0xCE => { let a = self.am_abs(bus); let v = self.read(bus, a).wrapping_sub(1); self.write(bus, a, v); self.nz(v); 6 }
            0xDE => { let a = self.am_absx(bus).0; let v = self.read(bus, a).wrapping_sub(1); self.write(bus, a, v); self.nz(v); 7 }
            0xE8 => { self.x = self.x.wrapping_add(1); self.nz(self.x); 2 }
            0xCA => { self.x = self.x.wrapping_sub(1); self.nz(self.x); 2 }
            0xC8 => { self.y = self.y.wrapping_add(1); self.nz(self.y); 2 }
            0x88 => { self.y = self.y.wrapping_sub(1); self.nz(self.y); 2 }
            0x18 => { self.set_flag(FLAG_C, false); 2 }
            0x38 => { self.set_flag(FLAG_C, true); 2 }
            0x58 => { self.set_flag(FLAG_I, false); 2 }
            0x78 => { self.set_flag(FLAG_I, true); 2 }
            0xB8 => { self.set_flag(FLAG_V, false); 2 }
            0xD8 => { self.set_flag(FLAG_D, false); 2 }
            0xF8 => { self.set_flag(FLAG_D, true); 2 }
            0x10 => self.branch(bus, !self.flag(FLAG_N), 2),
            0x30 => self.branch(bus, self.flag(FLAG_N), 2),
            0x50 => self.branch(bus, !self.flag(FLAG_V), 2),
            0x70 => self.branch(bus, self.flag(FLAG_V), 2),
            0x90 => self.branch(bus, !self.flag(FLAG_C), 2),
            0xB0 => self.branch(bus, self.flag(FLAG_C), 2),
            0xD0 => self.branch(bus, !self.flag(FLAG_Z), 2),
            0xF0 => self.branch(bus, self.flag(FLAG_Z), 2),
            0x4C => { self.pc = self.am_abs(bus); 3 }
            0x6C => { self.pc = self.am_ind(bus); 5 }
            0x20 => {
                let target = self.fetch16(bus);
                let ret = self.pc.wrapping_sub(1);
                self.push(bus, (ret >> 8) as u8);
                self.push(bus, ret as u8);
                self.pc = target;
                6
            }
            0x60 => {
                let lo = self.pop(bus) as u16;
                let hi = self.pop(bus) as u16;
                self.pc = ((hi << 8) | lo).wrapping_add(1);
                6
            }
            0x40 => {
                let p = self.pop(bus);
                self.p = (p | FLAG_U) & !FLAG_B;
                let lo = self.pop(bus) as u16;
                let hi = self.pop(bus) as u16;
                self.pc = (hi << 8) | lo;
                6
            }
            0x00 => {
                self.pc = self.pc.wrapping_add(1);
                self.interrupt(bus, 0xFFFE, true);
                7
            }
            0xEA => 2,
            0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => 2,
            0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => { self.fetch(bus); 2 }
            0x04 | 0x44 | 0x64 => { self.am_zp(bus); 3 }
            0x0C => { self.am_abs(bus); 4 }
            0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => { self.am_zpx(bus); 4 }
            0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => { let (_, p) = self.am_absx(bus); 4 + p as u32 }
            _ => {
                self.pc = self.pc.wrapping_sub(1);
                self.jammed = true;
                2
            }
        }
    }
}
```

## Cargo.toml

```toml
[package]
name = "smb-rs"
version = "0.1.0"
edition = "2021"
description = "NES (NROM) emulator + Super Mario Bros. reverse-engineering toolchain, in Rust"
license = "MIT"

[dependencies]
minifb = "0.29"
png = "0.17"
cpal = { version = "0.15", optional = true }

[features]
default = ["audio"]
audio = ["dep:cpal"]

[profile.release]
opt-level = 3
lto = true
codegen-units = 1

[[bin]]
name = "smb-rs"
path = "src/main.rs"

[[bin]]
name = "nesdump"
path = "src/bin/nesdump.rs"
```
