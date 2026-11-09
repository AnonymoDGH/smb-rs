//! Sensores de entorno compartidos por `playbot`, el piloto base y `deepscan`.
//!
//! Todas las consultas se hacen contra la Block Buffer real de SMB1
//! (`$0500-$069F`), no contra la VRAM ni contra los sprites de OAM. Eso evita
//! falsos positivos con nubes/arbustos (que se dibujan pero no son sólidos).

/// Tipo de metatile según la Block Buffer: 0 aire, 1 sólido, 2 moneda.
#[inline]
pub fn world_tile_kind(ram: &[u8; 2048], wx: i32, wy: i32) -> u8 {
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
pub fn is_solid(ram: &[u8; 2048], wx: i32, wy: i32) -> bool {
    world_tile_kind(ram, wx, wy) == 1
}

#[inline]
pub fn is_pit(ram: &[u8; 2048], wx: i32) -> bool {
    !is_solid(ram, wx, 208) && !is_solid(ram, wx, 224)
}

/// Lectura del entorno inmediato de Mario usada por el piloto balístico.
pub struct EnvScan {
    pub wall_dist: i32,
    pub wall_height: i32,
    pub wall_top_y: i32,
    pub pit_dist: i32,
    pub pit_width: i32,
    pub enemy_dist: i32,
    pub enemy_height: i32,
    pub low_ceiling: bool,
}

pub fn scan_environment(ram: &[u8; 2048], x: i32, feet: i32) -> EnvScan {
    let front_x = x + 12;
    let row_low = (feet - 6).clamp(32, 236);
    let row_mid = (feet - 20).clamp(32, 236);

    // 1. Muro/tubería/escalón más cercano y su altura real.
    let mut wall_dist = 999i32;
    let mut wall_top_y = feet;
    for d in 0..=72 {
        let wx = front_x + d;
        if is_solid(ram, wx, row_low) || is_solid(ram, wx, row_mid) {
            wall_dist = d;
            let mut ty = row_low;
            while ty >= 48 && is_solid(ram, wx, ty) {
                wall_top_y = ty;
                ty -= 16;
            }
            break;
        }
    }
    let wall_height = (feet - wall_top_y).max(0);

    // 2. Pozo más cercano y su anchura.
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

    // 3. Enemigos activos en los 5 slots ($000F..$0013).
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

    // 4. Techo bajo: bloques '?' o ladrillos justo encima de la cabeza.
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
