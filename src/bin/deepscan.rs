//! deepscan — escáner profundo de Super Mario Bros. 1-1.
//!
//! Pipeline:
//!   1. `map`    : recorre 1-1 con el piloto base y cartografía todos los
//!                 metatiles visibles (Block Buffer $0500), enemigos y bandera.
//!   2. `plan`   : descompone el mapa en obstáculos y calcula un plan de saltos
//!                 con el modelo de física de la ROM (`route::physics`).
//!   3. `global` : búsqueda en haz con rollouts reales del emulador
//!                 (`route::beam`) para encontrar una ruta rápida y verificada.
//!   4. `all`    : las tres fases + reporte.
//!
//! Uso:
//!   deepscan <rom> <outdir> [--mode map|plan|global|all|selftest]
//!            [--frames N] [--beam N] [--stride N] [--goal-x N]
//!            [--max-rollout N] [--time-weight F] [--quiet]

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::path::{Path, PathBuf};

use smb_rs::console::{Console, BUTTON_A, BUTTON_B, BUTTON_LEFT, BUTTON_RIGHT};
use smb_rs::route::hash::sha256_hex;
use smb_rs::route::{
    beam_search, boot_to_level, inputs_to_script, plan_route, plan_to_json, BeamConfig,
    BeamResult, LevelMap, ManeuverKind, Pilot, RoutePlan,
};
use smb_rs::route::tricks::{
    analyze_tricks, probe_low_gravity, probe_start, read_trick_state, ProbeResult, TrickReport,
};
use smb_rs::Cartridge;

struct Args {
    rom: PathBuf,
    outdir: PathBuf,
    mode: String,
    frames: u32,
    beam: usize,
    stride: u32,
    goal_x: i32,
    max_rollout: u32,
    time_weight: f64,
    quiet: bool,
    script: Option<PathBuf>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            rom: PathBuf::new(),
            outdir: PathBuf::new(),
            mode: "all".to_string(),
            frames: 4000,
            beam: 16,
            stride: 6,
            goal_x: 3160,
            max_rollout: 60,
            time_weight: 2.5,
            quiet: false,
            script: None,
        }
    }
}

fn usage() {
    eprintln!(
        "Uso: deepscan <rom> <outdir> [--mode map|plan|global|probe|all|selftest]\n\
         \x20     [--frames N] [--beam N] [--stride N] [--goal-x N]\n\
         \x20     [--max-rollout N] [--time-weight F] [--script FILE] [--quiet]\n\n\
         Ejemplos:\n\
         \x20 deepscan rom.nes out_deep --mode map\n\
         \x20 deepscan rom.nes out_deep --mode global --beam 24 --stride 6\n\
         \x20 deepscan rom.nes out_deep --mode all --beam 16 --frames 4000"
    );
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().collect();
    if raw.len() < 3 {
        usage();
        std::process::exit(2);
    }
    let mut a = Args::default();
    a.rom = PathBuf::from(&raw[1]);
    a.outdir = PathBuf::from(&raw[2]);
    let mut i = 3;
    while i < raw.len() {
        match raw[i].as_str() {
            "--mode" => {
                i += 1;
                if i < raw.len() {
                    a.mode = raw[i].clone();
                }
            }
            "--frames" => {
                i += 1;
                a.frames = raw.get(i).and_then(|s| s.parse().ok()).unwrap_or(a.frames);
            }
            "--beam" => {
                i += 1;
                a.beam = raw.get(i).and_then(|s| s.parse().ok()).unwrap_or(a.beam);
            }
            "--stride" => {
                i += 1;
                a.stride = raw.get(i).and_then(|s| s.parse().ok()).unwrap_or(a.stride);
            }
            "--goal-x" => {
                i += 1;
                a.goal_x = raw.get(i).and_then(|s| s.parse().ok()).unwrap_or(a.goal_x);
            }
            "--max-rollout" => {
                i += 1;
                a.max_rollout = raw
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(a.max_rollout);
            }
            "--time-weight" => {
                i += 1;
                a.time_weight = raw
                    .get(i)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(a.time_weight);
            }
            "--script" => {
                i += 1;
                a.script = raw.get(i).map(PathBuf::from);
            }
            "--quiet" => a.quiet = true,
            _ => {}
        }
        i += 1;
    }
    a
}

fn main() {
    let args = parse_args();
    std::fs::create_dir_all(&args.outdir).expect("no se pudo crear outdir");
    let rom = std::fs::read(&args.rom).expect("no se pudo leer la ROM");
    let sha = sha256_hex(&rom);
    println!("ROM {} | sha256 {}", args.rom.display(), sha);
    println!(
        "mode={} frames={} beam={} stride={} goal_x={}",
        args.mode, args.frames, args.beam, args.stride, args.goal_x
    );

    match args.mode.as_str() {
        "selftest" => {
            let ok = selftest(&rom, &args.outdir);
            if !ok {
                std::process::exit(1);
            }
        }
        "probe" => {
            run_trick_probe(&rom, &args.outdir);
        }
        "wall" => {
            run_wall_probe(&rom, &args.outdir);
        }
        "fpg" => {
            run_fpg_probe(&rom, &args.outdir);
        }
        "replay" => {
            let path = args
                .script
                .clone()
                .unwrap_or_else(|| args.outdir.join("route_script.txt"));
            replay_route(&rom, &sha, &args, &path);
        }
        "map" => {
            let (map, stats) = run_pilot_map(&rom, &sha, &args);
            write_map(&map, &args.outdir);
            println!(
                "map: {} cols, {} enemies, {} obstacles | piloto: {} frames, x={}, bandera={}",
                map.cols,
                map.enemies.len(),
                map.obstacles().len(),
                stats.frames,
                stats.x,
                stats.reached_flag
            );
        }
        "plan" => {
            let (map, stats) = run_pilot_map(&rom, &sha, &args);
            write_map(&map, &args.outdir);
            let plan = plan_route(&map);
            write_plan(&map, &plan, &args.outdir);
            let exec = execute_plan(&rom, &plan, &args.outdir);
            write_report(
                &args,
                &sha,
                &map,
                &stats,
                Some(&plan),
                Some(&exec),
                None,
            );
            println!(
                "plan: {} maniobras ({} saltos), estimado {:.0}f | ejecución: {}f x={} bandera={}",
                plan.maneuvers.len(),
                plan.jumps(),
                plan.estimated_frames,
                exec.frames,
                exec.x,
                exec.reached_flag
            );
        }
        "global" => {
            let g = run_global(&rom, &sha, &args);
            write_global(&g, &args.outdir);
            write_report(&args, &sha, &g.map, &g.pilot, None, None, Some(&g));
            println!(
                "global: {}f x={} bandera={} rollouts={} nodos={} | vx_max={} f{} | flagpole={:?}",
                g.result.frames,
                g.result.x,
                g.result.reached_goal,
                g.result.rollouts,
                g.result.nodes,
                g.tricks.max_vx,
                g.tricks.max_vx_frame,
                g.tricks.flagpole_frame
            );
        }
        "all" => {
            let (map, stats) = run_pilot_map(&rom, &sha, &args);
            write_map(&map, &args.outdir);
            let plan = plan_route(&map);
            write_plan(&map, &plan, &args.outdir);
            let exec = execute_plan(&rom, &plan, &args.outdir);
            let g = run_global(&rom, &sha, &args);
            write_global(&g, &args.outdir);
            write_report(
                &args,
                &sha,
                &map,
                &stats,
                Some(&plan),
                Some(&exec),
                Some(&g),
            );
            println!(
                "all: mapa {} cols | plan {} saltos | global {}f x={} bandera={}",
                map.cols,
                plan.jumps(),
                g.result.frames,
                g.result.x,
                g.result.reached_goal
            );
        }
        other => {
            eprintln!("modo desconocido: {other}");
            usage();
            std::process::exit(2);
        }
    }
}

// --------------------------------------------------------------------------- //
// Fase 1: piloto base + cartografía
// --------------------------------------------------------------------------- //

struct PilotStats {
    frames: u32,
    x: i32,
    reached_flag: bool,
    jumps: u32,
    unstucks: u32,
}

fn run_pilot_map(rom: &[u8], sha: &str, args: &Args) -> (LevelMap, PilotStats) {
    let cart = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut con = Console::new(cart);
    let Some((_snap, _boot)) = boot_to_level(&mut con, 2400) else {
        panic!("no se pudo llegar al nivel 1-1");
    };
    let mut pilot = Pilot::new();
    let mut map = LevelMap::new(sha);
    let mut csv = String::from("frame,buttons,x,y,vx,state,camera,op\n");
    let mut frames = 0u32;
    let mut x = 0i32;
    let mut reached = false;

    for f in 0..args.frames {
        let b = pilot.step(&con.bus.ram);
        con.set_buttons(0, b);
        if !con.step_frame() {
            break;
        }
        con.bus.apu.samples.clear();
        map.ingest(&con.bus.ram, f as u64);

        let r = &con.bus.ram;
        x = (r[0x006D] as i32) * 256 + (r[0x0086] as i32);
        let y = r[0x00CE] as i32;
        let vx = (r[0x0057] as i8) as i32;
        let state = r[0x001D];
        let cam = (r[0x071A] as i32) * 256 + (r[0x071C] as i32);
        let op = r[0x0770];
        csv.push_str(&format!("{f},{b},{x},{y},{vx},{state},{cam},{op}\n"));
        frames = f + 1;

        if op == 0x02 || state == 3 || r[0x000E] == 4 || r[0x000E] == 5 {
            reached = true;
            // Capturar la zona de la bandera.
            for k in 0..90 {
                con.set_buttons(0, 0);
                if !con.step_frame() {
                    break;
                }
                con.bus.apu.samples.clear();
                map.ingest(&con.bus.ram, (f + k) as u64);
            }
            break;
        }
        if x >= 3200 {
            break;
        }
    }

    std::fs::write(args.outdir.join("map_pilot_telemetry.csv"), csv)
        .expect("error escribiendo map_pilot_telemetry.csv");
    (
        map,
        PilotStats {
            frames,
            x,
            reached_flag: reached,
            jumps: pilot.jumps,
            unstucks: pilot.unstucks,
        },
    )
}

fn write_map(map: &LevelMap, outdir: &Path) {
    std::fs::write(outdir.join("level_map.json"), map.to_json()).expect("error level_map.json");
    std::fs::write(outdir.join("level_map.svg"), map.to_svg(None)).expect("error level_map.svg");
    std::fs::write(outdir.join("level_map.txt"), map.to_ascii()).expect("error level_map.txt");
}

fn write_plan(map: &LevelMap, plan: &RoutePlan, outdir: &Path) {
    std::fs::write(outdir.join("route_plan.json"), plan_to_json(plan))
        .expect("error route_plan.json");
    std::fs::write(
        outdir.join("route_plan.svg"),
        map.to_svg(Some(&plan.markers())),
    )
    .expect("error route_plan.svg");
    let mut md = String::from("# Plan heurístico 1-1\n\n");
    md.push_str(&format!(
        "- Obstáculos: {}\n- Saltos: {}\n- Maniobras: {}\n- Estimado: {:.0} frames ({:.1}s)\n\n",
        map.obstacles().len(),
        plan.jumps(),
        plan.maneuvers.len(),
        plan.estimated_frames,
        plan.estimated_frames / 60.0988
    ));
    md.push_str("| # | tipo | desde | hasta | despegue | aterrizaje | hold | airtime | nota |\n");
    md.push_str("|---|---|---|---|---|---|---|---|---|\n");
    for (i, m) in plan.maneuvers.iter().enumerate() {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            i + 1,
            if m.kind == ManeuverKind::Jump { "salto" } else { "correr" },
            m.from_col,
            m.to_col,
            m.takeoff_col,
            m.land_col,
            m.hold,
            m.airtime,
            m.note
        ));
    }
    md.push_str("\n## Notas\n\n");
    for n in &plan.notes {
        md.push_str(&format!("- {n}\n"));
    }
    std::fs::write(outdir.join("route_plan.md"), md).expect("error route_plan.md");
}

// --------------------------------------------------------------------------- //
// Ejecución del plan heurístico (feedback por X)
// --------------------------------------------------------------------------- //

struct PlanExecStats {
    frames: u32,
    x: i32,
    reached_flag: bool,
    notes: Vec<String>,
}

/// Estado del ejecutor cerrado del plan: además de correr/saltar, detecta
/// atascos, reintenta saltos y cae al piloto heurístico si el plan no encaja.
struct PlanExec {
    con: Console,
    csv: String,
    frame: u32,
    reached: bool,
    died: bool,
    start_lives: u8,
    budget: u32,
}

impl PlanExec {
    fn x(&self) -> i32 {
        let r = &self.con.bus.ram;
        (r[0x006D] as i32) * 256 + (r[0x0086] as i32)
    }

    fn state(&self) -> u8 {
        self.con.bus.ram[0x001D]
    }

    fn step(&mut self, b: u8, phase: &str) -> bool {
        if self.frame >= self.budget {
            self.died = true;
            return false;
        }
        self.con.set_buttons(0, b);
        if !self.con.step_frame() {
            self.died = true;
            return false;
        }
        self.con.bus.apu.samples.clear();
        self.frame += 1;
        let r = &self.con.bus.ram;
        let x = (r[0x006D] as i32) * 256 + (r[0x0086] as i32);
        let y = r[0x00CE] as i32;
        let vx = r[0x0057];
        let st = r[0x001D];
        let lives = r[0x075A];
        let op = r[0x0770];
        self.csv.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            self.frame, b, x, y, vx, st, lives, phase
        ));
        if lives < self.start_lives {
            self.died = true;
            return false;
        }
        if st == 3 || op == 0x02 || r[0x000E] == 4 || r[0x000E] == 5 {
            self.reached = true;
            return false;
        }
        // `$CE` es solo el byte bajo de la Y: un salto alto puede dar 255
        // estando por encima de la pantalla. La muerte real es HighPos == 1
        // (Mario ha caído por debajo del nivel).
        if r[0x000B] == 1 {
            self.died = true;
            return false;
        }
        true
    }
}

/// ¿Hay un enemigo a distancia de salto delante de Mario? Usa los mismos
/// sensores de Block Buffer que el piloto base.
fn enemy_ahead(ram: &[u8; 2048], x: i32, vx: i32) -> bool {
    let ypix = ram[0x03B8] as i32;
    let power = ram[0x0756];
    let feet = (ypix + if power == 0 { 32 } else { 48 }).clamp(32, 239);
    let scan = smb_rs::route::scan_environment(ram, x, feet);
    let trigger = (10 + (vx * 3) / 4).clamp(20, 38);
    scan.enemy_dist >= 0 && scan.enemy_dist <= trigger
}

/// Corre hacia `target` con detección de atasco y esquiva de enemigos.
/// `true` si llegó al objetivo.
fn plan_run_to(
    ex: &mut PlanExec,
    target: i32,
    limit: u32,
    phase: &str,
    notes: &mut Vec<String>,
) -> bool {
    let mut guard = 0;
    let mut last_x = ex.x();
    let mut stuck = 0u32;
    while ex.x() < target && guard < limit {
        if ex.state() == 0 {
            let vx = (ex.con.bus.ram[0x0057] as i8) as i32;
            if enemy_ahead(&ex.con.bus.ram, ex.x(), vx) {
                if !plan_jump_once(ex, 6, "enemy-hop", notes) {
                    return false;
                }
                guard += 6;
                continue;
            }
        }
        if !ex.step(BUTTON_RIGHT | BUTTON_B, phase) {
            return false;
        }
        guard += 1;
        if ex.x() <= last_x {
            stuck += 1;
        } else {
            stuck = 0;
            last_x = ex.x();
        }
        if stuck >= 24 && ex.state() == 0 {
            break;
        }
    }
    ex.x() >= target
}

/// Salta con `hold` frames de A y espera el aterrizaje. `true` si aterrizó.
fn plan_jump_once(
    ex: &mut PlanExec,
    hold: u8,
    phase: &str,
    notes: &mut Vec<String>,
) -> bool {
    // Flanco limpio de A (SMB1 exige A=0 el frame anterior).
    if !ex.step(BUTTON_RIGHT | BUTTON_B, phase) {
        return false;
    }
    let mut air = 0u32;
    loop {
        let b = if air < hold as u32 {
            BUTTON_RIGHT | BUTTON_B | BUTTON_A
        } else {
            BUTTON_RIGHT | BUTTON_B
        };
        if !ex.step(b, phase) {
            return false;
        }
        air += 1;
        if air > 4 && ex.state() == 0 {
            return true;
        }
        if air > 120 {
            notes.push(format!("salto sin aterrizar en 120f (x={})", ex.x()));
            return false;
        }
    }
}

fn execute_plan(rom: &[u8], plan: &RoutePlan, outdir: &Path) -> PlanExecStats {
    let cart = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut con = Console::new(cart);
    let Some((_snap, _boot)) = boot_to_level(&mut con, 2400) else {
        return PlanExecStats {
            frames: 0,
            x: 0,
            reached_flag: false,
            notes: vec!["no se pudo bootear el nivel".into()],
        };
    };

    let start_lives = con.bus.ram[0x075A];
    let budget = (plan.estimated_frames.max(1200.0) as u32)
        .saturating_mul(3)
        .saturating_add(900);
    let mut ex = PlanExec {
        con,
        csv: String::from("frame,buttons,x,y,vx,state,lives,phase\n"),
        frame: 0,
        reached: false,
        died: false,
        start_lives,
        budget,
    };
    let mut notes = Vec::new();
    let mut pilot = Pilot::new();

    'outer: for m in &plan.maneuvers {
        match m.kind {
            ManeuverKind::Run => {
                let target = m.to_col * 16;
                if !plan_run_to(&mut ex, target, 600, "run", &mut notes) {
                    if ex.reached || ex.died {
                        break 'outer;
                    }
                    // Atasco: salto completo de desatasco y reintento.
                    for _ in 0..2 {
                        if !plan_jump_once(&mut ex, 26, "run-unstick", &mut notes) {
                            break;
                        }
                        if ex.x() >= target {
                            break;
                        }
                        for _ in 0..10 {
                            if !ex.step(BUTTON_LEFT, "back") {
                                break;
                            }
                        }
                    }
                    if ex.x() < target && !ex.reached && !ex.died {
                        notes.push(format!(
                            "run atascado en x={} (objetivo {}), fallback al piloto",
                            ex.x(),
                            target
                        ));
                        for _ in 0..150 {
                            let b = pilot.step(&ex.con.bus.ram);
                            if !ex.step(b, "pilot") {
                                break;
                            }
                            if ex.x() >= target {
                                break;
                            }
                        }
                    }
                }
            }
            ManeuverKind::Jump => {
                let target = m.takeoff_col * 16;
                if !plan_run_to(&mut ex, target, 600, "approach", &mut notes) {
                    if ex.reached || ex.died {
                        break 'outer;
                    }
                    // Sin carrerilla: se intenta el salto desde la posición actual.
                }
                let mut ok = false;
                for attempt in 0..3 {
                    let hold = if attempt == 0 { m.hold } else { 26 };
                    if !plan_jump_once(&mut ex, hold, "jump", &mut notes) {
                        break;
                    }
                    if ex.x() >= m.land_col * 16 {
                        ok = true;
                        break;
                    }
                    // Reintento: retroceder para coger carrerilla.
                    for _ in 0..12 {
                        if !ex.step(BUTTON_LEFT, "retry-back") {
                            break;
                        }
                    }
                }
                if !ok && ex.x() < m.land_col * 16 && !ex.reached && !ex.died {
                    notes.push(format!(
                        "salto {} sin superar col {} (x={}), fallback al piloto",
                        m.takeoff_col,
                        m.land_col,
                        ex.x()
                    ));
                    for _ in 0..180 {
                        let b = pilot.step(&ex.con.bus.ram);
                        if !ex.step(b, "pilot") {
                            break;
                        }
                        if ex.x() >= m.land_col * 16 {
                            break;
                        }
                    }
                }
            }
        }
        if ex.reached || ex.died {
            break 'outer;
        }
    }

    if ex.died && !ex.reached {
        notes.push(format!(
            "ejecución abortada en frame {} x={} (muerte o presupuesto)",
            ex.frame,
            ex.x()
        ));
    }

    let final_x = ex.x();
    std::fs::write(outdir.join("plan_execution.csv"), ex.csv)
        .expect("error plan_execution.csv");
    PlanExecStats {
        frames: ex.frame,
        x: final_x,
        reached_flag: ex.reached,
        notes,
    }
}

// --------------------------------------------------------------------------- //
// Fase 3: búsqueda global con rollouts
// --------------------------------------------------------------------------- //

struct GlobalStats {
    map: LevelMap,
    pilot: PilotStats,
    result: BeamResult,
    cfg: BeamConfig,
    tricks: TrickReport,
}

fn run_global(rom: &[u8], sha: &str, args: &Args) -> GlobalStats {
    let cfg = BeamConfig {
        beam_width: args.beam,
        max_frames: args.frames,
        stride: args.stride,
        goal_x: args.goal_x,
        max_rollout: args.max_rollout,
        time_weight: args.time_weight,
        verbose: !args.quiet,
        goal_mode: smb_rs::route::GoalMode::Flagpole,
    };

    let cart = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut con = Console::new(cart);
    let Some((snap, boot_inputs)) = boot_to_level(&mut con, 2400) else {
        panic!("no se pudo llegar al nivel 1-1");
    };
    let mut result = beam_search(&mut con, &snap, &cfg);
    result.boot_frames = boot_inputs.len() as u32;
    let mut full = boot_inputs;
    full.extend_from_slice(&result.level_inputs);
    result.inputs = full;

    // Replay determinista para verificar y cartografiar la ruta encontrada.
    let cart2 = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut con2 = Console::new(cart2);
    let mut map = LevelMap::new(sha);
    let mut pilot = PilotStats {
        frames: 0,
        x: 0,
        reached_flag: false,
        jumps: 0,
        unstucks: 0,
    };
    let mut csv = String::from("frame,buttons,x,y,vx,state,camera,op\n");
    let mut trick_samples: Vec<(u32, u8, smb_rs::route::TrickState)> = Vec::new();
    for (f, &b) in result.inputs.iter().enumerate() {
        con2.set_buttons(0, b);
        if !con2.step_frame() {
            break;
        }
        con2.bus.apu.samples.clear();
        let r = &con2.bus.ram;
        let x = (r[0x006D] as i32) * 256 + (r[0x0086] as i32);
        let y = r[0x00CE] as i32;
        let vx = (r[0x0057] as i8) as i32;
        let st = r[0x001D];
        let cam = (r[0x071A] as i32) * 256 + (r[0x071C] as i32);
        let op = r[0x0770];
        if f >= result.boot_frames as usize {
            map.ingest(&con2.bus.ram, f as u64);
            trick_samples.push((
                (f as u32).saturating_sub(result.boot_frames),
                b,
                read_trick_state(&con2),
            ));
        }
        csv.push_str(&format!("{f},{b},{x},{y},{vx},{st},{cam},{op}\n"));
        pilot.x = x;
        pilot.frames = (f as u32).saturating_sub(result.boot_frames);
        if op == 0x02 || st == 3 || r[0x000E] == 4 || r[0x000E] == 5 {
            pilot.reached_flag = true;
            break;
        }
    }
    std::fs::write(args.outdir.join("route_telemetry.csv"), csv)
        .expect("error route_telemetry.csv");

    let tricks = analyze_tricks(&trick_samples);

    GlobalStats {
        map,
        pilot,
        result,
        cfg,
        tricks,
    }
}

fn write_global(g: &GlobalStats, outdir: &Path) {
    std::fs::write(outdir.join("level_map.json"), g.map.to_json())
        .expect("error level_map.json");
    std::fs::write(outdir.join("level_map.svg"), g.map.to_svg(None))
        .expect("error level_map.svg");
    std::fs::write(outdir.join("level_map.txt"), g.map.to_ascii())
        .expect("error level_map.txt");

    std::fs::write(outdir.join("route_script.txt"), inputs_to_script(&g.result.inputs))
        .expect("error route_script.txt");

    let mut trace = String::from("step,frame,x,y,vx,score,beam_len,goal_frame\n");
    for t in &g.result.trace {
        trace.push_str(&format!(
            "{},{},{},{},{},{:.3},{},{}\n",
            t.step,
            t.frame,
            t.x,
            t.y,
            t.vx,
            t.score,
            t.beam_len,
            t.goal_frame.map(|f| f as i64).unwrap_or(-1)
        ));
    }
    std::fs::write(outdir.join("route_trace.csv"), trace).expect("error route_trace.csv");

    let mut json = String::new();
    json.push_str("{\n");
    json.push_str(&format!("  \"rom_sha256\": \"{}\",\n", g.map.rom_sha256));
    json.push_str("  \"mode\": \"global_beam\",\n");
    json.push_str(&format!(
        "  \"beam_width\": {},\n  \"stride\": {},\n  \"max_frames\": {},\n  \"goal_x\": {},\n  \"time_weight\": {},\n",
        g.cfg.beam_width, g.cfg.stride, g.cfg.max_frames, g.cfg.goal_x, g.cfg.time_weight
    ));
    json.push_str(&format!(
        "  \"boot_frames\": {},\n  \"level_frames\": {},\n  \"total_frames\": {},\n  \"x\": {},\n  \"y\": {},\n  \"reached_goal\": {},\n  \"rollouts\": {},\n  \"nodes\": {},\n",
        g.result.boot_frames,
        g.result.frames,
        g.result.frames + g.result.boot_frames,
        g.result.x,
        g.result.y,
        g.result.reached_goal,
        g.result.rollouts,
        g.result.nodes
    ));
    json.push_str("  \"files\": {\n");
    json.push_str("    \"script\": \"route_script.txt\",\n");
    json.push_str("    \"trace\": \"route_trace.csv\",\n");
    json.push_str("    \"telemetry\": \"route_telemetry.csv\",\n");
    json.push_str("    \"map\": \"level_map.json\",\n");
    json.push_str("    \"map_svg\": \"level_map.svg\"\n");
    json.push_str("  }\n}\n");
    std::fs::write(outdir.join("route.json"), json).expect("error route.json");
    std::fs::write(outdir.join("trick_report.md"), g.tricks.to_markdown())
        .expect("error trick_report.md");
}

// --------------------------------------------------------------------------- //
// Reporte
// --------------------------------------------------------------------------- //

fn write_report(
    args: &Args,
    sha: &str,
    map: &LevelMap,
    pilot: &PilotStats,
    plan: Option<&RoutePlan>,
    exec: Option<&PlanExecStats>,
    global: Option<&GlobalStats>,
) {
    let mut md = String::new();
    md.push_str("# deepscan — reporte de ruta 1-1\n\n");
    md.push_str(&format!(
        "- ROM: `{}`\n- SHA-256: `{}`\n- Modo: `{}`\n- Frames máx: {}\n\n",
        args.rom.display(),
        sha,
        args.mode,
        args.frames
    ));

    md.push_str("## Mapa\n\n");
    md.push_str(&format!(
        "- Columnas cartografiadas: {}\n- Filas: 13 (y=32..239)\n- Suelo base: fila {}\n- Enemigos vistos: {}\n- Obstáculos: {}\n- Bandera: col {:?}\n\n",
        map.cols,
        map.baseline_ground_row(),
        map.enemies.len(),
        map.obstacles().len(),
        map.flag_col
    ));
    let mut pits = 0;
    let mut walls = 0;
    for o in map.obstacles() {
        match o.kind {
            smb_rs::route::ObstacleKind::Pit => pits += 1,
            smb_rs::route::ObstacleKind::Wall => walls += 1,
        }
    }
    md.push_str(&format!("- Pozos: {pits}\n- Muros/tuberías/escalones: {walls}\n\n"));

    md.push_str("## Recorrido (piloto o replay)\n\n");
    md.push_str(
        "En `map`/`plan` son estadísticas del piloto base; en `global` son del \
         replay determinista de la ruta encontrada por el beam.\n\n",
    );
    md.push_str(&format!(
        "- Frames: {}\n- X final: {}\n- Bandera: {}\n",
        pilot.frames, pilot.x, pilot.reached_flag
    ));
    if pilot.jumps > 0 || pilot.unstucks > 0 {
        md.push_str(&format!(
            "- Saltos: {}\n- Desatascos: {}\n",
            pilot.jumps, pilot.unstucks
        ));
    }
    if let Some(g) = global {
        md.push_str("\n## Trucos detectados\n\n");
        md.push_str(&format!(
            "- Velocidad máxima: {} (frame {})\n- Frames a velocidad tope: {}\n- Fricción inversa: {} frames\n- Moonwalk (L+R): {} frames\n- Aire: {} frames\n- Saltos: {}\n- Primer salto: frame {:?}, airtime {:?}\n- Flagpole: frame {:?}, Y={}, score={}, ventana={}\n",
            g.tricks.max_vx,
            g.tricks.max_vx_frame,
            g.tricks.top_speed_frames,
            g.tricks.reverse_friction_frames,
            g.tricks.moonwalk_frames,
            g.tricks.airborne_frames,
            g.tricks.jumps,
            g.tricks.first_jump_frame,
            g.tricks.first_jump_airtime,
            g.tricks.flagpole_frame,
            g.tricks.flagpole_y,
            g.tricks.flagpole_score,
            if g.tricks.flagpole_window { "sí" } else { "no" }
        ));
    }
    md.push('\n');

    if let Some(p) = plan {
        md.push_str("## Plan heurístico\n\n");
        md.push_str(&format!(
            "- Saltos: {}\n- Maniobras: {}\n- Estimado: {:.0} frames ({:.1}s)\n\n",
            p.jumps(),
            p.maneuvers.len(),
            p.estimated_frames,
            p.estimated_frames / 60.0988
        ));
        for n in &p.notes {
            md.push_str(&format!("- {n}\n"));
        }
        md.push('\n');
    }

    if let Some(e) = exec {
        md.push_str("## Ejecución del plan\n\n");
        md.push_str(&format!(
            "- Frames: {}\n- X final: {}\n- Bandera: {}\n",
            e.frames, e.x, e.reached_flag
        ));
        for n in &e.notes {
            md.push_str(&format!("- {n}\n"));
        }
        md.push('\n');
    }

    if let Some(g) = global {
        md.push_str("## Búsqueda global (beam)\n\n");
        md.push_str(&format!(
            "- Beam: {} | stride: {} | max_rollout: {} | time_weight: {}\n- Frames de nivel: {} ({:.2}s)\n- Frames totales (con boot): {}\n- X final: {} | Y: {}\n- Bandera: {}\n- Rollouts: {}\n- Nodos conservados: {}\n\n",
            g.cfg.beam_width,
            g.cfg.stride,
            g.cfg.max_rollout,
            g.cfg.time_weight,
            g.result.frames,
            g.result.frames as f64 / 60.0988,
            g.result.frames + g.result.boot_frames,
            g.result.x,
            g.result.y,
            g.result.reached_goal,
            g.result.rollouts,
            g.result.nodes
        ));
    }

    md.push_str("## Reproducir\n\n");
    md.push_str("```powershell\n");
    md.push_str(&format!(
        ".\\target\\release\\nesdump.exe \"{}\" out_replay --frames {} --script (Get-Content {}\\route_script.txt -Raw)\n",
        args.rom.display(),
        (global.map(|g| g.result.frames + g.result.boot_frames).unwrap_or(args.frames)),
        args.outdir.display()
    ));
    md.push_str("```\n\n");
    md.push_str("## Limitaciones\n\n");
    md.push_str("- El mapa asume niveles sin scroll vertical (1-1). Para 1-2/4-2/8-4 hay que compensar la cámara Y.\n");
    md.push_str("- El plan heurístico usa velocidad de carrera constante; la ejecución real acelera desde 0.\n");
    md.push_str("- La búsqueda global es costosa: `--beam` y `--frames` controlan el presupuesto. Un beam mayor explora más pero tarda más.\n");
    md.push_str("- `route_script.txt` incluye los frames de boot (START) y se puede pasar a `nesdump --script`.\n");
    std::fs::write(args.outdir.join("deepscan_report.md"), md).expect("error deepscan_report.md");
}

// --------------------------------------------------------------------------- //
// Replay de una ruta guardada (route_script.txt)
// --------------------------------------------------------------------------- //

fn parse_route_script(path: &Path) -> Vec<u8> {
    let text = std::fs::read_to_string(path).expect("no se pudo leer el script");
    let mut events = Vec::new();
    for ev in text.trim().split(',') {
        let mut it = ev.split(':');
        let Some(f) = it.next().and_then(|s| s.trim().parse::<usize>().ok()) else {
            continue;
        };
        let Some(name) = it.next() else { continue };
        let Some(val) = it.next().and_then(|s| s.trim().parse::<u8>().ok()) else {
            continue;
        };
        let mask = match name {
            "a" => BUTTON_A,
            "b" => BUTTON_B,
            "select" => smb_rs::console::BUTTON_SELECT,
            "start" => smb_rs::console::BUTTON_START,
            "up" => smb_rs::console::BUTTON_UP,
            "down" => smb_rs::console::BUTTON_DOWN,
            "left" => BUTTON_LEFT,
            "right" => BUTTON_RIGHT,
            _ => continue,
        };
        events.push((f, mask, val != 0));
    }
    let max = events.iter().map(|e| e.0).max().unwrap_or(0);
    // Los eventos son cambios de estado: hay que propagar el valor entre frames.
    let mut inputs = vec![0u8; max + 1];
    let mut cur = 0u8;
    let mut idx = 0;
    for f in 0..=max {
        while idx < events.len() && events[idx].0 == f {
            let (_, mask, down) = events[idx];
            if down {
                cur |= mask;
            } else {
                cur &= !mask;
            }
            idx += 1;
        }
        inputs[f] = cur;
    }
    // El script solo guarda cambios: hay que alargar la cola con los últimos
    // botones para llegar al mástil (los eventos acaban en el último cambio).
    inputs.extend(std::iter::repeat(cur).take(120));
    inputs
}

fn replay_route(rom: &[u8], sha: &str, args: &Args, script: &Path) {
    let inputs = parse_route_script(script);
    let cart = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut boot_con = Console::new(cart);
    let Some((_, boot)) = boot_to_level(&mut boot_con, 2400) else {
        eprintln!("replay: no se pudo detectar el inicio del nivel");
        return;
    };
    let boot_frames = boot.len() as u32;

    let cart = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut con = Console::new(cart);
    let mut map = LevelMap::new(sha);
    let mut csv = String::from("frame,buttons,x,y,vx,state,camera,op\n");
    let mut trick_samples: Vec<(u32, u8, smb_rs::route::TrickState)> = Vec::new();
    let mut final_x = 0;
    let mut final_frame = 0u32;
    let mut reached = false;
    let mut died = false;

    for (f, &b) in inputs.iter().enumerate() {
        con.set_buttons(0, b);
        if !con.step_frame() {
            died = true;
            break;
        }
        con.bus.apu.samples.clear();
        let r = &con.bus.ram;
        let x = (r[0x006D] as i32) * 256 + (r[0x0086] as i32);
        let y = r[0x00CE] as i32;
        let vx = (r[0x0057] as i8) as i32;
        let st = r[0x001D];
        let cam = (r[0x071A] as i32) * 256 + (r[0x071C] as i32);
        let op = r[0x0770];
        csv.push_str(&format!("{f},{b},{x},{y},{vx},{st},{cam},{op}\n"));
        if f as u32 >= boot_frames {
            map.ingest(&con.bus.ram, f as u64);
            trick_samples.push((
                (f as u32).saturating_sub(boot_frames),
                b,
                read_trick_state(&con),
            ));
        }
        final_x = x;
        final_frame = f as u32;
        if r[0x000B] == 1 {
            died = true;
            break;
        }
        if st == 3 || r[0x000E] == 4 || r[0x000E] == 5 || r[0x070F] != 0 {
            reached = true;
            break;
        }
    }

    let level_frames = if reached {
        final_frame.saturating_sub(boot_frames) + 1
    } else {
        final_frame.saturating_sub(boot_frames)
    };
    let tricks = analyze_tricks(&trick_samples);

    std::fs::write(args.outdir.join("route_replay.csv"), csv)
        .expect("error route_replay.csv");
    std::fs::write(args.outdir.join("trick_report.md"), tricks.to_markdown())
        .expect("error trick_report.md");
    std::fs::write(args.outdir.join("level_map.json"), map.to_json())
        .expect("error level_map.json");
    std::fs::write(args.outdir.join("level_map.svg"), map.to_svg(None))
        .expect("error level_map.svg");
    std::fs::write(args.outdir.join("level_map.txt"), map.to_ascii())
        .expect("error level_map.txt");

    println!(
        "replay: {}f nivel (total {}f, boot {}f) x={} bandera={} muerto={} | vx_max={} f{} | flagpole={:?} Y={} ventana={}",
        level_frames,
        final_frame + 1,
        boot_frames,
        final_x,
        reached,
        died,
        tricks.max_vx,
        tricks.max_vx_frame,
        tricks.flagpole_frame,
        tricks.flagpole_y,
        tricks.flagpole_window
    );
}

fn run_wall_probe(rom: &[u8], outdir: &Path) {
    let cart = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut con = Console::new(cart);
    let Some((snap0, _)) = boot_to_level(&mut con, 2400) else {
        eprintln!("wall: sin boot");
        return;
    };
    let inputs = parse_route_script(&outdir.join("route_script.txt"));
    con.restore(&snap0);
    let mut st = None;
    for (f, &b) in inputs.iter().enumerate() {
        if f >= 1045 {
            break;
        }
        con.set_buttons(0, b);
        con.step_frame();
        con.bus.apu.samples.clear();
    }
    let r = &con.bus.ram;
    let x0 = (r[0x6D] as i32) * 256 + r[0x86] as i32;
    st = Some((con.snapshot(), x0));
    let (snap, x0) = st.unwrap();
    println!("wall: snapshot escalera x={x0}");
    let hits = smb_rs::route::tricks::probe_wall_entry(&mut con, &snap, x0);
    println!("wall: {} entradas/teleports en x≈2144", hits.len());
    for h in &hits {
        println!("  {h}");
    }
    let mut md = String::from("# wall entry probe (escalera x≈2144)\n\n");
    md.push_str(&format!(
        "Snapshot: x={x0}. Probadas andar/correr + salto 0..5 + tiron LEFT.\n\n## Resultados\n\n"
    ));
    if hits.is_empty() {
        md.push_str("Sin entradas ni teletransportes (0 casos).\n");
    }
    for h in &hits {
        md.push_str(&format!("- {h}\n"));
    }
    std::fs::write(outdir.join("wall_probe.md"), md).expect("wall_probe.md");
}

fn run_fpg_probe(rom: &[u8], outdir: &Path) {
    let cart = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut con = Console::new(cart);
    let Some((snap0, _)) = boot_to_level(&mut con, 2400) else {
        eprintln!("fpg: sin boot");
        return;
    };
    let inputs = parse_route_script(&outdir.join("route_script.txt"));
    con.restore(&snap0);
    for (f, &b) in inputs.iter().enumerate() {
        if f >= 1434 {
            break;
        }
        con.set_buttons(0, b);
        con.step_frame();
        con.bus.apu.samples.clear();
    }
    let snap = con.snapshot();
    println!("fpg: snapshot cumulo, probando subpixel+caida...");
    let (tried, found, best) =
        smb_rs::route::tricks::probe_fpg_drop(&mut con, &snap);
    println!("fpg: probados={tried} FPG={found} mejor_slide={best}f");
    let mut md = String::from("# FPG probe (caida dentro del asta)\n\n");
    md.push_str(&format!(
        "- Probados: {tried}\n- FPG (slide 0): {found}\n- Mejor slide visto: {best}f\n\n"
    ));
    std::fs::write(outdir.join("fpg_probe.md"), md).expect("fpg_probe.md");
}

// --------------------------------------------------------------------------- //
// Probe empírico de trucos
// --------------------------------------------------------------------------- //

fn probe_row(r: &ProbeResult) -> String {
    format!(
        "| `{}` | {} | {} | {} | {} | {} | {:?} | {} |",
        r.name,
        r.frames,
        r.x,
        r.vx,
        r.max_vx,
        r.peak_height,
        r.landed,
        if r.final_state.flagpole_window() { "sí" } else { "no" }
    )
}

fn run_trick_probe(rom: &[u8], outdir: &Path) {
    let cart = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut con = Console::new(cart);
    let Some((snap, boot)) = boot_to_level(&mut con, 2400) else {
        eprintln!("probe: no se pudo bootear el nivel");
        return;
    };
    let start = smb_rs::route::read_trick_state(&con);

    println!(
        "probe: inicio de nivel x={} y={} vx={} facing={} moving={} frame_rule={}",
        start.x, start.y, start.vx, start.facing, start.moving, start.frame_rule
    );

    let results = probe_start(&mut con, &snap);
    let exhaustive = smb_rs::route::tricks::probe_exhaustive_prefix(&mut con, &snap);
    println!("\nExhaustivo (2 frames + correr hasta 60): top 10");
    for r in exhaustive.iter().take(10) {
        println!(
            "  {:<20} x={:4} vx={:2} max_vx={}",
            r.name, r.x, r.vx, r.max_vx
        );
    }
    let mut md = String::from("# deepscan probe — trucos de arranque (1-1)\n\n");
    md.push_str(&format!(
        "Snapshot de inicio: x={} y={} vx={} (boot {} frames).\n\n",
        start.x, start.y, start.vx, boot.len()
    ));
    md.push_str(
        "Cada prueba ejecuta un prefijo exacto y luego mantiene `RIGHT+B` hasta el frame 90. \
         `altura` es la altura máxima sobre la Y inicial; `aterrizaje` es el frame del primer \
         contacto con el suelo tras el salto.\n\n",
    );
    md.push_str("| prueba | frames | x | vx | max_vx | altura | aterrizaje | flag |\n");
    md.push_str("|---|---:|---:|---:|---:|---:|---:|---|\n");
    for r in &results {
        md.push_str(&probe_row(r));
        md.push('\n');
    }

    md.push_str("\n## Low-gravity: primer salto vs salto tras correr\n\n");
    md.push_str("| prueba | frames | x | vx | max_vx | altura | aterrizaje |\n");
    md.push_str("|---|---:|---:|---:|---:|---:|---:|\n");
    for hold_a in [4u32, 8, 14, 26] {
        let (first, later) = probe_low_gravity(&mut con, &snap, hold_a, 20);
        for r in [&first, &later] {
            md.push_str(&format!(
                "| `{}` | {} | {} | {} | {} | {} | {:?} |\n",
                r.name, r.frames, r.x, r.vx, r.max_vx, r.peak_height, r.landed
            ));
        }
    }

    md.push_str("\n## Exhaustivo: 2 frames + correr (hasta 60f)\n\n");
    md.push_str("| prueba | frames | x | vx | max_vx | altura | aterrizaje | flag |\n");
    md.push_str("|---|---:|---:|---:|---:|---:|---:|---|\n");
    for r in exhaustive.iter().take(30) {
        md.push_str(&probe_row(r));
        md.push('\n');
    }

    std::fs::write(outdir.join("trick_probe.md"), md).expect("error trick_probe.md");

    println!("\nTop 12 por X a los 90 frames:");
    for r in results.iter().take(12) {
        println!(
            "  {:<28} x={:4} vx={:2} max_vx={:2} altura={:2} aterrizaje={:?}",
            r.name, r.x, r.vx, r.max_vx, r.peak_height, r.landed
        );
    }
    let best = &results[0];
    println!(
        "\nMejor arranque: {} x={} (baseline {}), ganancia {} px",
        best.name,
        best.x,
        results
            .iter()
            .find(|r| r.name == "baseline_run")
            .map(|r| r.x)
            .unwrap_or(0),
        best.x
            - results
                .iter()
                .find(|r| r.name == "baseline_run")
                .map(|r| r.x)
                .unwrap_or(0)
    );
}

// --------------------------------------------------------------------------- //
// Selftest de determinismo del snapshot
// --------------------------------------------------------------------------- //

fn state_hash(con: &Console) -> u64 {
    let mut h = DefaultHasher::new();
    h.write(&con.bus.ram[..]);
    h.write_u8(con.cpu.a);
    h.write_u8(con.cpu.x);
    h.write_u8(con.cpu.y);
    h.write_u8(con.cpu.sp);
    h.write_u16(con.cpu.pc);
    h.write_u8(con.cpu.p);
    h.write_u64(con.cpu.cycles);
    let p = &con.bus.ppu;
    h.write(&p.vram[..]);
    h.write(&p.palette[..]);
    h.write(&p.oam[..]);
    h.write_u16(p.v);
    h.write_u16(p.t);
    h.write_u8(p.ctrl);
    h.write_u8(p.mask);
    h.write_u8(p.status);
    h.write_u16(p.scanline);
    h.write_u16(p.dot);
    h.finish()
}

fn selftest(rom: &[u8], outdir: &Path) -> bool {
    let cart = Cartridge::from_bytes(rom).expect("cartucho inválido");
    let mut con = Console::new(cart);
    let Some((snap, _boot)) = boot_to_level(&mut con, 2400) else {
        eprintln!("selftest: no se pudo bootear el nivel");
        return false;
    };

    let mut report = String::from("# deepscan selftest\n\n");
    let mut ok = true;

    // Test 1: mismo snapshot, misma secuencia -> mismo estado.
    let mut hashes = Vec::new();
    for _ in 0..2 {
        con.restore(&snap);
        for f in 0..120u32 {
            let b = if f % 8 < 4 {
                BUTTON_RIGHT | BUTTON_B
            } else {
                BUTTON_RIGHT | BUTTON_B | BUTTON_A
            };
            con.set_buttons(0, b);
            con.step_frame();
            con.bus.apu.samples.clear();
        }
        hashes.push(state_hash(&con));
    }
    let t1 = hashes[0] == hashes[1];
    ok &= t1;
    report.push_str(&format!(
        "- [{}] replay desde inicio: {:016x} vs {:016x}\n",
        if t1 { "PASS" } else { "FAIL" },
        hashes[0],
        hashes[1]
    ));

    // Test 2: snapshot en el aire (tras 40f) y replay.
    con.restore(&snap);
    for _ in 0..40 {
        con.set_buttons(0, BUTTON_RIGHT | BUTTON_B | BUTTON_A);
        con.step_frame();
        con.bus.apu.samples.clear();
    }
    let mid = con.snapshot();
    let mut hashes2 = Vec::new();
    for phase in 0..2 {
        if phase == 1 {
            con.restore(&mid);
        }
        for _ in 0..60 {
            con.set_buttons(0, BUTTON_RIGHT | BUTTON_B);
            con.step_frame();
            con.bus.apu.samples.clear();
        }
        hashes2.push(state_hash(&con));
    }
    let t2 = hashes2[0] == hashes2[1];
    ok &= t2;
    report.push_str(&format!(
        "- [{}] replay desde el aire: {:016x} vs {:016x}\n",
        if t2 { "PASS" } else { "FAIL" },
        hashes2[0],
        hashes2[1]
    ));

    std::fs::write(outdir.join("selftest.txt"), report).expect("error selftest.txt");
    println!("selftest: {}", if ok { "PASS" } else { "FAIL" });
    ok
}
