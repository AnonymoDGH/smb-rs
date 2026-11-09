//! Primitivas y verificación empírica de bugs/trucos de SMB1.
//!
//! Este módulo no "cree" en el catálogo: ejecuta secuencias reales sobre el
//! emulador y mide el resultado (X, velocidad, altura, frames). Sirve para
//! convertir los trucos de `analysis/BUGS_Y_TRUCOS.md` en acciones que el
//! beam search puede probar.
//!
//! Convenciones de botones: las mismas de `crate::console` (A=0x01, B=0x02,
//! LEFT=0x40, RIGHT=0x80). `L+R` es `BUTTON_LEFT | BUTTON_RIGHT`.

use crate::console::{
    Console, ConsoleSnapshot, BUTTON_A, BUTTON_B, BUTTON_LEFT, BUTTON_RIGHT,
};

/// L+R simultáneos: base del truco de aceleración inversa y del moonwalk.
pub const LR: u8 = BUTTON_LEFT | BUTTON_RIGHT;

/// Estado relevante para detectar trucos. Todas las direcciones son las de
/// la RAM de SMB1; se leen en cada frame de un rollout.
#[derive(Clone, Copy, Debug, Default)]
pub struct TrickState {
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub vy: i32,
    /// `$0B` Player_Y_HighPos: 1 = cayó fuera del nivel.
    pub y_high: u8,
    /// `$1D` Player_State (0 = suelo).
    pub state: u8,
    pub on_ground: bool,
    /// `$33` PlayerFacingDir.
    pub facing: u8,
    /// `$45` Player_MovingDir.
    pub moving: u8,
    /// `$0705/$0706` Player_X_MoveForce (subpíxel horizontal).
    pub x_force: u16,
    /// `$077F` contador de la regla de 21 frames.
    pub frame_rule: u8,
    /// `$070F` FlagpoleCollisionYPos (0 = sin colisión).
    pub flagpole_y: u8,
    /// `$010F` FlagpoleScore.
    pub flagpole_score: u8,
    /// `$0756` PlayerStatus (0 = pequeño, 1 = grande, 2 = fuego).
    pub power: u8,
    pub lives: u8,
    pub timer: u8,
    pub op: u8,
    pub sub: u8,
}

impl TrickState {
    #[inline]
    pub fn x_low_nybble(&self) -> u8 {
        (self.x & 0x0F) as u8
    }

    /// Ventana de colisión estrecha del flagpole: nybble bajo de X en 6..=9.
    #[inline]
    pub fn flagpole_window(&self) -> bool {
        (6..=9).contains(&self.x_low_nybble())
    }

    /// Mario mira a la derecha (`$33` = 1).
    #[inline]
    pub fn facing_right(&self) -> bool {
        self.facing == 1
    }

    /// El movimiento va a la derecha (`$45` = 1).
    #[inline]
    pub fn moving_right(&self) -> bool {
        self.moving == 1
    }

    /// Fricción duplicada: mira y se mueve en direcciones distintas.
    #[inline]
    pub fn reverse_friction(&self) -> bool {
        self.facing != 0 && self.moving != 0 && self.facing != self.moving
    }
}

#[inline]
pub fn read_trick_state(con: &Console) -> TrickState {
    let r = &con.bus.ram;
    let state = r[0x001D];
    TrickState {
        x: (r[0x006D] as i32) * 256 + (r[0x0086] as i32),
        y: r[0x00CE] as i32,
        vx: (r[0x0057] as i8) as i32,
        vy: (r[0x009F] as i8) as i32,
        y_high: r[0x000B],
        state,
        on_ground: state == 0,
        facing: r[0x0033],
        moving: r[0x0045],
        x_force: (r[0x0705] as u16) | ((r[0x0706] as u16) << 8),
        frame_rule: r[0x077F],
        flagpole_y: r[0x070F],
        flagpole_score: r[0x010F],
        power: r[0x0756],
        lives: r[0x075A],
        timer: r[0x07F8],
        op: r[0x0770],
        sub: r[0x000E],
    }
}

/// Especificación de una prueba: prefijo exacto frame a frame y cola
/// constante (normalmente `RIGHT|B`).
#[derive(Clone, Debug)]
pub struct ProbeSpec {
    pub name: String,
    pub prefix: Vec<u8>,
    pub tail: u8,
    pub frames: u32,
}

impl ProbeSpec {
    pub fn new(name: impl Into<String>, prefix: Vec<u8>, tail: u8, frames: u32) -> Self {
        Self {
            name: name.into(),
            prefix,
            tail,
            frames,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProbeResult {
    pub name: String,
    pub frames: u32,
    pub x: i32,
    pub y: i32,
    pub vx: i32,
    pub max_vx: i32,
    /// Altura máxima sobre la Y inicial, en px (positivo = arriba).
    pub peak_height: i32,
    /// Frame en el que volvió al suelo tras el primer salto.
    pub landed: Option<u32>,
    pub final_state: TrickState,
    /// Muestras cada 5 frames: (frame, x, vx, y).
    pub trace: Vec<(u32, i32, i32, i32)>,
}

/// Ejecuta una prueba desde un snapshot. No mata el proceso si el juego
/// cambia de modo: se detiene y devuelve lo observado.
pub fn run_probe(con: &mut Console, start: &ConsoleSnapshot, spec: &ProbeSpec) -> ProbeResult {
    con.restore(start);
    let s0 = read_trick_state(con);
    let mut max_vx = 0i32;
    let mut min_y = s0.y;
    let mut landed = None;
    let mut was_air = false;
    let mut trace = Vec::new();
    let mut last = s0;
    let mut frames = 0u32;

    for f in 0..spec.frames {
        let b = spec.prefix.get(f as usize).copied().unwrap_or(spec.tail);
        con.set_buttons(0, b);
        if !con.step_frame() {
            break;
        }
        con.bus.apu.samples.clear();
        let s = read_trick_state(con);
        frames = f + 1;
        max_vx = max_vx.max(s.vx);
        min_y = min_y.min(s.y);
        if was_air && s.on_ground && landed.is_none() {
            landed = Some(frames);
        }
        was_air = !s.on_ground;
        if f % 5 == 0 {
            trace.push((frames, s.x, s.vx, s.y));
        }
        last = s;
        if s.op != 1 || s.lives < s0.lives || s.y_high == 1 {
            break;
        }
    }

    ProbeResult {
        name: spec.name.clone(),
        frames,
        x: last.x,
        y: last.y,
        vx: last.vx,
        max_vx,
        peak_height: s0.y - min_y,
        landed,
        final_state: last,
        trace,
    }
}

fn hold(n: u32, b: u8) -> Vec<u8> {
    vec![b; n as usize]
}

/// Suite de pruebas de arranque: compara la aceleración normal con las
/// variantes del truco "accelerating faster" (sección 18 del catálogo).
///
/// Devuelve las pruebas ordenadas por X alcanzada a los 90 frames (mayor es
/// mejor); el llamador decide si además exige velocidad máxima.
pub fn probe_start(con: &mut Console, start: &ConsoleSnapshot) -> Vec<ProbeResult> {
    let frames = 90u32;
    let mut specs = Vec::new();

    // Línea base: correr desde el primer frame.
    specs.push(ProbeSpec::new(
        "baseline_run",
        Vec::new(),
        BUTTON_RIGHT | BUTTON_B,
        frames,
    ));

    // Salto inicial con low-gravity y distintos holds de A.
    for h in [1u32, 2, 3, 4, 6, 8, 10, 14, 20, 26] {
        specs.push(ProbeSpec::new(
            format!("first_jump_hold{h}"),
            hold(h, BUTTON_A),
            BUTTON_RIGHT | BUTTON_B,
            frames,
        ));
    }

    // Salto inicial con A+RIGHT (el que usa la ruta actual).
    for h in [2u32, 4, 6, 8, 12, 20, 26] {
        specs.push(ProbeSpec::new(
            format!("first_jump_right_hold{h}"),
            hold(h, BUTTON_A | BUTTON_RIGHT),
            BUTTON_RIGHT | BUTTON_B,
            frames,
        ));
    }

    // Truco documentado: L+R 1f, A 1f, B, giro de 1f sin B y salto.
    // El giro se prueba en una ventana de frames porque el aterrizaje
    // depende del subpíxel.
    for turn in [18u32, 20, 22, 24, 26, 28, 30, 32, 34, 36] {
        let mut p = vec![LR, BUTTON_A];
        p.extend(hold(turn.saturating_sub(2), BUTTON_RIGHT | BUTTON_B));
        p.push(BUTTON_LEFT);
        p.push(BUTTON_RIGHT | BUTTON_B | BUTTON_A);
        specs.push(ProbeSpec::new(
            format!("accel_lr_a_turn{turn}"),
            p,
            BUTTON_RIGHT | BUTTON_B,
            frames,
        ));
    }

    // Variantes más simples del truco: L+R y luego correr; giro sin salto.
    specs.push(ProbeSpec::new(
        "lr_then_run",
        vec![LR],
        BUTTON_RIGHT | BUTTON_B,
        frames,
    ));
    for back in [1u32, 2, 3, 4, 6, 8] {
        let mut p = hold(20, BUTTON_RIGHT | BUTTON_B);
        p.extend(hold(back, BUTTON_LEFT));
        p.push(BUTTON_RIGHT | BUTTON_B | BUTTON_A);
        specs.push(ProbeSpec::new(
            format!("run20_back{back}_jump"),
            p,
            BUTTON_RIGHT | BUTTON_B,
            frames,
        ));
    }

    let mut out: Vec<ProbeResult> = specs
        .iter()
        .map(|s| run_probe(con, start, s))
        .collect();
    out.sort_by(|a, b| b.x.cmp(&a.x).then(b.max_vx.cmp(&a.max_vx)));
    out
}

/// Compara un salto en el primer frame (low-gravity) con el mismo salto tras
/// correr `run` frames. Devuelve (primer salto, salto tras correr).
pub fn probe_low_gravity(
    con: &mut Console,
    start: &ConsoleSnapshot,
    hold_a: u32,
    run_frames: u32,
) -> (ProbeResult, ProbeResult) {
    let frames = 120u32;
    let first = ProbeSpec::new(
        format!("first_jump_h{hold_a}"),
        hold(hold_a, BUTTON_A),
        BUTTON_RIGHT | BUTTON_B,
        frames,
    );
    let mut p = hold(run_frames, BUTTON_RIGHT | BUTTON_B);
    p.extend(hold(hold_a, BUTTON_RIGHT | BUTTON_B | BUTTON_A));
    let later = ProbeSpec::new(
        format!("run{run_frames}_jump_h{hold_a}"),
        p,
        BUTTON_RIGHT | BUTTON_B,
        frames,
    );
    (
        run_probe(con, start, &first),
        run_probe(con, start, &later),
    )
}

/// Resumen legible de una prueba (para `--mode probe` y reportes).
pub fn probe_line(r: &ProbeResult) -> String {
    format!(
        "| `{}` | {} | {} | {} | {} | {} | {:?} | {} |",
        r.name,
        r.frames,
        r.x,
        r.vx,
        r.max_vx,
        r.peak_height,
        r.landed,
        r.final_state.flagpole_window() as u8
    )
}

/// Informe de trucos detectados en una ruta ya ejecutada.
#[derive(Clone, Debug, Default)]
pub struct TrickReport {
    pub frames: u32,
    pub max_vx: i32,
    pub max_vx_frame: u32,
    pub top_speed_frames: u32,
    pub reverse_friction_frames: u32,
    pub moonwalk_frames: u32,
    pub airborne_frames: u32,
    pub jumps: u32,
    pub first_jump_frame: Option<u32>,
    pub first_jump_airtime: Option<u32>,
    pub flagpole_frame: Option<u32>,
    pub flagpole_y: u8,
    pub flagpole_score: u8,
    pub flagpole_window: bool,
    pub events: Vec<String>,
}

impl TrickReport {
    pub fn to_markdown(&self) -> String {
        let mut md = String::from("# Trucos detectados en la ruta\n\n");
        md.push_str(&format!(
            "- Frames de nivel: {}\n- Velocidad máxima: {} (frame {})\n- Frames a velocidad tope: {}\n- Frames con fricción inversa: {}\n- Frames de moonwalk (L+R): {}\n- Frames en el aire: {}\n- Saltos: {}\n",
            self.frames,
            self.max_vx,
            self.max_vx_frame,
            self.top_speed_frames,
            self.reverse_friction_frames,
            self.moonwalk_frames,
            self.airborne_frames,
            self.jumps
        ));
        md.push_str(&format!(
            "- Primer salto: frame {:?}, airtime {:?}\n",
            self.first_jump_frame, self.first_jump_airtime
        ));
        md.push_str(&format!(
            "- Flagpole: frame {:?}, Y={}, score={}, ventana subpíxel={}\n\n",
            self.flagpole_frame,
            self.flagpole_y,
            self.flagpole_score,
            if self.flagpole_window { "sí" } else { "no" }
        ));
        md.push_str("## Eventos\n\n");
        for e in &self.events {
            md.push_str(&format!("- {e}\n"));
        }
        md
    }
}

/// Analiza una secuencia de muestras `(frame_de_nivel, botones, estado)`.
pub fn analyze_tricks(samples: &[(u32, u8, TrickState)]) -> TrickReport {
    let mut rep = TrickReport::default();
    let mut prev_ground = true;
    let mut jump_start: Option<u32> = None;
    let mut rev_start: Option<u32> = None;
    let mut top_reached = false;

    for &(frame, buttons, s) in samples {
        rep.frames = frame + 1;
        if s.vx > rep.max_vx {
            rep.max_vx = s.vx;
            rep.max_vx_frame = frame;
        }
        if s.vx >= 40 {
            rep.top_speed_frames += 1;
            if !top_reached {
                top_reached = true;
                rep.events
                    .push(format!("velocidad máxima (40) en frame {frame}"));
            }
        }
        if !s.on_ground {
            rep.airborne_frames += 1;
        }
        if buttons & LR == LR {
            rep.moonwalk_frames += 1;
        }
        if s.reverse_friction() {
            rep.reverse_friction_frames += 1;
            if rev_start.is_none() {
                rev_start = Some(frame);
                rep.events.push(format!(
                    "fricción inversa desde frame {frame} (facing={} moving={})",
                    s.facing, s.moving
                ));
            }
        } else if let Some(start) = rev_start.take() {
            rep.events.push(format!(
                "fricción inversa hasta frame {frame} ({} frames)",
                frame - start
            ));
        }

        if prev_ground && !s.on_ground {
            rep.jumps += 1;
            jump_start = Some(frame);
            if rep.first_jump_frame.is_none() {
                rep.first_jump_frame = Some(frame);
                if frame <= 10 {
                    rep.events.push(format!(
                        "primer salto en frame {frame}: ventana low-gravity"
                    ));
                }
            }
        }
        if !prev_ground && s.on_ground {
            if let Some(start) = jump_start.take() {
                let airtime = frame - start;
                if rep.first_jump_airtime.is_none() {
                    rep.first_jump_airtime = Some(airtime);
                    rep.events
                        .push(format!("primer salto: {airtime} frames en el aire"));
                }
            }
        }
        prev_ground = s.on_ground;

        if rep.flagpole_frame.is_none() && (s.state == 3 || s.flagpole_y != 0) {
            rep.flagpole_frame = Some(frame);
            rep.flagpole_y = s.flagpole_y;
            rep.flagpole_score = s.flagpole_score;
            rep.flagpole_window = s.flagpole_window();
            rep.events.push(format!(
                "flagpole en frame {frame}: Y={} score={} ventana={}",
                s.flagpole_y,
                s.flagpole_score,
                if s.flagpole_window() { "sí" } else { "no" }
            ));
        }
    }
    rep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flagpole_window_is_narrow() {
        let mut s = TrickState::default();
        s.x = 3160;
        assert!(s.flagpole_window()); // nybble 8
        s.x = 3162;
        assert!(!s.flagpole_window());
        s.x = 3161;
        assert!(s.flagpole_window()); // nybble 9
    }

    #[test]
    fn reverse_friction_detects_turn() {
        let mut s = TrickState::default();
        s.facing = 1;
        s.moving = 2;
        assert!(s.reverse_friction());
        s.moving = 1;
        assert!(!s.reverse_friction());
    }
}
