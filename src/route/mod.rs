//! Escáner profundo de rutas para SMB1.
//!
//! Capas:
//! 1. `map`     — cartografía de metatiles desde la Block Buffer (`$0500`).
//! 2. `physics` — modelo de salto/gravedad con las constantes de la ROM.
//! 3. `planner` — plan heurístico de saltos sobre los obstáculos del mapa.
//! 4. `beam`    — búsqueda en haz con rollouts reales del emulador.
//! 5. `pilot`   — controlador base que recorre el nivel para cartografiarlo.
//!
//! El punto de entrada de usuario es el binario `deepscan`
//! (`src/bin/deepscan.rs`).

pub mod beam;
pub mod hash;
pub mod map;
pub mod physics;
pub mod pilot;
pub mod planner;
pub mod sensors;

pub use beam::{
    beam_search, boot_to_level, inputs_to_script, BeamConfig, BeamResult, BeamTrace,
};
pub use map::{EnemySighting, LevelMap, Obstacle, ObstacleKind, RouteMarker};
pub use physics::{fall_time, jump_arc, JumpArc};
pub use pilot::Pilot;
pub use planner::{plan_route, plan_to_json, Maneuver, ManeuverKind, RoutePlan};
pub use sensors::{is_pit, is_solid, scan_environment, world_tile_kind, EnvScan};
