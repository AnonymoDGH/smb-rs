//! 2C02 PPU: loopy scrolling, background + sprite rendering, sprite-0 hit, NMI.

use crate::cartridge::{Cartridge, Mirroring};

pub const WIDTH: usize = 256;
pub const HEIGHT: usize = 240;

/// 2C02 master palette (FCEUX/NESdev reference values, RGB).
pub const NES_PALETTE: [u32; 64] = [
    0x7C7C7C, 0x0000FC, 0x0000BC, 0x4428BC, 0x940084, 0xA80020, 0xA81000, 0x881400,
    0x503000, 0x007800, 0x006800, 0x005800, 0x004058, 0x000000, 0x000000, 0x000000,
    0xBCBCBC, 0x0078F8, 0x0058F8, 0x6844FC, 0xD800CC, 0xE40058, 0xF83800, 0xE45C10,
    0xAC7C00, 0x00B800, 0x00A800, 0x00A844, 0x008888, 0x000000, 0x000000, 0x000000,
    0xF8F8F8, 0x3CBCFC, 0x6888FC, 0x9878F8, 0xF878F8, 0xF85898, 0xF87858, 0xFCA044,
    0xF8B800, 0xB8F818, 0x58D854, 0x58F898, 0x00E8D8, 0x787878, 0x000000, 0x000000,
    0xFCFCFC, 0xA4E4FC, 0xB8B8F8, 0xD8B8F8, 0xF8B8F8, 0xF8A4C0, 0xF0D0B0, 0xFCE0A8,
    0xF8D878, 0xD8F878, 0xB8F8B8, 0x58D854, 0x00FCFC, 0xF8D8F8, 0x000000, 0x000000,
];

pub struct Ppu {
    pub vram: [u8; 4096],
    pub palette: [u8; 32],
    pub oam: [u8; 256],
    pub oam_addr: u8,
    pub ctrl: u8,
    pub mask: u8,
    pub status: u8,
    pub v: u16,
    pub t: u16,
    pub fine_x: u8,
    pub w: bool,
    pub read_buffer: u8,
    pub scanline: u16,
    pub dot: u16,
    pub frame_ready: bool,
    pub nmi: bool,
    pub frame_count: u64,
    pub sprite0_hit_at: i32,
    pub frame: Vec<u32>,
    /// Background pixel indices for the scanline being scanned.
    line_bg: [u8; WIDTH],
    /// Pixels where OAM #0 is opaque, so sprite-0 hit is flagged on the right dot.
    sprite0_mask: [bool; WIDTH],
    mirroring: Mirroring,
}

impl Ppu {
    pub fn new(mirroring: Mirroring) -> Ppu {
        Ppu {
            vram: [0; 4096],
            palette: [0; 32],
            oam: [0; 256],
            oam_addr: 0,
            ctrl: 0,
            mask: 0,
            status: 0,
            v: 0,
            t: 0,
            fine_x: 0,
            w: false,
            read_buffer: 0,
            scanline: 0,
            dot: 0,
            frame_ready: false,
            nmi: false,
            frame_count: 0,
            sprite0_hit_at: -1,
            frame: vec![0xFF000000; WIDTH * HEIGHT],
            line_bg: [0; WIDTH],
            sprite0_mask: [false; WIDTH],
            mirroring,
        }
    }

    #[inline]
    fn rendering(&self) -> bool {
        self.mask & 0x18 != 0
    }

    fn nt_index(&self, nt: u16) -> usize {
        let table = match self.mirroring {
            Mirroring::Vertical => nt & 1,
            Mirroring::Horizontal => (nt >> 1) & 1,
            Mirroring::FourScreen => nt & 3,
        };
        (table as usize) * 0x400
    }

    pub fn read_vram(&self, addr: u16, cart: &Cartridge) -> u8 {
        let a = addr & 0x3FFF;
        match a {
            // Pattern tables live in the cartridge. SMB stores its title-screen
            // data at the tail of CHR-ROM and streams it out through $2007.
            0x0000..=0x1FFF => cart.ppu_read(a),
            0x2000..=0x3EFF => {
                let off = (a - 0x2000) & 0x0FFF;
                self.vram[self.nt_index(off >> 10) + (off as usize & 0x3FF)]
            }
            _ => {
                let mut i = (a - 0x3F00) as usize & 0x1F;
                if i & 0x13 == 0x10 {
                    i &= 0x0F; // $3F10/$14/$18/$1C mirror $3F00/$04/$08/$0C
                }
                self.palette[i]
            }
        }
    }

    pub fn write_vram(&mut self, addr: u16, val: u8, cart: &mut Cartridge) {
        let a = addr & 0x3FFF;
        match a {
            0x0000..=0x1FFF => cart.ppu_write(a, val),
            0x2000..=0x3EFF => {
                let off = (a - 0x2000) & 0x0FFF;
                let i = self.nt_index(off >> 10) + (off as usize & 0x3FF);
                self.vram[i] = val;
            }
            _ => {
                let mut i = (a - 0x3F00) as usize & 0x1F;
                if i & 0x13 == 0x10 {
                    i &= 0x0F;
                }
                self.palette[i] = val & 0x3F;
            }
        }
    }

    fn increment_y(&mut self) {
        if (self.v & 0x7000) != 0x7000 {
            self.v += 0x1000;
        } else {
            self.v &= !0x7000;
            let mut y = (self.v >> 5) & 0x1F;
            if y == 29 {
                y = 0;
                self.v ^= 0x0800;
            } else if y == 31 {
                y = 0;
            } else {
                y += 1;
            }
            self.v = (self.v & !0x03E0) | (y << 5);
        }
    }

    fn copy_x(&mut self) {
        self.v = (self.v & !0x041F) | (self.t & 0x041F);
    }

    pub fn cpu_read(&mut self, reg: u16, cart: &Cartridge) -> u8 {
        match reg & 7 {
            2 => {
                let mut r = self.status & 0xE0;
                if self.scanline == 241 && self.dot == 0 {
                    r |= 0x80;
                }
                self.status &= !0x80;
                self.w = false;
                // open bus bits 0-4 come from the last written latch; SMB does not rely on it.
                r | (self.status & 0x1F)
            }
            4 => {
                let mut v = self.oam[self.oam_addr as usize];
                // Reading $2004 during rendering corrupts the low bits on real hardware.
                if self.rendering() && self.scanline < 240 {
                    v = v & 0xE0 | 0x1F;
                }
                v
            }
            7 => {
                let a = self.v & 0x3FFF;
                let result = if a >= 0x3F00 {
                    // Palette reads return immediately, but the read buffer is
                    // filled from the nametable the address mirrors into.
                    self.read_buffer = self.read_vram(a - 0x1000, cart);
                    self.read_vram(a, cart)
                } else {
                    let buffered = self.read_buffer;
                    self.read_buffer = self.read_vram(a, cart);
                    buffered
                };
                self.v = self.v.wrapping_add(if self.ctrl & 0x04 != 0 { 32 } else { 1 }) & 0x7FFF;
                result
            }
            _ => 0,
        }
    }

    pub fn cpu_write(&mut self, reg: u16, val: u8, cart: &mut Cartridge) {
        match reg & 7 {
            0 => {
                // Raising NMI enable while the vblank flag is already set
                // triggers an NMI immediately (SMB relies on this).
                let was_enabled = self.ctrl & 0x80 != 0;
                self.ctrl = val;
                self.t = (self.t & !0x0C00) | (((val as u16) & 3) << 10);
                if !was_enabled && self.ctrl & 0x80 != 0 && self.status & 0x80 != 0 {
                    self.nmi = true;
                }
            }
            1 => self.mask = val,
            3 => self.oam_addr = val,
            4 => {
                self.oam[self.oam_addr as usize] = val;
                self.oam_addr = self.oam_addr.wrapping_add(1);
            }
            5 => {
                if !self.w {
                    self.t = (self.t & !0x001F) | ((val as u16) >> 3);
                    self.fine_x = val & 7;
                    self.w = true;
                } else {
                    self.t = (self.t & !0x73E0)
                        | (((val as u16) & 0x07) << 12)
                        | (((val as u16) & 0xF8) << 2);
                    self.w = false;
                }
            }
            6 => {
                if !self.w {
                    self.t = (self.t & 0x00FF) | (((val as u16) & 0x3F) << 8);
                    self.w = true;
                } else {
                    self.t = (self.t & 0xFF00) | val as u16;
                    self.v = self.t;
                    self.w = false;
                }
            }
            7 => {
                let a = self.v & 0x3FFF;
                self.write_vram(a, val, cart);
                self.v = self.v.wrapping_add(if self.ctrl & 0x04 != 0 { 32 } else { 1 }) & 0x7FFF;
            }
            _ => {}
        }
    }

    /// Advance the PPU by a number of dots (3 per CPU cycle).
    pub fn step(&mut self, dots: u32, cart: &Cartridge) {
        for _ in 0..dots {
            self.tick(cart);
            if self.frame_ready {
                self.frame_count += 1;
                return;
            }
        }
    }

    fn tick(&mut self, cart: &Cartridge) {
        if self.scanline == 241 && self.dot == 1 {
            self.status |= 0x80;
            if self.ctrl & 0x80 != 0 {
                self.nmi = true;
            }
        }
        if self.scanline == 261 && self.dot == 1 {
            self.status &= !0xE0;
            self.nmi = false;
        }
        if self.scanline == 261 && self.dot == 280 && self.rendering() {
            self.v = (self.v & !0x7BE0) | (self.t & 0x7BE0);
        }
        if self.scanline < 240 {
            // The loopy registers only move at the end of the line, so the whole
            // line can be built at dot 0; the sprite-0 hit itself must be reported
            // on the dot that scans the colliding pixel, not at the end of the line.
            if self.dot == 0 {
                self.render_scanline(cart);
            } else if self.dot <= 256 && self.status & 0x40 == 0 && self.rendering() {
                let x = (self.dot - 1) as usize;
                if self.sprite0_mask[x] && self.line_bg[x] != 0 {
                    self.status |= 0x40;
                    if self.sprite0_hit_at < 0 {
                        self.sprite0_hit_at = self.scanline as i32;
                    }
                }
            }
            if self.dot == 256 && self.rendering() {
                self.increment_y();
            }
        }
        if (self.scanline < 240 || self.scanline == 261) && self.dot == 257 && self.rendering() {
            self.copy_x();
        }

        self.dot += 1;
        if self.dot >= 341 {
            self.dot = 0;
            self.scanline += 1;
            if self.scanline > 261 {
                self.scanline = 0;
                self.frame_ready = true;
            }
        }
    }

    fn render_scanline(&mut self, cart: &Cartridge) {
        let y = self.scanline as usize;
        if y >= HEIGHT {
            return;
        }
        let backdrop = (self.palette[0] & 0x3F) as u32;
        let mut bg = [0u8; WIDTH];
        let mut sprite0 = [false; WIDTH];
        let mut line = [NES_PALETTE[backdrop as usize]; WIDTH];

        if self.rendering() {
            let bg_base: u16 = if self.ctrl & 0x10 != 0 { 0x1000 } else { 0 };
            let fine_y = ((self.v >> 12) & 7) as u16;
            let coarse_y = (self.v >> 5) & 0x1F;
            let nt_y = (self.v >> 11) & 1;
            let nt_x = (self.v >> 10) & 1;
            let coarse_x = self.v & 0x1F;
            let fine_x = self.fine_x as u16;
            let show_bg = self.mask & 0x08 != 0;
            let show_left_bg = self.mask & 0x02 != 0;

            for tx in 0..33u16 {
                let abs_col = coarse_x + tx;
                let col = abs_col & 31;
                let nt = nt_x ^ ((abs_col >> 5) & 1);
                let table = self.nt_index(nt | (nt_y << 1));
                let tile = self.vram[table + (coarse_y as usize) * 32 + col as usize] as u16;
                let attr_addr = table + 0x3C0 + ((coarse_y as usize) >> 2) * 8 + ((col as usize) >> 2);
                let attr = self.vram[attr_addr];
                let shift = ((coarse_y & 2) << 1) | (col & 2);
                let pal = (attr >> shift) & 3;
                let pattern = bg_base + tile * 16 + fine_y;
                let lo = cart.ppu_read(pattern);
                let hi = cart.ppu_read(pattern + 8);

                let x0 = (tx * 8) as i32 - fine_x as i32;
                for bit in 0..8u16 {
                    let x = x0 + bit as i32;
                    if x < 0 || x >= WIDTH as i32 {
                        continue;
                    }
                    if !show_bg || (!show_left_bg && x < 8) {
                        continue;
                    }
                    let b = 7 - bit;
                    let px = ((lo >> b) & 1) | (((hi >> b) & 1) << 1);
                    bg[x as usize] = px;
                    if px != 0 {
                        let pi = (self.palette[(pal as usize) * 4 + px as usize] & 0x3F) as usize;
                        line[x as usize] = NES_PALETTE[pi];
                    }
                }
            }

            // ---- sprites ----
            if self.mask & 0x10 != 0 {
                let sprite_base: u16 = if self.ctrl & 0x08 != 0 { 0x1000 } else { 0 };
                let mut selected: [(usize, i32, u8, u8); 8] = [(0, 0, 0, 0); 8];
                let mut count = 0usize;
                for i in 0..64 {
                    let sy = self.oam[i * 4] as i32 + 1;
                    let row = y as i32 - sy;
                    if row < 0 || row > 7 {
                        continue;
                    }
                    if count == 8 {
                        self.status |= 0x20;
                        break;
                    }
                    selected[count] = (
                        i,
                        self.oam[i * 4 + 3] as i32,
                        self.oam[i * 4 + 1],
                        self.oam[i * 4 + 2],
                    );
                    count += 1;
                }
                // Lower OAM index has priority: draw from the end backwards.
                for s in (0..count).rev() {
                    let (idx, sx, tile, attr) = selected[s];
                    if sx <= -8 || sx >= 256 {
                        continue;
                    }
                    let row = (y as i32 - (self.oam[idx * 4] as i32 + 1)) as u16;
                    let row = if attr & 0x80 != 0 { 7 - row } else { row };
                    let pattern = sprite_base + (tile as u16) * 16 + row;
                    let mut lo = cart.ppu_read(pattern);
                    let mut hi = cart.ppu_read(pattern + 8);
                    if attr & 0x40 != 0 {
                        lo = flip_byte(lo);
                        hi = flip_byte(hi);
                    }
                    for bit in 0..8i32 {
                        let x = sx + bit;
                        if x < 0 || x >= WIDTH as i32 {
                            continue;
                        }
                        if self.mask & 0x04 == 0 && x < 8 {
                            continue;
                        }
                        let b = 7 - bit as u8;
                        let px = ((lo >> b) & 1) | (((hi >> b) & 1) << 1);
                        if px == 0 {
                            continue;
                        }
                        // Sprite-0 hit needs an opaque sprite pixel over an
                        // opaque background pixel; x=255 can never flag it.
                        if idx == 0 && x != 255 {
                            sprite0[x as usize] = true;
                        }
                        if bg[x as usize] != 0 && attr & 0x20 != 0 {
                            continue; // sprite behind background
                        }
                        let pi = (self.palette[0x10 + ((attr as usize & 3) * 4) + px as usize] & 0x3F)
                            as usize;
                        line[x as usize] = NES_PALETTE[pi];
                    }
                }
            }
        }

        let row = y * WIDTH;
        self.frame[row..row + WIDTH].copy_from_slice(&line);
        self.line_bg = bg;
        self.sprite0_mask = sprite0;
    }
}

#[inline]
fn flip_byte(mut b: u8) -> u8 {
    b = (b & 0xF0) >> 4 | (b & 0x0F) << 4;
    b = (b & 0xCC) >> 2 | (b & 0x33) << 2;
    b = (b & 0xAA) >> 1 | (b & 0x55) << 1;
    b
}

// --------------------------------------------------------------------------- //
// Snapshot del PPU (sin framebuffer) para búsqueda con rollouts.
// Se captura SIEMPRE en un límite de frame (después de `Console::step_frame`).
// --------------------------------------------------------------------------- //

/// Estado completo del 2C02 excepto el framebuffer y los buffers de scanline
/// (`line_bg`, `sprite0_mask`), que son scratch y se regeneran al renderizar.
#[derive(Clone)]
pub struct PpuState {
    pub vram: [u8; 4096],
    pub palette: [u8; 32],
    pub oam: [u8; 256],
    pub oam_addr: u8,
    pub ctrl: u8,
    pub mask: u8,
    pub status: u8,
    pub v: u16,
    pub t: u16,
    pub fine_x: u8,
    pub w: bool,
    pub read_buffer: u8,
    pub scanline: u16,
    pub dot: u16,
    pub frame_ready: bool,
    pub nmi: bool,
    pub frame_count: u64,
    pub sprite0_hit_at: i32,
}

impl Ppu {
    /// Captura el estado del PPU. Llamar solo entre frames.
    pub fn save_state(&self) -> PpuState {
        PpuState {
            vram: self.vram,
            palette: self.palette,
            oam: self.oam,
            oam_addr: self.oam_addr,
            ctrl: self.ctrl,
            mask: self.mask,
            status: self.status,
            v: self.v,
            t: self.t,
            fine_x: self.fine_x,
            w: self.w,
            read_buffer: self.read_buffer,
            scanline: self.scanline,
            dot: self.dot,
            frame_ready: self.frame_ready,
            nmi: self.nmi,
            frame_count: self.frame_count,
            sprite0_hit_at: self.sprite0_hit_at,
        }
    }

    /// Restaura un estado capturado en un límite de frame. El framebuffer
    /// actual se conserva (no participa en la lógica del juego).
    pub fn load_state(&mut self, s: &PpuState) {
        self.vram = s.vram;
        self.palette = s.palette;
        self.oam = s.oam;
        self.oam_addr = s.oam_addr;
        self.ctrl = s.ctrl;
        self.mask = s.mask;
        self.status = s.status;
        self.v = s.v;
        self.t = s.t;
        self.fine_x = s.fine_x;
        self.w = s.w;
        self.read_buffer = s.read_buffer;
        self.frame_count = s.frame_count;
        self.sprite0_hit_at = s.sprite0_hit_at;
        // Rearrancar el PPU al inicio del frame visible: si se restaura el
        // punto exacto donde `frame_ready` acababa de subir, el siguiente
        // `step_frame` puede salir tras 1-2 instrucciones y congelar el juego.
        self.scanline = 0;
        self.dot = 0;
        self.frame_ready = false;
        self.nmi = false;
        self.line_bg = [0; WIDTH];
        self.sprite0_mask = [false; WIDTH];
    }
}
