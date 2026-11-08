//! playbot <rom> <outdir> [--frames 3000] [--scale 2] [--trick showcase|speedrun|fpg|double-friction|minus]
//!         [--fpg-x 3036] [--macro "x=140..145:SEQ(4:L+B,2:L+R+B,12:R+B+A);x=3036..3040:SEQ(18:R+B+A)"]
//!
//! Motor TAS Heuristic + Inyector de Trucos Programable por RAM/Subpixel/FrameRule.
//! Genera frame_*.png + telemetry.csv (100% compatible con make_video.py)
//! + replay_script.txt (compatible con nesdump --script) + tricks_audit.log.

use std::path::PathBuf;
use smb_rs::console::*;
use smb_rs::Cartridge;

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
fn is_solid(ram: &[u8; 2048], wx: i32, wy: i32) -> bool {
    world_tile_kind(ram, wx, wy) == 1
}

#[inline]
fn is_pit(ram: &[u8; 2048], wx: i32) -> bool {
    !is_solid(ram, wx, 208) && !is_solid(ram, wx, 224)
}

// ---------------- Motor de Macros y Secuencias TAS (DSL) ----------------

#[derive(Clone, Debug)]
struct TriggerRule {
    name: String,
    frame_range: Option<(u64, u64)>,
    lframe_range: Option<(u64, u64)>,
    x_range: Option<(i32, i32)>,
    y_range: Option<(i32, i32)>,
    fr_range: Option<(i32, i32)>,   // $077F Frame Rule (0..20)
    sub_range: Option<(i32, i32)>,  // $0705 Subpixel MoveForce (0..255)
    spd_range: Option<(i32, i32)>,  // $0057 X Speed (-40..40)
    st_range: Option<(i32, i32)>,   // $001D Player_State (0=suelo, 1=aire, 3=mastil)
    once: bool,
    fired: bool,
    sequence: Vec<u8>,              // Botones frame por frame al dispararse
}

impl TriggerRule {
    fn matches(
        &self,
        frame: u64,
        lframe: u64,
        wx: i32,
        wy: i32,
        fr: i32,
        sub: i32,
        spd: i32,
        st: i32,
    ) -> bool {
        if self.once && self.fired {
            return false;
        }
        if let Some((a, b)) = self.frame_range {
            if frame < a || frame > b {
                return false;
            }
        }
        if let Some((a, b)) = self.lframe_range {
            if lframe < a || lframe > b {
                return false;
            }
        }
        if let Some((a, b)) = self.x_range {
            if wx < a || wx > b {
                return false;
            }
        }
        if let Some((a, b)) = self.y_range {
            if wy < a || wy > b {
                return false;
            }
        }
        if let Some((a, b)) = self.fr_range {
            if fr < a || fr > b {
                return false;
            }
        }
        if let Some((a, b)) = self.sub_range {
            if sub < a || sub > b {
                return false;
            }
        }
        if let Some((a, b)) = self.spd_range {
            if spd < a || spd > b {
                return false;
            }
        }
        if let Some((a, b)) = self.st_range {
            if st < a || st > b {
                return false;
            }
        }
        true
    }
}

fn parse_buttons(spec: &str) -> u8 {
    let mut b = 0u8;
    for tok in spec.split('+') {
        match tok.trim().to_uppercase().as_str() {
            "A" => b |= BUTTON_A,
            "B" => b |= BUTTON_B,
            "SELECT" | "E" => b |= BUTTON_SELECT,
            "START" | "S" => b |= BUTTON_START,
            "UP" | "U" => b |= BUTTON_UP,
            "DOWN" | "D" => b |= BUTTON_DOWN,
            "LEFT" | "L" => b |= BUTTON_LEFT,
            "RIGHT" | "R" => b |= BUTTON_RIGHT,
            "NONE" | "0" | "" => {}
            other => {
                if let Ok(raw) = other.parse::<u8>() {
                    b |= raw;
                }
            }
        }
    }
    b
}

fn format_buttons(b: u8) -> String {
    if b == 0 {
        return "NONE".to_string();
    }
    let mut parts = Vec::new();
    if b & BUTTON_LEFT != 0 { parts.push("L"); }
    if b & BUTTON_RIGHT != 0 { parts.push("R"); }
    if b & BUTTON_UP != 0 { parts.push("U"); }
    if b & BUTTON_DOWN != 0 { parts.push("D"); }
    if b & BUTTON_B != 0 { parts.push("B"); }
    if b & BUTTON_A != 0 { parts.push("A"); }
    if b & BUTTON_SELECT != 0 { parts.push("SEL"); }
    if b & BUTTON_START != 0 { parts.push("START"); }
    parts.join("+")
}

fn parse_range_i32(s: &str) -> Option<(i32, i32)> {
    if let Some((a, b)) = s.split_once("..") {
        let lo = a.trim().parse::<i32>().ok()?;
        let hi = b.trim().parse::<i32>().ok()?;
        Some((lo, hi))
    } else {
        let v = s.trim().parse::<i32>().ok()?;
        Some((v, v))
    }
}

fn parse_range_u64(s: &str) -> Option<(u64, u64)> {
    if let Some((a, b)) = s.split_once("..") {
        let lo = a.trim().parse::<u64>().ok()?;
        let hi = b.trim().parse::<u64>().ok()?;
        Some((lo, hi))
    } else {
        let v = s.trim().parse::<u64>().ok()?;
        Some((v, v))
    }
}

/// Parsea reglas de trucos como:
/// "x=120..126@spd=36..40:SEQ(4:L+B,2:L+R+B,8:R+B);x=3035..3039:SEQ(16:R+B+A)"
fn parse_macro_rules(dsl: &str) -> Vec<TriggerRule> {
    let mut rules = Vec::new();
    for (idx, raw_rule) in dsl.split(';').enumerate() {
        let line = raw_rule.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((cond_part, act_part)) = line.split_once(':') else {
            continue;
        };
        let mut rule = TriggerRule {
            name: format!("macro_{}", idx + 1),
            frame_range: None,
            lframe_range: None,
            x_range: None,
            y_range: None,
            fr_range: None,
            sub_range: None,
            spd_range: None,
            st_range: None,
            once: true,
            fired: false,
            sequence: Vec::new(),
        };

        for cond in cond_part.split('@') {
            let Some((k, v)) = cond.split_once('=') else {
                continue;
            };
            match k.trim() {
                "name" => rule.name = v.trim().to_string(),
                "f" | "frame" => rule.frame_range = parse_range_u64(v),
                "lf" | "lframe" => rule.lframe_range = parse_range_u64(v),
                "x" => rule.x_range = parse_range_i32(v),
                "y" => rule.y_range = parse_range_i32(v),
                "fr" => rule.fr_range = parse_range_i32(v),
                "sub" => rule.sub_range = parse_range_i32(v),
                "spd" => rule.spd_range = parse_range_i32(v),
                "st" => rule.st_range = parse_range_i32(v),
                "repeat" => {
                    rule.once = v.trim() != "1" && !v.trim().eq_ignore_ascii_case("true");
                }
                _ => {}
            }
        }

        let act_trim = act_part.trim();
        if act_trim.to_uppercase().starts_with("SEQ(") && act_trim.ends_with(')') {
            let inner = &act_trim[4..act_trim.len() - 1];
            for step in inner.split(',') {
                if let Some((cnt_s, btn_s)) = step.trim().split_once(':') {
                    let count = cnt_s.trim().parse::<usize>().unwrap_or(1);
                    let b = parse_buttons(btn_s);
                    for _ in 0..count {
                        rule.sequence.push(b);
                    }
                }
            }
        } else {
            rule.sequence.push(parse_buttons(act_trim));
            // Si es un rango directo (ej. x=100..110:L+B) sin SEQ, se evalua en cada frame del rango
            rule.once = false;
        }

        if !rule.sequence.is_empty() {
            rules.push(rule);
        }
    }
    rules
}

fn build_preset_tricks(mode: &str, fpg_x: i32) -> Vec<TriggerRule> {
    let mut rules = Vec::new();
    match mode {
        "showcase" => {
            // 1) Demostracion de Giro con Doble Friccion ($B57F-$B588: ASL $0702; ROL $0701)
            //    Cuando Mario alcanza velocidad en suelo plano, invierte direccion 4 frames (facing != moving)
            //    y luego inyecta L+R (moonwalk lock) antes de retomar velocidad maxima.
            rules.extend(parse_macro_rules(
                "name=DoubleFriction_Turnaround@x=136..142@st=0@spd=30..40:SEQ(4:L+B,2:L+R+B,10:R+B)",
            ));
            // 2) Demostracion de L+R Facing/Walk Lock en tramo plano despues de la 1ra tuberia
            rules.extend(parse_macro_rules(
                "name=LR_Moonwalk_Pulse@x=518..524@st=0:SEQ(2:L+R+B,2:R+B,2:L+R+B,6:R+B)",
            ));
            // 3) Setup de Flagpole Glitch / Impacto de precision en el mastil ($DE29 / $DE88)
            let fpg_rule = format!(
                "name=Flagpole_Precision_Launch@x={}..{}@st=0:SEQ(15:R+B+A,30:R+B)",
                fpg_x,
                fpg_x + 5
            );
            rules.extend(parse_macro_rules(&fpg_rule));
        }
        "double-friction" => {
            rules.extend(parse_macro_rules(
                "name=DoubleFriction_1@x=135..142@st=0:SEQ(5:L+B,15:R+B);\
                 name=DoubleFriction_2@x=230..238@st=0:SEQ(4:L+B,2:L+R+B,12:R+B)",
            ));
        }
        "fpg" | "speedrun" => {
            let fpg_rule = format!(
                "name=Flagpole_TAS_Launch@x={}..{}@st=0:SEQ(15:R+B+A,35:R+B)",
                fpg_x,
                fpg_x + 5
            );
            rules.extend(parse_macro_rules(&fpg_rule));
        }
        "minus" => {
            // Setup de eyeccion de pared izquierda al final de 1-2 para Minus World ($87F2 -> $24)
            rules.extend(parse_macro_rules(
                "name=MinusWorld_WallClip@x=2644..2652@st=0:SEQ(6:L,4:D,8:L+A,18:R+U,20:D)",
            ));
        }
        _ => {}
    }
    rules
}

// ---------------- Escaneo Balistico del Entorno ($0500 + Enemigos) ----------------

struct EnvScan {
    wall_dist: i32,
    wall_height: i32,
    wall_top_y: i32,
    pit_dist: i32,
    pit_width: i32,
    enemy_dist: i32,
    enemy_height: i32,
    low_ceiling: bool,
}

fn scan_environment(ram: &[u8; 2048], x: i32, feet: i32) -> EnvScan {
    let front_x = x + 12;
    let row_low = (feet - 6).clamp(32, 236);
    let row_mid = (feet - 20).clamp(32, 236);

    // 1. Distancia y altura exacta del muro/tuberia/escalon mas cercano
    let mut wall_dist = 999i32;
    let mut wall_top_y = feet;
    for d in 0..=72 {
        let wx = front_x + d;
        if is_solid(ram, wx, row_low) || is_solid(ram, wx, row_mid) {
            wall_dist = d;
            // Buscar hasta que altura sube este muro/tuberia para hacer un Low Jump exacto
            let mut ty = row_low;
            while ty >= 48 && is_solid(ram, wx, ty) {
                wall_top_y = ty;
                ty -= 16;
            }
            break;
        }
    }
    let wall_height = (feet - wall_top_y).max(0);

    // 2. Distancia y anchura exacta del pozo (pit)
    let mut pit_dist = 999i32;
    let mut pit_width = 0i32;
    for d in 0..=96 {
        let wx = front_x + d;
        if is_pit(ram, wx) {
            pit_dist = d;
            let mut w = 0;
            while is_pit(ram, wx + w) && w < 96 {
                w += 1;
            }
            pit_width = w;
            break;
        }
    }

    // 3. Enemigos activos en los 5 slots ($000F..$0013)
    let mut enemy_dist = 999i32;
    let mut enemy_height = 16i32;
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
        if ewx >= x - 6 && ewx <= x + 96 {
            let d = ewx - front_x;
            if d >= -4 && d < enemy_dist {
                enemy_dist = d;
                enemy_height = (feet - ewy).clamp(12, 48);
            }
        }
    }

    // 4. Deteccion de techo bajo (? blocks / ladrillos en wy = feet - 48..feet - 30)
    //    Evita que Mario se golpee la cabeza y pierda su velocidad horizontal ($B51C).
    let head_check_y = (feet - 42).clamp(32, 220);
    let low_ceiling = is_solid(ram, x + 6, head_check_y)
        || is_solid(ram, x + 14, head_check_y)
        || is_solid(ram, x + 22, head_check_y);

    EnvScan {
        wall_dist,
        wall_height,
        wall_top_y,
        pit_dist,
        pit_width,
        enemy_dist,
        enemy_height,
        low_ceiling,
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "Uso: playbot <rom> <outdir> [--frames N] [--scale N] \
             [--trick showcase|speedrun|fpg|double-friction|minus] \
             [--fpg-x 3036] [--macro \"regla1;regla2\"]"
        );
        std::process::exit(2);
    }

    let rom = PathBuf::from(&args[1]);
    let outdir = PathBuf::from(&args[2]);
    let mut max_frame: u64 = 3000;
    let mut scale: usize = 2;
    let mut trick_mode = String::from("showcase");
    let mut fpg_x: i32 = 3036; // Cresta del ultimo escalon en 1-1
    let mut custom_macro_dsl = String::new();

    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--frames" => {
                i += 1;
                if i < args.len() {
                    max_frame = args[i].parse().unwrap_or(3000);
                }
            }
            "--scale" => {
                i += 1;
                if i < args.len() {
                    scale = args[i].parse().unwrap_or(2);
                }
            }
            "--trick" => {
                i += 1;
                if i < args.len() {
                    trick_mode = args[i].clone();
                }
            }
            "--fpg-x" => {
                i += 1;
                if i < args.len() {
                    fpg_x = args[i].parse().unwrap_or(3036);
                }
            }
            "--macro" => {
                i += 1;
                if i < args.len() {
                    if !custom_macro_dsl.is_empty() {
                        custom_macro_dsl.push(';');
                    }
                    custom_macro_dsl.push_str(&args[i]);
                }
            }
            _ => {}
        }
        i += 1;
    }

    let mut rules = build_preset_tricks(&trick_mode, fpg_x);
    if !custom_macro_dsl.is_empty() {
        rules.extend(parse_macro_rules(&custom_macro_dsl));
    }

    let cart = Cartridge::load(&rom).expect("No se pudo cargar la ROM");
    let mut con = Console::new(cart);
    std::fs::create_dir_all(&outdir).ok();

    let mut csv = String::with_capacity(256 * 1024);
    csv.push_str("frame,buttons,x_spd,x_moveforce,y_spd,x_pos,x_page,y_pos,state,facing,moving,x_abs,interval,screentimer,timer_hi,x_abs_dec\n");

    let mut audit_log = String::new();
    audit_log.push_str("=== PLAYBOT TAS TRICK & TELEMETRY AUDIT LOG ===\n");
    audit_log.push_str(&format!("Modo de trucos: {} | Reglas cargadas: {}\n\n", trick_mode, rules.len()));

    // Cola para secuencias multi-frame activas (SEQ)
    let mut active_seq: Vec<u8> = Vec::new();
    let mut active_seq_idx: usize = 0;
    let mut active_trick_name = String::new();

    // Estado del controlador balistico de Low Jumps
    let mut jump_hold: usize = 0;
    let mut jump_cooldown: usize = 0;
    let mut target_clear_y: i32 = 0;
    let mut jumping_over_pit = false;
    let mut air_frames: usize = 0;
    let mut prev_buttons: u8 = 0;
    let mut script_events: Vec<String> = Vec::new();
    let mut debug_prev_x: i32 = 0;
    let mut stuck_cnt: usize = 0;
    let mut unstick_cnt: usize = 0;

    let mut phase: u8 = 0; // 0 = Boot/Title, 1 = Jugando en nivel
    let mut level_start_frame: u64 = 0;
    let mut flagpole_touched_frame: Option<u64> = None;
    let mut victory_after_frames: usize = 0;

    for frame in 0..=max_frame {
        let ram = &con.bus.ram;
        let x = (ram[0x6D] as i32) * 256 + (ram[0x86] as i32);
        let y_pos_raw = ram[0xCE] as i32;
        let op = ram[0x0770];
        let timer = ((ram[0x07F8] >> 4) as i32) * 10 + (ram[0x07F8] & 0x0F) as i32;
        let st = ram[0x000E];
        let player_state = ram[0x001D] as i32;
        let on_ground = player_state == 0;
        let vx = (ram[0x57] as i8) as i32;
        let vy = (ram[0x9F] as i8) as i32;
        let sub_mf = ram[0x0705] as i32;
        let fr_timer = ram[0x077F] as i32;

        let mut buttons = 0u8;

        if phase == 0 {
            if op == 0x01 && (1..=3).contains(&timer) && st == 0x08 && x > 30 && on_ground {
                phase = 1;
                level_start_frame = frame;
                let msg = format!(
                    "[BOOT] Nivel iniciado en frame={frame} | x={x} | frame_rule($077F)={fr_timer} | sub($0705)={sub_mf}"
                );
                eprintln!("{msg}");
                audit_log.push_str(&format!("{msg}\n"));
            } else if frame % 30 < 2 {
                buttons = BUTTON_START;
            }
        } else {
            let lframe = frame.saturating_sub(level_start_frame);
            if on_ground {
                air_frames = 0;
            } else {
                air_frames += 1;
            }

            // 1. Verificar si alguna regla de truco / macro se dispara en este frame
            if active_seq_idx >= active_seq.len() {
                active_seq.clear();
                active_seq_idx = 0;
                for rule in rules.iter_mut() {
                    if rule.matches(frame, lframe, x, y_pos_raw, fr_timer, sub_mf, vx, player_state) {
                        rule.fired = true;
                        active_seq = rule.sequence.clone();
                        active_seq_idx = 0;
                        active_trick_name = rule.name.clone();
                        let msg = format!(
                            "[TRICK DISPARADO] '{}' en f={frame} (lf={lframe}) | x={x} (nybble=${:01X}) | spd={vx} | sub={sub_mf} | fr($077F)={fr_timer} | len={}f",
                            rule.name, x & 0x0F, active_seq.len()
                        );
                        eprintln!("{msg}");
                        audit_log.push_str(&format!("{msg}\n"));
                        break;
                    }
                }
            }

            // 2. Si hay una secuencia de truco activa, tiene prioridad absoluta sobre los botones
            if active_seq_idx < active_seq.len() {
                buttons = active_seq[active_seq_idx];
                active_seq_idx += 1;
                jump_hold = 0; // Sincronizar estado de salto
            } else {
                // 3. Controlador Balistico TAS (Aceleracion Optima $B51C + Low Jumps)
                // Stuck detector para escaleras (x=2914): si no avanza 40f, retrocede y salta con impulso
                if on_ground && x == debug_prev_x && vx.abs() < 6 {
                    stuck_cnt += 1;
                } else {
                    stuck_cnt = 0;
                }
                debug_prev_x = x;
                if stuck_cnt > 40 && unstick_cnt == 0 {
                    unstick_cnt = 38; // 10f atras + 2f parado + 26f salto con impulso
                    audit_log.push_str(&format!("[UNSTUCK] f={frame} x={x} stuck>40 -> backstep+jump"));
                }
                if unstick_cnt > 0 {
                    unstick_cnt -= 1;
                    jump_hold = 0;
                    if unstick_cnt > 28 {
                        buttons = BUTTON_LEFT; // retroceder sin A (crea flanco)
                    } else if unstick_cnt > 26 {
                        buttons = 0; // pausa 2f para flanco limpio
                    } else {
                        buttons = BUTTON_RIGHT | BUTTON_B | BUTTON_A; // salto completo
                    }
                } else {
                    buttons = BUTTON_RIGHT | BUTTON_B;
                }

                let feet = {
                    let ypix = ram[0x03B8] as i32;
                    let power = ram[0x0756];
                    (ypix + if power == 0 { 32 } else { 48 }).clamp(32, 239)
                };

                let scan = scan_environment(ram, x, feet);

                if on_ground {
                    jumping_over_pit = false;
                    let mut desired_hold: usize = 0;

                    // Distancia dinamica de despegue segun la velocidad horizontal actual ($0057)
                    let pipe_takeoff = (14 + vx).clamp(24, 52);
                    let enemy_takeoff = (10 + (vx * 3) / 4).clamp(20, 38);

                    // A) Pozo delante: saltar en el ultimo momento posible con la altura justa
                    if scan.pit_dist <= 38 && scan.pit_width < 96 {
                        desired_hold = (7 + (scan.pit_width as usize) / 4).clamp(7, 20);
                        jumping_over_pit = true;
                        target_clear_y = feet - 24;
                    }
                    // B) Tuberia / Muro / Escalera delante: Low-Jump calculado segun la altura del muro
                    else if scan.wall_dist <= pipe_takeoff {
                        // FIX escaleras grandes: con velocidad, 6f se queda corto en el primer escalon.
                        if vx < 12 {
                            desired_hold = 23;
                        } else if scan.wall_height <= 18 {
                            desired_hold = 12;  // 1 bloque: salto medio (antes 6f, insuficiente en escalera grande)
                        } else if scan.wall_height <= 34 {
                            desired_hold = 16; // 2 bloques
                        } else if scan.wall_height <= 50 {
                            desired_hold = 20; // 3 bloques
                        } else {
                            desired_hold = 25; // 4+ bloques
                        }
                        target_clear_y = scan.wall_top_y - 2;
                    }
                    // C) Enemigo cercano: salto rasante (Low Jump de 6 frames) para perder 0 inercia
                    else if scan.enemy_dist >= 0 && scan.enemy_dist <= enemy_takeoff {
                        desired_hold = if scan.low_ceiling { 4 } else { 6 };
                        target_clear_y = feet - scan.enemy_height - 4;
                    }
                    // D) Desatasco de emergencia si Mario choco contra una pared (vx == 0)
                    else if vx == 0 && scan.wall_dist < 64 {
                        desired_hold = 18;
                        target_clear_y = scan.wall_top_y - 4;
                    }

                    // Si hay un bloque '?' justo encima y no es una tuberia alta, limitar el salto
                    if scan.low_ceiling && scan.wall_height <= 18 && !jumping_over_pit {
                        desired_hold = desired_hold.min(5);
                    }

                    if desired_hold > 0 && jump_hold == 0 && jump_cooldown == 0 {
                        jump_hold = desired_hold;
                        audit_log.push_str(&format!(
                            "[LOW-JUMP] f={frame} x={x} spd={vx} sub={sub_mf} | hold={jump_hold}f | wall_d={} wall_h={} pit_d={} enem_d={}\n",
                            scan.wall_dist, scan.wall_height, scan.pit_dist, scan.enemy_dist
                        ));
                    }
                } else {
                    // EN EL AIRE: Corte Balistico Temprano (True Low-Jump Release)
                    // Si Mario ya tiene suficiente altura/impulso vertical para librar el borde
                    // del obstaculo y no esta cruzando un pozo ancho, soltar 'A' inmediatamente.
                    let predicted_feet = feet + vy * 3;
                    if jump_hold > 0
                        && !jumping_over_pit
                        && air_frames >= 4
                        && vy < 0
                        && predicted_feet <= target_clear_y
                        && scan.wall_dist > 14
                    {
                        jump_hold = 0;
                    }
                }

                if jump_cooldown > 0 {
                    jump_cooldown -= 1;
                }
                if jump_hold > 0 {
                    buttons |= BUTTON_A;
                    jump_hold -= 1;
                    if jump_hold == 0 {
                        jump_cooldown = 3; // 3 frames sin A para crear flanco en el siguiente salto (ProcJumping exige PreviousA=0)
                    }
                }
            }
        }

        // Registrar cambios de botones para generar el script determinista de nesdump (--script)
        if buttons != prev_buttons {
            let pairs = [
                (BUTTON_A, "a"),
                (BUTTON_B, "b"),
                (BUTTON_SELECT, "select"),
                (BUTTON_START, "start"),
                (BUTTON_UP, "up"),
                (BUTTON_DOWN, "down"),
                (BUTTON_LEFT, "left"),
                (BUTTON_RIGHT, "right"),
            ];
            for (mask, name) in pairs {
                if (buttons & mask) != (prev_buttons & mask) {
                    let val = if (buttons & mask) != 0 { 1 } else { 0 };
                    script_events.push(format!("{frame}:{name}:{val}"));
                }
            }
            prev_buttons = buttons;
        }

        con.set_buttons(0, buttons);
        if !con.step_frame() {
            eprintln!("Emulador detenido en frame {frame}");
            break;
        }
        con.bus.apu.samples.clear();

        let r = &con.bus.ram;
        let x_spd = r[0x57] as i8 as i32;
        let y_spd = r[0x9F] as i8 as i32;
        let x_moveforce = r[0x0705 & 0x7FF] as i32;
        let x_pos = r[0x86] as i32;
        let x_page = r[0x6D] as i32;
        let y_pos = r[0xCE] as i32;
        let state = r[0x1D] as i32;
        let facing = r[0x33] as i32;
        let moving = r[0x45] as i32;
        let x_abs = r[0x0700 & 0x7FF] as i32;
        let interval = r[0x077F & 0x7FF] as i32;
        let screentimer = r[0x07A0 & 0x7FF] as i32;
        let timer_hi = r[0x07F8 & 0x7FF] as i32;
        let cur_x = x_page * 256 + x_pos;

        // Detectar contacto exacto con el mastil de la bandera ($DE29 / $DE88: state == 3 o subrutina 4/5)
        if phase == 1 && flagpole_touched_frame.is_none() && (state == 3 || r[0x000E] == 4 || r[0x000E] == 5) {
            flagpole_touched_frame = Some(frame);
            let lframes = frame.saturating_sub(level_start_frame);
            let flag_score_idx = r[0x010F];
            let msg = format!(
                "\n>>> [BANDERA ALCANZADA] frame_global={frame} | frames_nivel={lframes} ({:.2}s) | x_pre={x} (nybble=${:01X}) -> x_snap={cur_x} | y_pos={y_pos} | frame_rule($077F)={interval} | score_idx($010F)={flag_score_idx} <<<\n",
                (frame as f64) / 60.0988,
                x & 0x0F
            );
            eprintln!("{msg}");
            audit_log.push_str(&msg);
        }

        // Detectar activacion de Doble Friccion ($B57F: facing != moving en suelo con velocidad)
        if phase == 1 && on_ground && facing != moving && x_spd != 0 && moving != 0 {
            audit_log.push_str(&format!(
                "[FISICA $B57F DOBLE FRICCION] f={frame} x={cur_x} | facing={facing} != moving={moving} | spd={x_spd} mf={x_moveforce} | btns={}\n",
                format_buttons(buttons)
            ));
        }

        // Guardar fila de telemetria (16 columnas exactas para make_video.py)
        csv.push_str(&format!(
            "{frame},{buttons},{x_spd},{x_moveforce},{y_spd},{x_pos},{x_page},{y_pos},{state},{facing},{moving},{x_abs},{interval},{screentimer},{timer_hi},{}\n",
            x_spd.abs()
        ));

        let name = outdir.join(format!("frame_{frame:05}.png"));
        smb_rs::pngout::write_png(&name, con.framebuffer(), 256, 240, scale);

        if frame % 120 == 0 {
            let active_lbl = if active_seq_idx < active_seq.len() {
                active_trick_name.as_str()
            } else {
                "ballistic"
            };
            println!(
                "f{frame:04} x={cur_x:4} spd={x_spd:2} mf={x_moveforce:3} y={y_pos:3} st={state} fr={interval:2} btns={:<7} mode={active_lbl}",
                format_buttons(buttons)
            );
        }

        if op == 0x02 || flagpole_touched_frame.is_some() {
            victory_after_frames += 1;
            // Capturar ~220 frames despues de tocar la bandera para que el video muestre la secuencia completa
            if victory_after_frames >= 220 {
                eprintln!("Secuencia de victoria completada en frame {frame}");
                break;
            }
        }
    }

    std::fs::write(outdir.join("telemetry.csv"), csv).expect("Error escribiendo telemetry.csv");
    std::fs::write(outdir.join("replay_script.txt"), script_events.join(",")).expect("Error escribiendo replay_script.txt");
    std::fs::write(outdir.join("tricks_audit.log"), audit_log).expect("Error escribiendo tricks_audit.log");

    println!(
        "done -> {} (telemetry.csv + replay_script.txt + tricks_audit.log generados)",
        outdir.display()
    );
}