//! Modelo de física de SMB1 para planificación (no sustituye al emulador).
//!
//! Unidades reales del juego:
//! - `Player_X_Speed` ($0057): 1/16 px por frame (0x28 = 2.5 px/f).
//! - `Player_X_MoveForce` ($0705): 1/256 px por frame (fracción de velocidad).
//! - `Player_Y_Speed` ($009F): 1 px por frame (entero con signo).
//! - `Player_Y_MoveForce` ($0709/$070A): 1/256 px (fracción de gravedad).
//!
//! Gravedad (de `PlayerPhysicsSub` / `ImposeGravity`):
//! - Subiendo con A: `JumpMForceData` = 0x20/0x1E/0x28 según velocidad.
//! - Cayendo o A suelto: `FallMForceData` = 0x70/0x60/0x90.
//! - Velocidad vertical inicial: `PlayerYSpdData` = -4 (andando) / -5 (corriendo).
//!
//! El primer frame del salto usa gravedad de caída porque el juego exige
//! `A && PreviousA` para mantener la gravedad de salto (ver `JumpSwimSub`).

pub const FPS: f64 = 60.0988;
pub const MAX_RUN_SPEED: f32 = 2.5;
pub const TILE_PX: f32 = 16.0;

#[derive(Clone, Copy, Debug)]
pub struct ArcPoint {
    pub t: u32,
    /// Desplazamiento horizontal desde el despegue, en px.
    pub x: f32,
    /// Desplazamiento vertical desde el despegue, en px (negativo = arriba).
    pub y: f32,
    pub vy: i32,
}

#[derive(Clone, Debug)]
pub struct JumpArc {
    pub vx: f32,
    pub hold: u8,
    pub points: Vec<ArcPoint>,
    pub max_height: f32,
    pub airtime: u32,
    pub dx: f32,
}

impl JumpArc {
    /// Altura (positiva) del arco a una distancia horizontal dada.
    pub fn height_at_dx(&self, dx: f32) -> Option<f32> {
        self.y_at_dx(dx).map(|y| -y)
    }

    /// `y` interpolada (negativa arriba) a una distancia horizontal dada.
    pub fn y_at_dx(&self, dx: f32) -> Option<f32> {
        if dx < 0.0 {
            return None;
        }
        let mut prev = self.points.first()?;
        if dx <= prev.x {
            return Some(prev.y);
        }
        for p in self.points.iter().skip(1) {
            if p.x >= dx {
                let span = (p.x - prev.x).max(0.0001);
                let f = (dx - prev.x) / span;
                return Some(prev.y + (p.y - prev.y) * f);
            }
            prev = p;
        }
        None
    }

    /// Ventana horizontal `(x_enter, x_exit)` en la que el arco supera
    /// `height_px` de altura. Útil para aterrizar sobre una plataforma.
    pub fn window_above(&self, height_px: f32) -> Option<(f32, f32)> {
        let mut enter: Option<f32> = None;
        let mut exit: Option<f32> = None;
        for p in &self.points {
            if -p.y >= height_px {
                if enter.is_none() {
                    enter = Some(p.x);
                }
                exit = Some(p.x);
            }
        }
        match (enter, exit) {
            (Some(a), Some(b)) if b > a => Some((a, b)),
            _ => None,
        }
    }
}

fn speed_index(vx: f32) -> usize {
    let s = (vx.abs() * 16.0).round();
    if s < 16.0 {
        0
    } else if s < 25.0 {
        2
    } else {
        4
    }
}

fn jump_forces(idx: usize) -> (i32, i32, i32) {
    match idx {
        0 | 1 => (0x20, 0x70, -4),
        2 => (0x1E, 0x60, -4),
        _ => (0x28, 0x90, -5),
    }
}

/// Simula un salto con velocidad horizontal `vx` (px/f) y `hold` frames de A.
pub fn jump_arc(vx: f32, hold: u8) -> JumpArc {
    let idx = speed_index(vx);
    let (jump_force, fall_force, vy0) = jump_forces(idx);
    let hold = hold.max(1) as u32;

    let mut y_f: i32 = 0; // 1/256 px, negativo = arriba
    let mut mf: i32 = 0;
    let mut vy: i32 = vy0;
    let mut x_f: i32 = 0;
    let vx_f = (vx * 256.0).round() as i32;

    let mut points = Vec::with_capacity(96);
    let mut max_height = 0.0f32;
    let mut airtime = 0u32;
    let mut dx = 0.0f32;

    for t in 0..180u32 {
        points.push(ArcPoint {
            t,
            x: x_f as f32 / 256.0,
            y: y_f as f32 / 256.0,
            vy,
        });
        let h = -(y_f as f32) / 256.0;
        if h > max_height {
            max_height = h;
        }

        // Frame 0: A recién pulsado, PreviousA=0 -> gravedad de caída.
        let g = if t > 0 && t < hold && vy < 0 {
            jump_force
        } else {
            fall_force
        };
        mf += g;
        let carry = mf >> 8;
        mf &= 0xFF;
        vy += carry;
        if vy > 4 || (vy == 4 && mf >= 0x80) {
            vy = 4;
            mf = 0;
        }
        y_f += vy * 256;
        x_f += vx_f;

        if t > 0 && y_f >= 0 && vy > 0 {
            airtime = t;
            dx = x_f as f32 / 256.0;
            break;
        }
    }

    JumpArc {
        vx,
        hold: hold as u8,
        points,
        max_height,
        airtime,
        dx,
    }
}

/// Tiempo de caída libre (frames) para descender `dy_px`.
pub fn fall_time(dy_px: f32) -> u32 {
    if dy_px <= 0.0 {
        return 0;
    }
    let mut y_f = 0i32;
    let mut mf = 0i32;
    let mut vy = 0i32;
    for t in 0..180u32 {
        mf += 0x70;
        let carry = mf >> 8;
        mf &= 0xFF;
        vy += carry;
        if vy > 4 || (vy == 4 && mf >= 0x80) {
            vy = 4;
            mf = 0;
        }
        y_f += vy * 256;
        if y_f as f32 / 256.0 >= dy_px {
            return t + 1;
        }
    }
    180
}

/// Altura máxima teórica de un salto a velocidad de carrera.
pub fn max_jump_height() -> f32 {
    jump_arc(MAX_RUN_SPEED, 26).max_height
}

/// Anchura máxima teórica de un salto a velocidad de carrera.
pub fn max_jump_dx() -> f32 {
    jump_arc(MAX_RUN_SPEED, 26).dx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_jump_is_about_five_tiles_high() {
        let h = max_jump_height();
        assert!(h > 72.0 && h < 88.0, "height={h}");
    }

    #[test]
    fn full_jump_crosses_about_eight_tiles() {
        let dx = max_jump_dx();
        assert!(dx > 110.0 && dx < 140.0, "dx={dx}");
    }

    #[test]
    fn short_hop_is_lower_than_full_jump() {
        assert!(jump_arc(2.5, 6).max_height < jump_arc(2.5, 26).max_height);
    }
}
