//! iNES cartridge loader + NROM (mapper 0) memory mapping.

pub const MIRROR_HORIZONTAL: u8 = 0;
pub const MIRROR_VERTICAL: u8 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mirroring {
    Horizontal,
    Vertical,
    FourScreen,
}

pub struct Cartridge {
    pub prg: Vec<u8>,
    pub chr: Vec<u8>,
    pub chr_ram: bool,
    pub mapper: u8,
    pub mirroring: Mirroring,
    pub battery: bool,
    pub prg_sha256: String,
    pub title: String,
}

impl Cartridge {
    pub fn load(path: &std::path::Path) -> Result<Cartridge, String> {
        let raw = std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        Cartridge::from_bytes(&raw)
    }

    pub fn from_bytes(raw: &[u8]) -> Result<Cartridge, String> {
        if raw.len() < 16 || &raw[0..4] != b"NES\x1a" {
            return Err("not an iNES image (missing NES\\x1a magic)".into());
        }
        let prg_banks = raw[4] as usize;
        let chr_banks = raw[5] as usize;
        let flags6 = raw[6];
        let flags7 = raw[7];
        let mapper = (flags7 & 0xF0) | (flags6 >> 4);
        let mirroring = if flags6 & 0x08 != 0 {
            Mirroring::FourScreen
        } else if flags6 & 0x01 != 0 {
            Mirroring::Vertical
        } else {
            Mirroring::Horizontal
        };
        let mut off = 16usize;
        if flags6 & 0x04 != 0 {
            off += 512; // trainer
        }
        if raw.len() < off {
            return Err(format!(
                "truncated ROM: header declares a 512-byte trainer, file has only {} bytes",
                raw.len()
            ));
        }
        if prg_banks == 0 {
            return Err("invalid ROM: header declares zero PRG banks".into());
        }
        let prg_len = prg_banks * 16384;
        let chr_len = chr_banks * 8192;
        if raw.len() < off + prg_len + chr_len {
            return Err(format!(
                "truncated ROM: header declares {} PRG + {} CHR bytes, file has {}",
                prg_len,
                chr_len,
                raw.len() - off
            ));
        }
        let prg = raw[off..off + prg_len].to_vec();
        // CHR bank count 0 means the board has 8 KiB of writable CHR-RAM; a
        // CHR-ROM that happens to be full of zeros is still ROM.
        let chr_ram = chr_len == 0;
        let chr = if chr_ram {
            vec![0u8; 8192]
        } else {
            raw[off + prg_len..off + prg_len + chr_len].to_vec()
        };
        if mapper != 0 {
            return Err(format!(
                "mapper {mapper} unsupported: this build implements NROM (mapper 0) only"
            ));
        }
        Ok(Cartridge {
            prg,
            chr,
            chr_ram,
            mapper,
            mirroring,
            battery: flags6 & 0x02 != 0,
            prg_sha256: sha256_hex(&raw[off..off + prg_len]),
            title: String::new(),
        })
    }

    /// CPU bus read in $4020-$FFFF space.
    #[inline]
    pub fn cpu_read(&self, addr: u16) -> u8 {
        if addr >= 0x8000 {
            let i = (addr as usize - 0x8000) % self.prg.len().max(1);
            self.prg[i]
        } else {
            0
        }
    }

    #[inline]
    pub fn cpu_write(&mut self, _addr: u16, _val: u8) {
        // NROM has no bank switching; writes are ignored.
    }

    /// PPU bus read in $0000-$1FFF (pattern tables).
    #[inline]
    pub fn ppu_read(&self, addr: u16) -> u8 {
        let a = (addr & 0x1FFF) as usize;
        self.chr[a % self.chr.len()]
    }

    #[inline]
    pub fn ppu_write(&mut self, addr: u16, val: u8) {
        if self.chr_ram {
            let a = (addr & 0x1FFF) as usize;
            let len = self.chr.len();
            self.chr[a % len] = val;
        }
        // CHR-ROM: writes ignored.
    }
}

fn sha256_hex(data: &[u8]) -> String {
    // Compact SHA-256, only used for provenance reporting.
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}
