//! Cartografía completa de un nivel a partir de la Block Buffer de SMB1.
//!
//! La Block Buffer (`$0500-$069F`) es una ventana circular de 32 columnas ×
//! 13 filas de metatiles. La pantalla visible son 16 columnas; el juego va
//! rellenando las columnas que entran por la derecha. `deepscan` recorre el
//! nivel una vez y fusiona cada ventana visible en un mapa absoluto.
//!
//! Limitación conocida (documentada): el mapeo `fila = (y_mundo - 32) / 16`
//! asume niveles sin scroll vertical (1-1, 1-2, 4-2, 8-4...). Para niveles
//! verticales hay que compensar con `ScreenY_Pos`/`VerticalScroll` (TODO).

use std::fmt::Write as _;

pub const ROWS: usize = 13;
pub const TILE_PX: i32 = 16;
pub const LEVEL_TOP_Y: i32 = 32;

#[derive(Clone, Debug)]
pub struct EnemySighting {
    pub kind: u8,
    pub x: i32,
    pub y: i32,
    pub frame: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObstacleKind {
    Pit,
    Wall,
}

#[derive(Clone, Debug)]
pub struct Obstacle {
    pub kind: ObstacleKind,
    pub start_col: i32,
    pub end_col: i32,
    pub top_row: i32,
    pub height_tiles: i32,
}

impl Obstacle {
    pub fn width_tiles(&self) -> i32 {
        self.end_col - self.start_col + 1
    }
}

#[derive(Clone)]
pub struct LevelMap {
    pub rom_sha256: String,
    pub cols: usize,
    pub tiles: Vec<u8>,
    pub seen: Vec<bool>,
    pub enemies: Vec<EnemySighting>,
    pub start_x: i32,
    pub flag_col: Option<usize>,
    pub min_cam_col: i32,
    pub max_cam_col: i32,
    /// Seguimiento por slot de enemigo ($000F..$0013) para registrar solo el
    /// spawn de cada activación, no cada frame en el que el enemigo se mueve.
    slot_active: [bool; 5],
    slot_kind: [u8; 5],
}

impl LevelMap {
    pub fn new(rom_sha256: &str) -> Self {
        Self {
            rom_sha256: rom_sha256.to_string(),
            cols: 0,
            tiles: Vec::new(),
            seen: Vec::new(),
            enemies: Vec::new(),
            start_x: 0,
            flag_col: None,
            min_cam_col: i32::MAX,
            max_cam_col: i32::MIN,
            slot_active: [false; 5],
            slot_kind: [0; 5],
        }
    }

    fn ensure_cols(&mut self, cols: usize) {
        if cols <= self.cols {
            return;
        }
        self.tiles.resize(cols * ROWS, 0);
        self.seen.resize(cols * ROWS, false);
        self.cols = cols;
    }

    /// Fusiona la ventana visible actual en el mapa absoluto.
    ///
    /// `frame` es el frame global (solo para las fichas de enemigos).
    pub fn ingest(&mut self, ram: &[u8; 2048], frame: u64) {
        let cam_x = (ram[0x071A] as i32) * 256 + (ram[0x071C] as i32);
        let cam_col = cam_x / TILE_PX;
        self.min_cam_col = self.min_cam_col.min(cam_col);
        self.max_cam_col = self.max_cam_col.max(cam_col);

        let player_x = (ram[0x006D] as i32) * 256 + (ram[0x0086] as i32);
        if self.start_x == 0 && player_x > 0 {
            self.start_x = player_x;
        }

        let base = cam_col.max(0) as usize;
        self.ensure_cols(base + 16);

        // Solo las 16 columnas visibles: son las que el juego garantiza que
        // están cargadas. La segunda página de la buffer puede contener datos
        // de la pantalla siguiente todavía no parseados.
        for c in 0..16 {
            let mt = cam_col + c as i32;
            if mt < 0 {
                continue;
            }
            let mt = mt as usize;
            let page = (mt / 16) & 1;
            let col = mt & 15;
            for row in 0..ROWS {
                let addr = 0x0500 + page * 208 + row * 16 + col;
                let idx = mt * ROWS + row;
                if !self.seen[idx] {
                    self.tiles[idx] = ram[addr];
                    self.seen[idx] = true;
                }
            }
        }

        // Enemigos: registrar solo el spawn de cada activación de slot.
        // Un enemigo que camina no genera una ficha por frame; cuando el slot
        // se libera (0) o cambia de tipo, la siguiente aparición es un spawn.
        for slot in 0..5usize {
            let kind = ram[0x0016 + slot];
            let active = ram[0x000F + slot] != 0 && kind != 0;
            if !active {
                self.slot_active[slot] = false;
                self.slot_kind[slot] = 0;
                continue;
            }
            let changed = !self.slot_active[slot] || self.slot_kind[slot] != kind;
            self.slot_active[slot] = true;
            self.slot_kind[slot] = kind;
            if !changed {
                continue;
            }
            let ex = (ram[0x006E + slot] as i32) * 256 + (ram[0x0087 + slot] as i32);
            let ey = ram[0x00CF + slot] as i32;
            self.enemies.push(EnemySighting {
                kind,
                x: ex,
                y: ey,
                frame,
            });
        }

        // Bandera: state==3 (trepando) o subrutina 4/5 (slide/end level).
        let state = ram[0x001D];
        let sub = ram[0x000E];
        if self.flag_col.is_none() && (state == 3 || sub == 4 || sub == 5) {
            self.flag_col = Some((player_x / TILE_PX) as usize);
        }
    }

    #[inline]
    pub fn tile(&self, col: i32, row: i32) -> u8 {
        if col < 0 || row < 0 || col as usize >= self.cols || row as usize >= ROWS {
            return 0;
        }
        let idx = col as usize * ROWS + row as usize;
        if !self.seen[idx] {
            0
        } else {
            self.tiles[idx]
        }
    }

    #[inline]
    pub fn seen_col(&self, col: i32) -> bool {
        if col < 0 || col as usize >= self.cols {
            return false;
        }
        self.seen[col as usize * ROWS]
    }

    #[inline]
    pub fn is_solid(&self, col: i32, row: i32) -> bool {
        let t = self.tile(col, row);
        t != 0 && t != 0xC2 && t != 0xC3
    }

    #[inline]
    pub fn is_coin(&self, col: i32, row: i32) -> bool {
        let t = self.tile(col, row);
        t == 0xC2 || t == 0xC3
    }

    /// Un pozo es una columna sin suelo en las dos filas inferiores.
    #[inline]
    pub fn is_pit(&self, col: i32) -> bool {
        !self.is_solid(col, 11) && !self.is_solid(col, 12)
    }

    /// Filas donde Mario puede apoyar los pies (sólido con 2 filas libres encima).
    pub fn surface_rows(&self, col: i32) -> Vec<usize> {
        let mut out = Vec::new();
        if col < 0 || col as usize >= self.cols {
            return out;
        }
        for row in 0..ROWS {
            if !self.is_solid(col, row as i32) {
                continue;
            }
            let above1 = row >= 1 && self.is_solid(col, row as i32 - 1);
            let above2 = row >= 2 && self.is_solid(col, row as i32 - 2);
            if !above1 && !above2 {
                out.push(row);
            }
        }
        out
    }

    /// Superficie de apoyo más baja (la que se usa para caminar). Para una
    /// tubería es su remate; para un bloque flotante sigue siendo el suelo.
    pub fn ground_row(&self, col: i32) -> Option<usize> {
        self.surface_rows(col).into_iter().max()
    }

    pub fn baseline_ground_row(&self) -> i32 {
        let mut hist = [0u32; ROWS];
        for col in 0..self.cols as i32 {
            if !self.seen_col(col) || self.is_pit(col) {
                continue;
            }
            if let Some(r) = self.ground_row(col) {
                hist[r] += 1;
            }
        }
        let mut best = 11usize;
        let mut best_n = 0;
        for (r, &n) in hist.iter().enumerate() {
            if n > best_n {
                best_n = n;
                best = r;
            }
        }
        best as i32
    }

    /// Una columna bloquea el paso si es un pozo, o si tiene un sólido a la
    /// altura del cuerpo de Mario (filas baseline-1/baseline-2) o una
    /// superficie más alta que el suelo base.
    pub fn is_wall(&self, col: i32, baseline: i32) -> bool {
        if self.is_pit(col) {
            return false;
        }
        if let Some(g) = self.ground_row(col) {
            if (g as i32) < baseline {
                return true;
            }
        }
        self.is_solid(col, baseline - 1) || self.is_solid(col, baseline - 2)
    }

    /// Obstáculos agrupados. Las escaleras se parten por altura para que el
    /// planificador pueda saltar escalón a escalón.
    pub fn obstacles(&self) -> Vec<Obstacle> {
        let baseline = self.baseline_ground_row();
        let mut out = Vec::new();
        let mut c = 0i32;
        while c < self.cols as i32 {
            if !self.seen_col(c) {
                c += 1;
                continue;
            }
            let pit = self.is_pit(c);
            let wall = !pit && self.is_wall(c, baseline);
            if !pit && !wall {
                c += 1;
                continue;
            }
            let top = self.ground_row(c).unwrap_or(baseline as usize) as i32;
            let start = c;
            let mut end = c;
            let mut c2 = c + 1;
            while c2 < self.cols as i32 && self.seen_col(c2) {
                let p2 = self.is_pit(c2);
                let w2 = !p2 && self.is_wall(c2, baseline);
                let top2 = self.ground_row(c2).unwrap_or(baseline as usize) as i32;
                if p2 == pit && w2 == wall && top2 == top {
                    end = c2;
                    c2 += 1;
                } else {
                    break;
                }
            }
            out.push(Obstacle {
                kind: if pit {
                    ObstacleKind::Pit
                } else {
                    ObstacleKind::Wall
                },
                start_col: start,
                end_col: end,
                top_row: top,
                height_tiles: baseline - top,
            });
            c = end + 1;
        }
        out
    }

    pub fn goal_col(&self) -> i32 {
        self.flag_col
            .map(|c| c as i32)
            .unwrap_or_else(|| (self.cols as i32 - 1).max(0))
    }

    pub fn to_json(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "{{");
        let _ = writeln!(s, "  \"rom_sha256\": \"{}\",", self.rom_sha256);
        let _ = writeln!(s, "  \"level\": \"1-1\",");
        let _ = writeln!(s, "  \"cols\": {},", self.cols);
        let _ = writeln!(s, "  \"rows\": {},", ROWS);
        let _ = writeln!(s, "  \"tile_px\": {},", TILE_PX);
        let _ = writeln!(s, "  \"baseline_ground_row\": {},", self.baseline_ground_row());
        let _ = writeln!(s, "  \"start_x\": {},", self.start_x);
        let _ = writeln!(s, "  \"flag_col\": {},", self.flag_col.map(|c| c as i64).unwrap_or(-1));
        let _ = writeln!(
            s,
            "  \"camera_cols\": [{}, {}],",
            self.min_cam_col, self.max_cam_col
        );

        let _ = write!(s, "  \"tiles\": [");
        for col in 0..self.cols {
            if col > 0 {
                s.push(',');
            }
            s.push('[');
            for row in 0..ROWS {
                if row > 0 {
                    s.push(',');
                }
                let _ = write!(s, "{}", self.tile(col as i32, row as i32));
            }
            s.push(']');
        }
        let _ = writeln!(s, "],");

        let _ = write!(s, "  \"enemies\": [");
        for (i, e) in self.enemies.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let _ = write!(
                s,
                "{{\"kind\":{},\"x\":{},\"y\":{},\"frame\":{}}}",
                e.kind, e.x, e.y, e.frame
            );
        }
        let _ = writeln!(s, "],");

        let obs = self.obstacles();
        let _ = write!(s, "  \"obstacles\": [");
        for (i, o) in obs.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            let kind = match o.kind {
                ObstacleKind::Pit => "pit",
                ObstacleKind::Wall => "wall",
            };
            let _ = write!(
                s,
                "{{\"kind\":\"{}\",\"start_col\":{},\"end_col\":{},\"top_row\":{},\"height_tiles\":{}}}",
                kind, o.start_col, o.end_col, o.top_row, o.height_tiles
            );
        }
        let _ = writeln!(s, "]");
        let _ = writeln!(s, "}}");
        s
    }

    pub fn to_ascii(&self) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "LevelMap 1-1  cols={}  start_x={}  flag_col={:?}", self.cols, self.start_x, self.flag_col);
        let _ = writeln!(s, "baseline_ground_row={}  enemies={}  obstacles={}", self.baseline_ground_row(), self.enemies.len(), self.obstacles().len());
        // Regla de decenas.
        let mut ruler = String::from("     ");
        for c in 0..self.cols {
            if c % 10 == 0 {
                let _ = write!(ruler, "{}", (c / 10) % 10);
            } else {
                ruler.push(' ');
            }
        }
        let _ = writeln!(s, "{}", ruler);
        for row in 0..ROWS {
            let _ = write!(s, "{:2} | ", row);
            for col in 0..self.cols {
                let ch = if self.is_coin(col as i32, row as i32) {
                    'o'
                } else if self.is_solid(col as i32, row as i32) {
                    '#'
                } else {
                    '.'
                };
                s.push(ch);
            }
            s.push('\n');
        }
        s
    }

    /// SVG autocontenido: mapa + enemigos + bandera + (opcional) ruta.
    pub fn to_svg(&self, route: Option<&[RouteMarker]>) -> String {
        let scale = 6i32;
        let pad = 12i32;
        let w = self.cols as i32 * scale + pad * 2;
        let h = ROWS as i32 * scale + pad * 2 + 18;
        let baseline = self.baseline_ground_row();
        let mut s = String::new();
        let _ = writeln!(
            s,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">"#
        );
        let _ = writeln!(s, r##"<rect width="100%" height="100%" fill="#0e0f14"/>"##);
        for col in 0..self.cols {
            for row in 0..ROWS {
                if !self.is_solid(col as i32, row as i32) && !self.is_coin(col as i32, row as i32) {
                    continue;
                }
                let color = if self.is_coin(col as i32, row as i32) {
                    "#f2c94c"
                } else if row as i32 >= baseline {
                    "#7a5a32"
                } else if self.is_wall(col as i32, baseline) {
                    "#3fa34d"
                } else {
                    "#9aa0a6"
                };
                let x = pad + col as i32 * scale;
                let y = pad + row as i32 * scale;
                let _ = writeln!(
                    s,
                    r#"<rect x="{x}" y="{y}" width="{scale}" height="{scale}" fill="{color}"/>"#
                );
            }
        }
        for e in &self.enemies {
            let x = pad + (e.x / TILE_PX) * scale;
            let y = pad + ((e.y - LEVEL_TOP_Y) / TILE_PX) * scale;
            let _ = writeln!(
                s,
                r##"<circle cx="{x}" cy="{y}" r="2.2" fill="#eb5757" opacity="0.9"/>"##
            );
        }
        if let Some(fc) = self.flag_col {
            let x = pad + fc as i32 * scale + scale / 2;
            let _ = writeln!(
                s,
                r##"<line x1="{x}" y1="{pad}" x2="{x}" y2="{}" stroke="#f2c94c" stroke-width="2"/>"##,
                pad + ROWS as i32 * scale
            );
        }
        if self.start_x > 0 {
            let x = pad + (self.start_x / TILE_PX) * scale;
            let y = pad + baseline * scale - 3;
            let _ = writeln!(s, r##"<circle cx="{x}" cy="{y}" r="3" fill="#56ccf2"/>"##);
        }
        if let Some(markers) = route {
            for m in markers {
                let x = pad + m.col * scale + scale / 2;
                let y0 = pad + baseline * scale;
                let color = if m.jump { "#bb6bd9" } else { "#56ccf2" };
                let _ = writeln!(
                    s,
                    r#"<line x1="{x}" y1="{y0}" x2="{x}" y2="{}" stroke="{color}" stroke-width="1" opacity="0.7"/>"#,
                    y0 - 10
                );
                if m.jump {
                    let _ = writeln!(
                        s,
                        r##"<text x="{x}" y="{}" fill="#bb6bd9" font-size="6" text-anchor="middle">J{}</text>"##,
                        y0 - 12,
                        m.hold
                    );
                }
            }
        }
        let _ = writeln!(
            s,
            r##"<text x="{pad}" y="{}" fill="#e8e8e8" font-family="monospace" font-size="9">SMB1 1-1 · deepscan · {} cols · {} enemies</text>"##,
            h - 5,
            self.cols,
            self.enemies.len()
        );
        let _ = writeln!(s, "</svg>");
        s
    }
}

/// Marcador de ruta para el SVG (columna, si es salto y hold de A).
#[derive(Clone, Copy, Debug)]
pub struct RouteMarker {
    pub col: i32,
    pub jump: bool,
    pub hold: u8,
}
