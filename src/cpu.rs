//! Cycle-counted MOS 6502 core (all documented opcodes + common illegal NOPs).

pub trait Bus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, val: u8);
}

pub const FLAG_C: u8 = 0x01;
pub const FLAG_Z: u8 = 0x02;
pub const FLAG_I: u8 = 0x04;
pub const FLAG_D: u8 = 0x08;
pub const FLAG_B: u8 = 0x10;
pub const FLAG_U: u8 = 0x20;
pub const FLAG_V: u8 = 0x40;
pub const FLAG_N: u8 = 0x80;

#[derive(Clone)]
pub struct Cpu {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub sp: u8,
    pub pc: u16,
    pub p: u8,
    pub nmi_pending: bool,
    pub irq_line: bool,
    pub cycles: u64,
    pub jammed: bool,
}

impl Cpu {
    pub fn new<B: Bus>(bus: &mut B) -> Cpu {
        let lo = bus.read(0xFFFC) as u16;
        let hi = bus.read(0xFFFD) as u16;
        Cpu {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xFD,
            pc: (hi << 8) | lo,
            p: FLAG_I | FLAG_U,
            nmi_pending: false,
            irq_line: false,
            cycles: 0,
            jammed: false,
        }
    }

    #[inline]
    fn read<B: Bus>(&mut self, bus: &mut B, addr: u16) -> u8 {
        bus.read(addr)
    }

    #[inline]
    fn write<B: Bus>(&mut self, bus: &mut B, addr: u16, v: u8) {
        bus.write(addr, v);
    }

    #[inline]
    fn fetch<B: Bus>(&mut self, bus: &mut B) -> u8 {
        let v = bus.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }

    #[inline]
    fn fetch16<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let lo = self.fetch(bus) as u16;
        let hi = self.fetch(bus) as u16;
        (hi << 8) | lo
    }

    #[inline]
    fn set_flag(&mut self, f: u8, on: bool) {
        if on {
            self.p |= f;
        } else {
            self.p &= !f;
        }
    }

    #[inline]
    fn flag(&self, f: u8) -> bool {
        self.p & f != 0
    }

    #[inline]
    fn nz(&mut self, v: u8) {
        self.set_flag(FLAG_Z, v == 0);
        self.set_flag(FLAG_N, v & 0x80 != 0);
    }

    fn push<B: Bus>(&mut self, bus: &mut B, v: u8) {
        bus.write(0x0100 | self.sp as u16, v);
        self.sp = self.sp.wrapping_sub(1);
    }

    fn pop<B: Bus>(&mut self, bus: &mut B) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        bus.read(0x0100 | self.sp as u16)
    }

    fn adc(&mut self, v: u8) {
        let a = self.a as u16;
        let carry = if self.flag(FLAG_C) { 1 } else { 0 };
        let sum = a + v as u16 + carry;
        let result = sum as u8;
        self.set_flag(FLAG_C, sum > 0xFF);
        let overflow = (!(self.a ^ v) & (self.a ^ result) & 0x80) != 0;
        self.set_flag(FLAG_V, overflow);
        self.a = result;
        self.nz(result);
    }

    fn sbc(&mut self, v: u8) {
        self.adc(v ^ 0xFF);
    }

    fn compare(&mut self, reg: u8, v: u8) {
        let r = reg.wrapping_sub(v);
        self.set_flag(FLAG_C, reg >= v);
        self.nz(r);
    }

    fn asl(&mut self, v: u8) -> u8 {
        self.set_flag(FLAG_C, v & 0x80 != 0);
        let r = v << 1;
        self.nz(r);
        r
    }

    fn lsr(&mut self, v: u8) -> u8 {
        self.set_flag(FLAG_C, v & 1 != 0);
        let r = v >> 1;
        self.nz(r);
        r
    }

    fn rol(&mut self, v: u8) -> u8 {
        let c = if self.flag(FLAG_C) { 1 } else { 0 };
        self.set_flag(FLAG_C, v & 0x80 != 0);
        let r = (v << 1) | c;
        self.nz(r);
        r
    }

    fn ror(&mut self, v: u8) -> u8 {
        let c = if self.flag(FLAG_C) { 0x80 } else { 0 };
        self.set_flag(FLAG_C, v & 1 != 0);
        let r = (v >> 1) | c;
        self.nz(r);
        r
    }

    fn branch<B: Bus>(&mut self, bus: &mut B, cond: bool, base: u32) -> u32 {
        if cond {
            let off = self.fetch(bus) as i8;
            let target = self.pc.wrapping_add(off as u16);
            let crossed = (target & 0xFF00) != (self.pc & 0xFF00);
            self.pc = target;
            base + 1 + if crossed { 1 } else { 0 }
        } else {
            self.pc = self.pc.wrapping_add(1);
            base
        }
    }

    // ---- addressing helpers: return (address, page_crossed) ----
    fn am_zp<B: Bus>(&mut self, bus: &mut B) -> u16 {
        self.fetch(bus) as u16
    }
    fn am_zpx<B: Bus>(&mut self, bus: &mut B) -> u16 {
        (self.fetch(bus).wrapping_add(self.x)) as u16
    }
    fn am_zpy<B: Bus>(&mut self, bus: &mut B) -> u16 {
        (self.fetch(bus).wrapping_add(self.y)) as u16
    }
    fn am_abs<B: Bus>(&mut self, bus: &mut B) -> u16 {
        self.fetch16(bus)
    }
    fn am_absx<B: Bus>(&mut self, bus: &mut B) -> (u16, bool) {
        let base = self.fetch16(bus);
        let addr = base.wrapping_add(self.x as u16);
        (addr, (base & 0xFF00) != (addr & 0xFF00))
    }
    fn am_absy<B: Bus>(&mut self, bus: &mut B) -> (u16, bool) {
        let base = self.fetch16(bus);
        let addr = base.wrapping_add(self.y as u16);
        (addr, (base & 0xFF00) != (addr & 0xFF00))
    }
    fn am_indx<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let zp = self.fetch(bus).wrapping_add(self.x);
        let lo = bus.read(zp as u16) as u16;
        let hi = bus.read(zp.wrapping_add(1) as u16) as u16;
        (hi << 8) | lo
    }
    fn am_indy<B: Bus>(&mut self, bus: &mut B) -> (u16, bool) {
        let zp = self.fetch(bus);
        let lo = bus.read(zp as u16) as u16;
        let hi = bus.read(zp.wrapping_add(1) as u16) as u16;
        let base = (hi << 8) | lo;
        let addr = base.wrapping_add(self.y as u16);
        (addr, (base & 0xFF00) != (addr & 0xFF00))
    }
    fn am_ind<B: Bus>(&mut self, bus: &mut B) -> u16 {
        let ptr = self.fetch16(bus);
        // 6502 page-boundary bug is reproduced faithfully.
        let lo = bus.read(ptr) as u16;
        let hi = bus.read((ptr & 0xFF00) | ((ptr + 1) & 0x00FF)) as u16;
        (hi << 8) | lo
    }

    fn interrupt<B: Bus>(&mut self, bus: &mut B, vector: u16, brk: bool) {
        let pc = self.pc;
        self.push(bus, (pc >> 8) as u8);
        self.push(bus, pc as u8);
        let mut flags = self.p | FLAG_U;
        if brk {
            flags |= FLAG_B;
        } else {
            flags &= !FLAG_B;
        }
        self.push(bus, flags);
        self.set_flag(FLAG_I, true);
        let lo = bus.read(vector) as u16;
        let hi = bus.read(vector + 1) as u16;
        self.pc = (hi << 8) | lo;
    }

    /// Execute one instruction (or service an interrupt). Returns CPU cycles used.
    pub fn step<B: Bus>(&mut self, bus: &mut B) -> u32 {
        if self.nmi_pending {
            self.nmi_pending = false;
            self.interrupt(bus, 0xFFFA, false);
            self.cycles += 7;
            return 7;
        }
        if self.irq_line && !self.flag(FLAG_I) {
            self.interrupt(bus, 0xFFFE, false);
            self.cycles += 7;
            return 7;
        }

        let op = self.fetch(bus);
        let cyc = self.execute(bus, op);
        self.cycles += cyc as u64;
        cyc
    }

    fn execute<B: Bus>(&mut self, bus: &mut B, op: u8) -> u32 {
        match op {
            // ---- LDA ----
            0xA9 => { let v = self.fetch(bus); self.a = v; self.nz(v); 2 }
            0xA5 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 3 }
            0xB5 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 4 }
            0xAD => { let a = self.am_abs(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 4 }
            0xBD => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 4 + p as u32 }
            0xB9 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 4 + p as u32 }
            0xA1 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 6 }
            0xB1 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.a = v; self.nz(v); 5 + p as u32 }
            // ---- LDX ----
            0xA2 => { let v = self.fetch(bus); self.x = v; self.nz(v); 2 }
            0xA6 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.x = v; self.nz(v); 3 }
            0xB6 => { let a = self.am_zpy(bus); let v = self.read(bus, a); self.x = v; self.nz(v); 4 }
            0xAE => { let a = self.am_abs(bus); let v = self.read(bus, a); self.x = v; self.nz(v); 4 }
            0xBE => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.x = v; self.nz(v); 4 + p as u32 }
            // ---- LDY ----
            0xA0 => { let v = self.fetch(bus); self.y = v; self.nz(v); 2 }
            0xA4 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.y = v; self.nz(v); 3 }
            0xB4 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.y = v; self.nz(v); 4 }
            0xAC => { let a = self.am_abs(bus); let v = self.read(bus, a); self.y = v; self.nz(v); 4 }
            0xBC => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.y = v; self.nz(v); 4 + p as u32 }
            // ---- STA ----
            0x85 => { let a = self.am_zp(bus); self.write(bus, a, self.a); 3 }
            0x95 => { let a = self.am_zpx(bus); self.write(bus, a, self.a); 4 }
            0x8D => { let a = self.am_abs(bus); self.write(bus, a, self.a); 4 }
            0x9D => { let a = self.am_absx(bus).0; self.write(bus, a, self.a); 5 }
            0x99 => { let a = self.am_absy(bus).0; self.write(bus, a, self.a); 5 }
            0x81 => { let a = self.am_indx(bus); self.write(bus, a, self.a); 6 }
            0x91 => { let a = self.am_indy(bus).0; self.write(bus, a, self.a); 6 }
            // ---- STX / STY ----
            0x86 => { let a = self.am_zp(bus); self.write(bus, a, self.x); 3 }
            0x96 => { let a = self.am_zpy(bus); self.write(bus, a, self.x); 4 }
            0x8E => { let a = self.am_abs(bus); self.write(bus, a, self.x); 4 }
            0x84 => { let a = self.am_zp(bus); self.write(bus, a, self.y); 3 }
            0x94 => { let a = self.am_zpx(bus); self.write(bus, a, self.y); 4 }
            0x8C => { let a = self.am_abs(bus); self.write(bus, a, self.y); 4 }
            // ---- transfers ----
            0xAA => { self.x = self.a; self.nz(self.x); 2 }
            0xA8 => { self.y = self.a; self.nz(self.y); 2 }
            0x8A => { self.a = self.x; self.nz(self.a); 2 }
            0x98 => { self.a = self.y; self.nz(self.a); 2 }
            0xBA => { self.x = self.sp; self.nz(self.x); 2 }
            0x9A => { self.sp = self.x; 2 }
            // ---- stack ----
            0x48 => { let v = self.a; self.push(bus, v); 3 }
            0x68 => { let v = self.pop(bus); self.a = v; self.nz(v); 4 }
            0x08 => { let v = self.p | FLAG_B | FLAG_U; self.push(bus, v); 3 }
            0x28 => { let v = self.pop(bus); self.p = (v | FLAG_U) & !FLAG_B; 4 }
            // ---- ADC ----
            0x69 => { let v = self.fetch(bus); self.adc(v); 2 }
            0x65 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.adc(v); 3 }
            0x75 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.adc(v); 4 }
            0x6D => { let a = self.am_abs(bus); let v = self.read(bus, a); self.adc(v); 4 }
            0x7D => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.adc(v); 4 + p as u32 }
            0x79 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.adc(v); 4 + p as u32 }
            0x61 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.adc(v); 6 }
            0x71 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.adc(v); 5 + p as u32 }
            // ---- SBC ----
            0xE9 => { let v = self.fetch(bus); self.sbc(v); 2 }
            0xE5 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.sbc(v); 3 }
            0xF5 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.sbc(v); 4 }
            0xED => { let a = self.am_abs(bus); let v = self.read(bus, a); self.sbc(v); 4 }
            0xFD => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.sbc(v); 4 + p as u32 }
            0xF9 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.sbc(v); 4 + p as u32 }
            0xE1 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.sbc(v); 6 }
            0xF1 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.sbc(v); 5 + p as u32 }
            // ---- AND ----
            0x29 => { let v = self.fetch(bus); self.a &= v; self.nz(self.a); 2 }
            0x25 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 3 }
            0x35 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 4 }
            0x2D => { let a = self.am_abs(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 4 }
            0x3D => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 4 + p as u32 }
            0x39 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 4 + p as u32 }
            0x21 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 6 }
            0x31 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.a &= v; self.nz(self.a); 5 + p as u32 }
            // ---- ORA ----
            0x09 => { let v = self.fetch(bus); self.a |= v; self.nz(self.a); 2 }
            0x05 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 3 }
            0x15 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 4 }
            0x0D => { let a = self.am_abs(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 4 }
            0x1D => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 4 + p as u32 }
            0x19 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 4 + p as u32 }
            0x01 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 6 }
            0x11 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.a |= v; self.nz(self.a); 5 + p as u32 }
            // ---- EOR ----
            0x49 => { let v = self.fetch(bus); self.a ^= v; self.nz(self.a); 2 }
            0x45 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 3 }
            0x55 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 4 }
            0x4D => { let a = self.am_abs(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 4 }
            0x5D => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 4 + p as u32 }
            0x59 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 4 + p as u32 }
            0x41 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 6 }
            0x51 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.a ^= v; self.nz(self.a); 5 + p as u32 }
            // ---- CMP / CPX / CPY ----
            0xC9 => { let v = self.fetch(bus); self.compare(self.a, v); 2 }
            0xC5 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.compare(self.a, v); 3 }
            0xD5 => { let a = self.am_zpx(bus); let v = self.read(bus, a); self.compare(self.a, v); 4 }
            0xCD => { let a = self.am_abs(bus); let v = self.read(bus, a); self.compare(self.a, v); 4 }
            0xDD => { let (a, p) = self.am_absx(bus); let v = self.read(bus, a); self.compare(self.a, v); 4 + p as u32 }
            0xD9 => { let (a, p) = self.am_absy(bus); let v = self.read(bus, a); self.compare(self.a, v); 4 + p as u32 }
            0xC1 => { let a = self.am_indx(bus); let v = self.read(bus, a); self.compare(self.a, v); 6 }
            0xD1 => { let (a, p) = self.am_indy(bus); let v = self.read(bus, a); self.compare(self.a, v); 5 + p as u32 }
            0xE0 => { let v = self.fetch(bus); self.compare(self.x, v); 2 }
            0xE4 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.compare(self.x, v); 3 }
            0xEC => { let a = self.am_abs(bus); let v = self.read(bus, a); self.compare(self.x, v); 4 }
            0xC0 => { let v = self.fetch(bus); self.compare(self.y, v); 2 }
            0xC4 => { let a = self.am_zp(bus); let v = self.read(bus, a); self.compare(self.y, v); 3 }
            0xCC => { let a = self.am_abs(bus); let v = self.read(bus, a); self.compare(self.y, v); 4 }
            // ---- BIT ----
            0x24 => {
                let a = self.am_zp(bus); let v = self.read(bus, a);
                self.set_flag(FLAG_Z, self.a & v == 0);
                self.set_flag(FLAG_N, v & 0x80 != 0);
                self.set_flag(FLAG_V, v & 0x40 != 0);
                3
            }
            0x2C => {
                let a = self.am_abs(bus); let v = self.read(bus, a);
                self.set_flag(FLAG_Z, self.a & v == 0);
                self.set_flag(FLAG_N, v & 0x80 != 0);
                self.set_flag(FLAG_V, v & 0x40 != 0);
                4
            }
            // ---- shifts in memory ----
            0x06 => { let a = self.am_zp(bus); let v = self.read(bus, a); let r = self.asl(v); self.write(bus, a, r); 5 }
            0x16 => { let a = self.am_zpx(bus); let v = self.read(bus, a); let r = self.asl(v); self.write(bus, a, r); 6 }
            0x0E => { let a = self.am_abs(bus); let v = self.read(bus, a); let r = self.asl(v); self.write(bus, a, r); 6 }
            0x1E => { let a = self.am_absx(bus).0; let v = self.read(bus, a); let r = self.asl(v); self.write(bus, a, r); 7 }
            0x46 => { let a = self.am_zp(bus); let v = self.read(bus, a); let r = self.lsr(v); self.write(bus, a, r); 5 }
            0x56 => { let a = self.am_zpx(bus); let v = self.read(bus, a); let r = self.lsr(v); self.write(bus, a, r); 6 }
            0x4E => { let a = self.am_abs(bus); let v = self.read(bus, a); let r = self.lsr(v); self.write(bus, a, r); 6 }
            0x5E => { let a = self.am_absx(bus).0; let v = self.read(bus, a); let r = self.lsr(v); self.write(bus, a, r); 7 }
            0x26 => { let a = self.am_zp(bus); let v = self.read(bus, a); let r = self.rol(v); self.write(bus, a, r); 5 }
            0x36 => { let a = self.am_zpx(bus); let v = self.read(bus, a); let r = self.rol(v); self.write(bus, a, r); 6 }
            0x2E => { let a = self.am_abs(bus); let v = self.read(bus, a); let r = self.rol(v); self.write(bus, a, r); 6 }
            0x3E => { let a = self.am_absx(bus).0; let v = self.read(bus, a); let r = self.rol(v); self.write(bus, a, r); 7 }
            0x66 => { let a = self.am_zp(bus); let v = self.read(bus, a); let r = self.ror(v); self.write(bus, a, r); 5 }
            0x76 => { let a = self.am_zpx(bus); let v = self.read(bus, a); let r = self.ror(v); self.write(bus, a, r); 6 }
            0x6E => { let a = self.am_abs(bus); let v = self.read(bus, a); let r = self.ror(v); self.write(bus, a, r); 6 }
            0x7E => { let a = self.am_absx(bus).0; let v = self.read(bus, a); let r = self.ror(v); self.write(bus, a, r); 7 }
            // ---- accumulators ----
            0x0A => { self.a = self.asl(self.a); 2 }
            0x4A => { self.a = self.lsr(self.a); 2 }
            0x2A => { self.a = self.rol(self.a); 2 }
            0x6A => { self.a = self.ror(self.a); 2 }
            // ---- INC / DEC ----
            0xE6 => { let a = self.am_zp(bus); let v = self.read(bus, a).wrapping_add(1); self.write(bus, a, v); self.nz(v); 5 }
            0xF6 => { let a = self.am_zpx(bus); let v = self.read(bus, a).wrapping_add(1); self.write(bus, a, v); self.nz(v); 6 }
            0xEE => { let a = self.am_abs(bus); let v = self.read(bus, a).wrapping_add(1); self.write(bus, a, v); self.nz(v); 6 }
            0xFE => { let a = self.am_absx(bus).0; let v = self.read(bus, a).wrapping_add(1); self.write(bus, a, v); self.nz(v); 7 }
            0xC6 => { let a = self.am_zp(bus); let v = self.read(bus, a).wrapping_sub(1); self.write(bus, a, v); self.nz(v); 5 }
            0xD6 => { let a = self.am_zpx(bus); let v = self.read(bus, a).wrapping_sub(1); self.write(bus, a, v); self.nz(v); 6 }
            0xCE => { let a = self.am_abs(bus); let v = self.read(bus, a).wrapping_sub(1); self.write(bus, a, v); self.nz(v); 6 }
            0xDE => { let a = self.am_absx(bus).0; let v = self.read(bus, a).wrapping_sub(1); self.write(bus, a, v); self.nz(v); 7 }
            // ---- register inc/dec ----
            0xE8 => { self.x = self.x.wrapping_add(1); self.nz(self.x); 2 }
            0xCA => { self.x = self.x.wrapping_sub(1); self.nz(self.x); 2 }
            0xC8 => { self.y = self.y.wrapping_add(1); self.nz(self.y); 2 }
            0x88 => { self.y = self.y.wrapping_sub(1); self.nz(self.y); 2 }
            // ---- flags ----
            0x18 => { self.set_flag(FLAG_C, false); 2 }
            0x38 => { self.set_flag(FLAG_C, true); 2 }
            0x58 => { self.set_flag(FLAG_I, false); 2 }
            0x78 => { self.set_flag(FLAG_I, true); 2 }
            0xB8 => { self.set_flag(FLAG_V, false); 2 }
            0xD8 => { self.set_flag(FLAG_D, false); 2 }
            0xF8 => { self.set_flag(FLAG_D, true); 2 }
            // ---- branches ----
            0x10 => self.branch(bus, !self.flag(FLAG_N), 2),
            0x30 => self.branch(bus, self.flag(FLAG_N), 2),
            0x50 => self.branch(bus, !self.flag(FLAG_V), 2),
            0x70 => self.branch(bus, self.flag(FLAG_V), 2),
            0x90 => self.branch(bus, !self.flag(FLAG_C), 2),
            0xB0 => self.branch(bus, self.flag(FLAG_C), 2),
            0xD0 => self.branch(bus, !self.flag(FLAG_Z), 2),
            0xF0 => self.branch(bus, self.flag(FLAG_Z), 2),
            // ---- jumps ----
            0x4C => { self.pc = self.am_abs(bus); 3 }
            0x6C => { self.pc = self.am_ind(bus); 5 }
            0x20 => {
                let target = self.fetch16(bus);
                let ret = self.pc.wrapping_sub(1);
                self.push(bus, (ret >> 8) as u8);
                self.push(bus, ret as u8);
                self.pc = target;
                6
            }
            0x60 => {
                let lo = self.pop(bus) as u16;
                let hi = self.pop(bus) as u16;
                self.pc = ((hi << 8) | lo).wrapping_add(1);
                6
            }
            0x40 => {
                let p = self.pop(bus);
                self.p = (p | FLAG_U) & !FLAG_B;
                let lo = self.pop(bus) as u16;
                let hi = self.pop(bus) as u16;
                self.pc = (hi << 8) | lo;
                6
            }
            0x00 => {
                // BRK is a two-byte instruction: skip the padding byte so the
                // pushed return address is the one after it.
                self.pc = self.pc.wrapping_add(1);
                self.interrupt(bus, 0xFFFE, true);
                7
            }
            0xEA => 2,
            // common illegal NOPs (SMB's data is never executed, but robustness)
            0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => 2,
            0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => { self.fetch(bus); 2 }
            0x04 | 0x44 | 0x64 => { self.am_zp(bus); 3 }
            0x0C => { self.am_abs(bus); 4 }
            0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => { self.am_zpx(bus); 4 }
            0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => { let (_, p) = self.am_absx(bus); 4 + p as u32 }
            _ => {
                // Unimplemented: treat as JAM, halt the CPU so the harness can report.
                self.pc = self.pc.wrapping_sub(1);
                self.jammed = true;
                2
            }
        }
    }
}
