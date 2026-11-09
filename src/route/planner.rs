//! Planificador heurístico de ruta sobre el mapa de metatiles.
//!
//! No es un A* exhaustivo todavía: descompone el nivel en obstáculos
//! (tuberías, escalones, pozos) y elige para cada uno el salto más corto que
//! lo supera, evaluando holds de A de 6 a 26 frames y distintos puntos de
//! despegue. La búsqueda con rollouts (`beam.rs`) valida y mejora el plan.
//!
//! Punto de extensión natural: sustituir `plan_route` por un A* sobre estados
//! `(columna, fila de apoyo, velocidad)` usando `physics::jump_arc`.

use crate::route::map::{LevelMap, ObstacleKind, RouteMarker};
use crate::route::physics::{jump_arc, MAX_RUN_SPEED, TILE_PX};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManeuverKind {
    Run,
    Jump,
}

#[derive(Clone, Debug)]
pub struct Maneuver {
    pub kind: ManeuverKind,
    pub from_col: i32,
    pub to_col: i32,
    pub takeoff_col: i32,
    pub land_col: i32,
    pub hold: u8,
    pub airtime: u32,
    pub note: String,
}

#[derive(Clone, Debug)]
pub struct RoutePlan {
    pub maneuvers: Vec<Maneuver>,
    pub estimated_frames: f64,
    pub goal_col: i32,
    pub notes: Vec<String>,
}

impl RoutePlan {
    pub fn markers(&self) -> Vec<RouteMarker> {
        let mut out = Vec::new();
        for m in &self.maneuvers {
            match m.kind {
                ManeuverKind::Run => out.push(RouteMarker {
                    col: m.to_col,
                    jump: false,
                    hold: 0,
                }),
                ManeuverKind::Jump => out.push(RouteMarker {
                    col: m.takeoff_col,
                    jump: true,
                    hold: m.hold,
                }),
            }
        }
        out
    }

    pub fn jumps(&self) -> usize {
        self.maneuvers
            .iter()
            .filter(|m| m.kind == ManeuverKind::Jump)
            .count()
    }
}

struct Candidate {
    takeoff_col: i32,
    land_col: i32,
    hold: u8,
    airtime: u32,
    time: f64,
}

/// ¿El arco pasa por encima del obstáculo y aterriza detrás?
fn try_jump(
    map: &LevelMap,
    baseline: i32,
    cursor_col: i32,
    obs_start: i32,
    obs_end: i32,
    obs_top_row: i32,
    obs_kind: ObstacleKind,
    hold: u8,
    back: i32,
) -> Option<Candidate> {
    let takeoff_col = obs_start - 1 - back;
    if takeoff_col <= cursor_col + 1 {
        return None;
    }
    if !map.seen_col(takeoff_col) || map.is_pit(takeoff_col) || map.is_wall(takeoff_col, baseline) {
        return None;
    }
    let takeoff_y = 32 + baseline * 16;
    let top_y = 32 + obs_top_row * 16;
    let relative_top = (top_y - takeoff_y) as f32; // negativo si el obstáculo sube
    let arc = jump_arc(MAX_RUN_SPEED, hold);

    // 1. Altura sobre el obstáculo (solo muros; en un pozo no hay suelo).
    if obs_kind == ObstacleKind::Wall {
        for c in (takeoff_col + 1)..=obs_end {
            let dx = ((c - takeoff_col) * 16) as f32 + 8.0;
            let y = arc.y_at_dx(dx)?;
            if y > relative_top - 4.0 {
                return None;
            }
        }
    }

    // 2. Buscar el primer aterrizaje válido detrás del obstáculo.
    let max_land = takeoff_col + (arc.dx / TILE_PX) as i32 + 2;
    let mut land = None;
    for c in (obs_end + 1)..=max_land {
        if !map.seen_col(c) || map.is_pit(c) || map.is_wall(c, baseline) {
            continue;
        }
        let Some(g) = map.ground_row(c) else { continue };
        let surface_y = (32 + g * 16) as f32;
        let dx = ((c - takeoff_col) * 16) as f32 + 8.0;
        let Some(y) = arc.y_at_dx(dx) else { continue };
        let feet_y = takeoff_y as f32 + y;
        if feet_y < surface_y - 4.0 {
            continue; // todavía va por encima del suelo
        }
        let descending = arc
            .y_at_dx(dx + 8.0)
            .map(|y2| y2 > y)
            .unwrap_or(false);
        if !descending {
            continue;
        }
        land = Some(c);
        break;
    }
    let land_col = land?;

    // 3. Colisiones con sólidos antes del aterrizaje (cuerpo de Mario = 16 px).
    let landing_x = ((land_col - takeoff_col) * 16) as f32;
    for p in &arc.points {
        if p.x < 8.0 || p.x > landing_x - 6.0 {
            continue;
        }
        let wx = takeoff_col * 16 + p.x as i32;
        let wy = takeoff_y + p.y as i32;
        let col = wx.div_euclid(16);
        let row_feet = (wy - 32).div_euclid(16);
        let row_head = (wy - 16 - 32).div_euclid(16);
        if map.is_solid(col, row_feet) || map.is_solid(col, row_head) {
            return None;
        }
    }

    let time = ((takeoff_col - cursor_col) as f64 * 16.0) / MAX_RUN_SPEED as f64
        + arc.airtime as f64;
    Some(Candidate {
        takeoff_col,
        land_col,
        hold,
        airtime: arc.airtime,
        time,
    })
}

pub fn plan_route(map: &LevelMap) -> RoutePlan {
    let baseline = map.baseline_ground_row();
    let goal_col = map.goal_col();
    let obstacles = map.obstacles();
    let mut maneuvers = Vec::new();
    let mut notes = Vec::new();
    let mut cursor = (map.start_x / 16).max(0);
    let mut estimated = 0.0f64;

    for obs in &obstacles {
        // La bandera (y la columnas del castillo posteriores) no son
        // obstáculos: el plan termina al tocarla.
        if obs.start_col >= goal_col {
            break;
        }
        if obs.end_col <= cursor {
            continue;
        }
        if obs.start_col < cursor {
            continue;
        }
        let mut best: Option<Candidate> = None;
        for &hold in &[26u8, 22, 18, 14, 10, 6] {
            for back in 0..=8 {
                if let Some(c) = try_jump(
                    map,
                    baseline,
                    cursor,
                    obs.start_col,
                    obs.end_col,
                    obs.top_row,
                    obs.kind,
                    hold,
                    back,
                ) {
                    if best.as_ref().map_or(true, |b| c.time < b.time) {
                        best = Some(c);
                    }
                }
            }
        }

        let cand = match best {
            Some(c) => c,
            None => {
                // Fallback conservador: salto completo desde 2 columnas antes.
                let takeoff_col = (obs.start_col - 2).max(cursor + 1);
                let arc = jump_arc(MAX_RUN_SPEED, 26);
                let land_col = obs.end_col + 2;
                notes.push(format!(
                    "Obstáculo {}..{} sin salto validado; fallback full-jump desde {}",
                    obs.start_col, obs.end_col, takeoff_col
                ));
                Candidate {
                    takeoff_col,
                    land_col,
                    hold: 26,
                    airtime: arc.airtime,
                    time: ((takeoff_col - cursor) as f64 * 16.0) / MAX_RUN_SPEED as f64
                        + arc.airtime as f64,
                }
            }
        };

        let run_cols = (cand.takeoff_col - cursor).max(0);
        maneuvers.push(Maneuver {
            kind: ManeuverKind::Run,
            from_col: cursor,
            to_col: cand.takeoff_col,
            takeoff_col: cand.takeoff_col,
            land_col: cand.takeoff_col,
            hold: 0,
            airtime: 0,
            note: format!("run {run_cols} cols"),
        });
        maneuvers.push(Maneuver {
            kind: ManeuverKind::Jump,
            from_col: cand.takeoff_col,
            to_col: cand.land_col,
            takeoff_col: cand.takeoff_col,
            land_col: cand.land_col,
            hold: cand.hold,
            airtime: cand.airtime,
            note: format!(
                "{} {}..{} top_row={} hold={} airtime={}f",
                if obs.kind == ObstacleKind::Pit { "pit" } else { "wall" },
                obs.start_col,
                obs.end_col,
                obs.top_row,
                cand.hold,
                cand.airtime
            ),
        });
        estimated += cand.time;
        cursor = cand.land_col;
    }

    if goal_col > cursor {
        // Una columna extra: el disparo de la bandera salta cuando Mario
        // entra en la columna de la pértiga, no al llegar a su borde.
        let finish_col = (goal_col + 1).min((map.cols as i32 - 1).max(goal_col));
        let frames = ((finish_col - cursor) as f64 * 16.0) / MAX_RUN_SPEED as f64;
        maneuvers.push(Maneuver {
            kind: ManeuverKind::Run,
            from_col: cursor,
            to_col: finish_col,
            takeoff_col: finish_col,
            land_col: finish_col,
            hold: 0,
            airtime: 0,
            note: "run to flag".to_string(),
        });
        estimated += frames;
    }

    notes.push(format!(
        "{} obstáculos, {} saltos, {} maniobras",
        obstacles.len(),
        maneuvers
            .iter()
            .filter(|m| m.kind == ManeuverKind::Jump)
            .count(),
        maneuvers.len()
    ));

    RoutePlan {
        maneuvers,
        estimated_frames: estimated,
        goal_col,
        notes,
    }
}

pub fn plan_to_json(plan: &RoutePlan) -> String {
    let mut s = String::new();
    s.push_str("{\n");
    s.push_str(&format!("  \"goal_col\": {},\n", plan.goal_col));
    s.push_str(&format!(
        "  \"estimated_frames\": {:.1},\n",
        plan.estimated_frames
    ));
    s.push_str("  \"maneuvers\": [\n");
    for (i, m) in plan.maneuvers.iter().enumerate() {
        let kind = match m.kind {
            ManeuverKind::Run => "run",
            ManeuverKind::Jump => "jump",
        };
        s.push_str(&format!(
            "    {{\"kind\":\"{}\",\"from_col\":{},\"to_col\":{},\"takeoff_col\":{},\"land_col\":{},\"hold\":{},\"airtime\":{},\"note\":\"{}\"}}{}\n",
            kind,
            m.from_col,
            m.to_col,
            m.takeoff_col,
            m.land_col,
            m.hold,
            m.airtime,
            m.note.replace('"', "'"),
            if i + 1 == plan.maneuvers.len() { "" } else { "," }
        ));
    }
    s.push_str("  ],\n  \"notes\": [\n");
    for (i, n) in plan.notes.iter().enumerate() {
        s.push_str(&format!(
            "    \"{}\"{}\n",
            n.replace('"', "'"),
            if i + 1 == plan.notes.len() { "" } else { "," }
        ));
    }
    s.push_str("  ]\n}\n");
    s
}
