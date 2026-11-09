//! Búsqueda en haz (beam search) sobre rollouts reales del emulador.
//!
//! Cada nodo guarda un `ConsoleSnapshot` determinista y la secuencia de
//! botones que llevó hasta él. En cada paso se prueban acciones macro
//! (correr, saltar con distintos holds de A, frenar, esperar) y se conservan
//! los `beam_width` estados con mejor puntuación. La puntuación premia avanzar
//! en X y castiga el tiempo, así que la ruta encontrada es una ruta rápida,
//! no solo una ruta que llega.
//!
//! Es la pieza "empírica": no confía en el modelo de física, ejecuta el juego.

use crate::console::{
    Console, ConsoleSnapshot, BUTTON_A, BUTTON_B, BUTTON_DOWN, BUTTON_LEFT, BUTTON_RIGHT,
};
use crate::route::sensors::{is_solid, scan_environment};

#[derive(Clone, Copy, Debug)]
enum Action {
    Run(u32),
    Jump(u8),
    /// Salto vertical (sin RIGHT) y luego avance: sirve para subir un escalón
    /// que está justo encima de Mario, donde un salto hacia delante choca.
    JumpUp(u8),
    /// Retrocede `back` frames y luego corre+salta: carrerilla para muros
    /// pegados donde el beam se queda sin espacio.
    BackJump(u32, u8),
    Wait(u32),
    Brake(u32),
    Moonwalk(u32),
    /// Congela el subspeed en el aire soltando la dirección 1 frame
    /// (subspeed manipulation, ver `tricks.rs`): ayuda a alcanzar la
    /// velocidad máxima más rápido tras la aceleración inversa.
    SubFreeze,
    /// Pulso de giro de 1 frame `LEFT` (sin B) y luego correr: da fricción
    /// doble 1 frame (`facing != moving`) para re-acelerar sin perder
    /// velocidad neta. Solo en suelo.
    Turn1,
    /// Pulso `L+R+B` de 1 frame y luego correr: `facing=3`, siempre fricción
    /// doble ese frame. Solo en suelo.
    LR1,
    /// Entrada en tubería warp: mantener `DOWN` sobre la tubería (sub==7).
    PipeDown(u32),
    /// Arranque con el truco de aceleración inversa (sección 18 del
    /// catálogo): L+R 1f, A 1f, carrera, giro de 1f sin B y salto con B.
    /// Solo se prueba en el nodo raíz.
    AccelStart,
}

#[derive(Clone)]
struct Node {
    snap: ConsoleSnapshot,
    frame: u32,
    x: i32,
    y: i32,
    vx: i32,
    state: u8,
    on_ground: bool,
    inputs: Vec<u8>,
    score: f64,
    dead: bool,
    reached_goal: bool,
    stuck_frames: u32,
    last_x: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GoalMode {
    /// Mástil (niveles no-warp): state==3 / sub 4/5 / $070F.
    Flagpole,
    /// Entrada en tubería warp: `GameEngineSubroutine == 7` y `$0750` activo,
    /// en la zona de warps (X > goal_x).
    Warp,
    /// Hacha de castillo: `OperMode == 2` (sprites invisibles, fin SDA 8-4).
    Axe,
}

#[derive(Clone, Debug)]
pub struct BeamConfig {
    pub beam_width: usize,
    pub max_frames: u32,
    pub stride: u32,
    pub goal_x: i32,
    pub max_rollout: u32,
    pub time_weight: f64,
    pub verbose: bool,
    pub goal_mode: GoalMode,
}

impl Default for BeamConfig {
    fn default() -> Self {
        Self {
            beam_width: 24,
            max_frames: 4000,
            stride: 6,
            goal_x: 3160,
            max_rollout: 60,
            // 2.5 = velocidad máxima (px/frame). El score equivale a
            // "frame + (goal_x - x) / vx_max": es una estimación del tiempo
            // total, no una preferencia por macro-acciones largas. Con 0.7 el
            // beam premiaba saltos de 40 frames sobre correr 8.
            time_weight: 2.5,
            verbose: true,
            goal_mode: GoalMode::Flagpole,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BeamTrace {
    pub step: u32,
    pub frame: u32,
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub score: f64,
    pub beam_len: usize,
    pub goal_frame: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct BeamResult {
    /// Botones frame a frame desde el inicio del nivel (sin el boot).
    pub level_inputs: Vec<u8>,
    /// Botones desde power-on (boot + nivel). Listo para `nesdump --script`.
    pub inputs: Vec<u8>,
    /// Frames de nivel hasta el mejor estado.
    pub frames: u32,
    /// Frames de boot (título -> nivel).
    pub boot_frames: u32,
    pub x: i32,
    pub y: i32,
    pub reached_goal: bool,
    pub trace: Vec<BeamTrace>,
    pub rollouts: u64,
    pub nodes: u64,
}

struct State {
    x: i32,
    y: i32,
    /// `Player_Y_HighPos` ($0B): 1 = Mario ha caído por debajo del nivel.
    /// El byte bajo solo no sirve (un salto alto puede dar 255 arriba).
    y_high: u8,
    vx: i32,
    state: u8,
    on_ground: bool,
    lives: u8,
    op: u8,
    timer: u8,
    sub: u8,
    /// `$0756`: 0 = pequeño, 1 = grande, 2 = fuego. Cambia la altura del
    /// hitbox usada por los sensores.
    power: u8,
    /// `$070F`: distinto de 0 cuando el flagpole registró la colisión.
    flagpole_y: u8,
}

fn read_state(con: &Console) -> State {
    let r = &con.bus.ram;
    let x = (r[0x006D] as i32) * 256 + (r[0x0086] as i32);
    State {
        x,
        y: r[0x00CE] as i32,
        y_high: r[0x000B],
        vx: (r[0x0057] as i8) as i32,
        state: r[0x001D],
        on_ground: r[0x001D] == 0,
        lives: r[0x075A],
        op: r[0x0770],
        timer: r[0x07F8],
        sub: r[0x000E],
        power: r[0x0756],
        flagpole_y: r[0x070F],
    }
}

/// ¿Debe cortarse un `Run` en este frame? Se para en el punto de despegue
/// seguro para el obstáculo más cercano. Sin esto, el beam solo puede saltar
/// en múltiplos del stride y pierde velocidad contra tuberías/escaleras.
fn run_should_stop(ram: &[u8; 2048], x: i32, y: i32, power: u8) -> bool {
    let feet = y + if power == 0 { 32 } else { 48 };
    let scan = scan_environment(ram, x, feet);

    // Pozo: despegar con margen.
    if scan.pit_dist <= 40 {
        return true;
    }

    // Muro/tubería: el margen crece con la altura. Para 4 tiles (64 px)
    // el despegue seguro está a ~88 px; para un escalón de 1 tile, a ~40 px.
    if scan.wall_dist <= 999 {
        let safe = (scan.wall_height + 24).clamp(24, 96);
        if scan.wall_dist <= safe {
            return true;
        }
    }

    // Borde con caída grande (valle de la pirámide de 1-1): si a 20 px ya no
    // hay suelo al nivel de los pies y tampoco 56 px más abajo, es un salto
    // de 4 tiles, no una escalera.
    if !is_solid(ram, x + 20, feet) && !is_solid(ram, x + 20, feet + 56) {
        return true;
    }

    false
}

fn bcd_to_dec(b: u8) -> i32 {
    ((b >> 4) as i32) * 10 + (b & 0x0F) as i32
}

/// Arranca el juego desde power-on hasta el frame 1 del nivel 1-1.
/// Devuelve el snapshot inicial y la secuencia de botones usada.
pub fn boot_to_level(con: &mut Console, max_frames: u32) -> Option<(ConsoleSnapshot, Vec<u8>)> {
    let mut inputs = Vec::new();
    for f in 0..max_frames {
        let r = &con.bus.ram;
        let op = r[0x0770];
        let timer = bcd_to_dec(r[0x07F8]);
        let sub = r[0x000E];
        let x = (r[0x006D] as i32) * 256 + (r[0x0086] as i32);
        let on_ground = r[0x001D] == 0;
        if op == 0x01 && (1..=3).contains(&timer) && sub == 0x08 && x > 30 && on_ground {
            return Some((con.snapshot(), inputs));
        }
        let b = if f % 30 < 2 { crate::console::BUTTON_START } else { 0 };
        inputs.push(b);
        con.set_buttons(0, b);
        if !con.step_frame() {
            return None;
        }
        con.bus.apu.samples.clear();
    }
    None
}

fn build_sequence(action: Action, prev_a: bool, cfg: &BeamConfig) -> Vec<u8> {
    let mut seq = Vec::new();
    match action {
        Action::Run(n) => {
            for _ in 0..n {
                seq.push(BUTTON_RIGHT | BUTTON_B);
            }
        }
        Action::Jump(hold) => {
            if prev_a {
                // Flanco limpio: SMB1 exige A=0 en el frame anterior.
                seq.push(BUTTON_RIGHT | BUTTON_B);
                seq.push(BUTTON_RIGHT | BUTTON_B);
            }
            for _ in 0..hold.max(1) {
                seq.push(BUTTON_RIGHT | BUTTON_B | BUTTON_A);
            }
            let used = seq.len() as u32;
            let rest = cfg.max_rollout.saturating_sub(used);
            for _ in 0..rest {
                seq.push(BUTTON_RIGHT | BUTTON_B);
            }
        }
        Action::JumpUp(hold) => {
            if prev_a {
                seq.push(BUTTON_B);
                seq.push(BUTTON_B);
            }
            // Sube en vertical y solo avanza al final del impulso.
            for _ in 0..hold.max(1) {
                seq.push(BUTTON_B | BUTTON_A);
            }
            let used = seq.len() as u32;
            let rest = cfg.max_rollout.saturating_sub(used);
            for _ in 0..rest {
                seq.push(BUTTON_RIGHT | BUTTON_B);
            }
        }
        Action::BackJump(back, hold) => {
            for _ in 0..back {
                seq.push(BUTTON_LEFT);
            }
            seq.push(BUTTON_RIGHT | BUTTON_B);
            for _ in 0..hold.max(1) {
                seq.push(BUTTON_RIGHT | BUTTON_B | BUTTON_A);
            }
            let used = seq.len() as u32;
            let rest = cfg.max_rollout.saturating_sub(used);
            for _ in 0..rest {
                seq.push(BUTTON_RIGHT | BUTTON_B);
            }
        }
        Action::Wait(n) => {
            for _ in 0..n {
                seq.push(0);
            }
        }
        Action::Brake(n) => {
            for _ in 0..n {
                seq.push(BUTTON_LEFT | BUTTON_B);
            }
        }
        Action::Moonwalk(n) => {
            seq.push(BUTTON_LEFT | BUTTON_RIGHT | BUTTON_B);
            for _ in 1..n {
                seq.push(BUTTON_RIGHT | BUTTON_B);
            }
        }
        Action::SubFreeze => {
            seq.push(BUTTON_B); // sin dirección: subspeed congelado
            let used = seq.len() as u32;
            let rest = cfg.max_rollout.saturating_sub(used);
            for _ in 0..rest {
                seq.push(BUTTON_RIGHT | BUTTON_B);
            }
        }
        Action::Turn1 => {
            seq.push(BUTTON_LEFT);
            let used = seq.len() as u32;
            let rest = cfg.max_rollout.saturating_sub(used);
            for _ in 0..rest {
                seq.push(BUTTON_RIGHT | BUTTON_B);
            }
        }
        Action::LR1 => {
            seq.push(BUTTON_LEFT | BUTTON_RIGHT | BUTTON_B);
            let used = seq.len() as u32;
            let rest = cfg.max_rollout.saturating_sub(used);
            for _ in 0..rest {
                seq.push(BUTTON_RIGHT | BUTTON_B);
            }
        }
        Action::PipeDown(n) => {
            for _ in 0..n {
                seq.push(BUTTON_DOWN);
            }
        }
        Action::AccelStart => {
            // Secuencia medida en `--mode probe`: el giro en el frame 24 es
            // el que mejor X deja a los 90 frames.
            seq.push(BUTTON_LEFT | BUTTON_RIGHT);
            seq.push(BUTTON_A);
            for _ in 0..22 {
                seq.push(BUTTON_RIGHT | BUTTON_B);
            }
            seq.push(BUTTON_LEFT);
            seq.push(BUTTON_RIGHT | BUTTON_B | BUTTON_A);
            let used = seq.len() as u32;
            let rest = cfg.max_rollout.saturating_sub(used);
            for _ in 0..rest {
                seq.push(BUTTON_RIGHT | BUTTON_B);
            }
        }
    }
    seq
}

fn rollout(
    con: &mut Console,
    parent: &Node,
    action: Action,
    cfg: &BeamConfig,
    start_lives: u8,
) -> Node {
    con.restore(&parent.snap);
    let mut child = parent.clone();
    child.dead = false;
    child.reached_goal = false;
    let prev_a = parent
        .inputs
        .last()
        .map(|b| b & BUTTON_A != 0)
        .unwrap_or(false);
    let seq = build_sequence(action, prev_a, cfg);
    let mut frames_done = 0u32;
    // Para BackJump, los primeros frames son de retroceso en el suelo: no se
    // puede cortar el rollout por "ya está en el suelo" hasta después del salto.
    let jump_grace = match action {
        Action::BackJump(back, _) => back + 4,
        Action::Jump(_) | Action::JumpUp(_) => 4,
        _ => u32::MAX,
    };
    // Los `Run` (y los pulsos de 1 frame) se cortan en el despegue seguro.
    let event_stop = matches!(
        action,
        Action::Run(_)
            | Action::Turn1
            | Action::LR1
            | Action::SubFreeze
            | Action::Moonwalk(_)
    );

    for b in seq {
        con.set_buttons(0, b);
        if !con.step_frame() {
            child.dead = true;
            break;
        }
        con.bus.apu.samples.clear();
        child.inputs.push(b);
        child.frame += 1;
        frames_done += 1;

        let s = read_state(con);
        child.x = s.x;
        child.y = s.y;
        child.vx = s.vx;
        child.state = s.state;
        child.on_ground = s.on_ground;

        if s.lives < start_lives {
            child.dead = true;
            break;
        }
        if s.op != 0x01 {
            if s.op == 0x02 || s.state == 3 || s.sub == 4 || s.sub == 5 {
                child.reached_goal = true;
            } else {
                child.dead = true;
            }
            break;
        }
        if s.timer == 0 {
            child.dead = true;
            break;
        }
        if s.y_high == 1 {
            child.dead = true;
            break;
        }
        // Objetivo según modo: mástil / warp (sub==7) / hacha (OperMode==2).
        // El detector de tubería usa $0750 != 0 (AreaPointer cargado).
        let area_ptr = con.bus.ram[0x0750];
        let goal_hit = match cfg.goal_mode {
            GoalMode::Flagpole => {
                s.state == 3 || s.sub == 4 || s.sub == 5 || s.flagpole_y != 0
            }
            GoalMode::Warp => s.sub == 7 && area_ptr != 0 && s.x > cfg.goal_x - 400,
            GoalMode::Axe => s.op == 0x02,
        };
        if goal_hit {
            child.reached_goal = true;
            break;
        }

        if s.x > child.last_x + 1 {
            child.last_x = s.x;
            child.stuck_frames = 0;
        } else {
            child.stuck_frames += 1;
        }
        if child.stuck_frames > 120 {
            child.dead = true;
            break;
        }

        if event_stop
            && frames_done > 1
            && s.on_ground
            && run_should_stop(&con.bus.ram, s.x, s.y, s.power)
        {
            break;
        }

        if frames_done > jump_grace && s.on_ground {
            break; // aterrizaje: decisión siguiente
        }
    }

    child.snap = con.snapshot();
    child.score = child.x as f64 - cfg.time_weight * child.frame as f64
        + 0.4 * child.vx as f64
        + if child.on_ground { 2.0 } else { 0.0 };
    // Cerca del mástil, premia ligeramente el subpíxel que activa la
    // colisión estrecha (nybble bajo 6..9). Sin esto el beam puede quedarse
    // con una variante que pasa de largo sin tocar la bandera.
    if child.x > cfg.goal_x - 64
        && child.x < cfg.goal_x + 32
        && (6..=9).contains(&(child.x & 0x0F))
    {
        child.score += 8.0;
    }
    child
}

/// Búsqueda en haz desde un snapshot al inicio del nivel.
pub fn beam_search(
    con: &mut Console,
    start: &ConsoleSnapshot,
    cfg: &BeamConfig,
) -> BeamResult {
    con.restore(start);
    let s0 = read_state(con);
    let start_lives = s0.lives;
    let root = Node {
        snap: start.clone(),
        frame: 0,
        x: s0.x,
        y: s0.y,
        vx: s0.vx,
        state: s0.state,
        on_ground: s0.on_ground,
        inputs: Vec::new(),
        score: 0.0,
        dead: false,
        reached_goal: false,
        stuck_frames: 0,
        last_x: s0.x,
    };

    let mut beam = vec![root];
    let mut best_goal: Option<Node> = None;
    let mut best_alive: Option<Node> = None;
    let mut best_dead: Option<Node> = None;
    let mut trace = Vec::new();
    let mut rollouts = 0u64;
    let mut nodes = 0u64;

    let max_steps = (cfg.max_frames as usize).saturating_mul(2).min(6000);
    for step in 0..max_steps {
        if beam.is_empty() {
            break;
        }
        if beam.iter().all(|n| n.frame >= cfg.max_frames) {
            break;
        }

        let mut children = Vec::new();
        for node in &beam {
            let mut actions = if node.on_ground {
                vec![
                    Action::Jump(26),
                    Action::Jump(20),
                    Action::Jump(14),
                    Action::Jump(8),
                    Action::JumpUp(26),
                    Action::JumpUp(20),
                    Action::BackJump(14, 26),
                    Action::BackJump(28, 26),
                    Action::Run(cfg.stride),
                    Action::Wait(cfg.stride),
                    Action::Brake(cfg.stride),
                    Action::Moonwalk(cfg.stride),
                    Action::SubFreeze,
                    Action::Turn1,
                    Action::LR1,
                    Action::PipeDown(cfg.stride),
                ]
            } else {
                vec![
                    Action::Run(cfg.stride),
                    Action::Wait(cfg.stride),
                    Action::Brake(cfg.stride),
                    Action::SubFreeze,
                ]
            };
            if node.frame == 0 {
                actions.push(Action::AccelStart);
            }
            for action in actions {
                let child = rollout(con, node, action, cfg, start_lives);
                rollouts += 1;
                if child.reached_goal {
                    if best_goal
                        .as_ref()
                        .map_or(true, |g| child.frame < g.frame)
                    {
                        best_goal = Some(child);
                    }
                    continue;
                }
                if child.dead {
                    if best_dead.as_ref().map_or(true, |b| child.x > b.x) {
                        best_dead = Some(child);
                    }
                    continue;
                }
                if best_alive.as_ref().map_or(true, |b| child.x > b.x) {
                    best_alive = Some(child.clone());
                }
                children.push(child);
            }
        }

        if children.is_empty() {
            break;
        }
        children.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut seen = std::collections::HashSet::new();
        let mut next = Vec::with_capacity(cfg.beam_width);
        for child in children {
            let last_a = child
                .inputs
                .last()
                .map(|b| b & BUTTON_A != 0)
                .unwrap_or(false);
            // Cerca del objetivo no se puede deduplicar por bloques de 8 px:
            // la colisión del flagpole depende del nybble bajo de X.
            let key = if child.x > cfg.goal_x - 160 {
                (
                    child.x,
                    child.y / 4,
                    child.vx,
                    child.state as i32,
                    child.on_ground as i32,
                    last_a as i32,
                )
            } else {
                (
                    child.x / 4,
                    child.y / 8,
                    child.vx / 2,
                    child.state as i32,
                    child.on_ground as i32,
                    last_a as i32,
                )
            };
            if seen.insert(key) {
                next.push(child);
                if next.len() >= cfg.beam_width {
                    break;
                }
            }
        }
        nodes += next.len() as u64;
        beam = next;

        if let Some(best) = beam.first() {
            trace.push(BeamTrace {
                step: step as u32,
                frame: best.frame,
                x: best.x,
                y: best.y,
                vx: best.vx,
                score: best.score,
                beam_len: beam.len(),
                goal_frame: best_goal.as_ref().map(|g| g.frame),
            });
            if cfg.verbose && step % 20 == 0 {
                eprintln!(
                    "[beam] step={step:3} best_f={:4} x={:4} y={:3} vx={:2} score={:8.1} beam={:2} goal={:?}",
                    best.frame,
                    best.x,
                    best.y,
                    best.vx,
                    best.score,
                    beam.len(),
                    best_goal.as_ref().map(|g| g.frame)
                );
            }
        }

        if let Some(g) = &best_goal {
            let can_beat = beam.iter().any(|n| {
                n.frame as f64 + ((cfg.goal_x - n.x).max(0) as f64 / 2.5) < g.frame as f64
            });
            if !can_beat {
                break;
            }
        }
    }

    let best = best_goal
        .or(best_alive)
        .or(best_dead)
        .or_else(|| beam.into_iter().max_by_key(|n| n.x))
        .expect("beam vacío");

    BeamResult {
        level_inputs: best.inputs.clone(),
        inputs: best.inputs,
        frames: best.frame,
        boot_frames: 0,
        x: best.x,
        y: best.y,
        reached_goal: best.reached_goal,
        trace,
        rollouts,
        nodes,
    }
}

/// Convierte una secuencia de botones frame a frame en eventos
/// `frame:boton:valor` para `nesdump --script`.
pub fn inputs_to_script(inputs: &[u8]) -> String {
    let pairs = [
        (BUTTON_A, "a"),
        (BUTTON_B, "b"),
        (crate::console::BUTTON_SELECT, "select"),
        (crate::console::BUTTON_START, "start"),
        (crate::console::BUTTON_UP, "up"),
        (crate::console::BUTTON_DOWN, "down"),
        (BUTTON_LEFT, "left"),
        (BUTTON_RIGHT, "right"),
    ];
    let mut events = Vec::new();
    let mut prev = 0u8;
    for (frame, &b) in inputs.iter().enumerate() {
        if b == prev {
            continue;
        }
        for (mask, name) in pairs {
            if (b & mask) != (prev & mask) {
                events.push(format!("{frame}:{name}:{}", if b & mask != 0 { 1 } else { 0 }));
            }
        }
        prev = b;
    }
    events.join(",")
}
