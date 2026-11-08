//! CPU bus, memory map, controllers, DMA, and the frame-driving console.

use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::cpu::{Bus as CpuBus, Cpu};
use crate::ppu::Ppu;

pub const BUTTON_A: u8 = 0x01;
pub const BUTTON_B: u8 = 0x02;
pub const BUTTON_SELECT: u8 = 0x04;
pub const BUTTON_START: u8 = 0x08;
pub const BUTTON_UP: u8 = 0x10;
pub const BUTTON_DOWN: u8 = 0x20;
pub const BUTTON_LEFT: u8 = 0x40;
pub const BUTTON_RIGHT: u8 = 0x80;

pub struct Bus {
    pub ram: [u8; 2048],
    pub ppu: Ppu,
    pub apu: Apu,
    pub cart: Cartridge,
    pub buttons: [u8; 2],
    pub strobe: bool,
    pub shift: [u8; 2],
    pub open_bus: u8,
}

impl Bus {
    pub fn read_debug(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => self.ram[(addr as usize) & 0x7FF],
            0x8000..=0xFFFF => self.cart.cpu_read(addr),
            _ => 0,
        }
    }

    pub fn new(cart: Cartridge) -> Bus {
        let mirroring = cart.mirroring;
        Bus {
            ram: [0; 2048],
            ppu: Ppu::new(mirroring),
            apu: Apu::new(),
            cart,
            buttons: [0; 2],
            strobe: false,
            shift: [0; 2],
            open_bus: 0,
        }
    }
}

impl CpuBus for Bus {
    fn read(&mut self, addr: u16) -> u8 {
        let v = match addr {
            0x0000..=0x1FFF => self.ram[(addr as usize) & 0x7FF],
            0x2000..=0x3FFF => self.ppu.cpu_read(addr & 0x2007, &self.cart),
            0x4014 => self.open_bus,
            0x4015 => {
                let mut s = 0u8;
                if self.apu.irq() {
                    s |= 0x40;
                }
                self.apu.clear_frame_irq();
                s
            }
            0x4016 | 0x4017 => {
                let i = (addr as usize) & 1;
                if self.strobe {
                    self.buttons[i] & 1
                } else {
                    let b = self.shift[i] & 1;
                    self.shift[i] = (self.shift[i] >> 1) | 0x80;
                    b
                }
            }
            0x4000..=0x401F => self.open_bus,
            0x4020..=0x7FFF => self.open_bus,
            _ => self.cart.cpu_read(addr),
        };
        self.open_bus = v;
        v
    }

    fn write(&mut self, addr: u16, val: u8) {
        self.open_bus = val;
        match addr {
            0x0000..=0x1FFF => self.ram[(addr as usize) & 0x7FF] = val,
            0x2000..=0x3FFF => self.ppu.cpu_write(addr & 0x2007, val, &mut self.cart),
            0x4014 => {
                let base = (val as u16) << 8;
                for i in 0..256u16 {
                    let b = self.read(base + i);
                    self.ppu.oam[i as usize] = b;
                }
            }
            0x4016 => {
                self.strobe = val & 1 != 0;
                if self.strobe {
                    self.shift = self.buttons;
                }
            }
            0x4000..=0x4013 | 0x4015 | 0x4017 => self.apu.write(addr, val),
            0x8000..=0xFFFF => self.cart.cpu_write(addr, val),
            _ => {}
        }
    }
}

pub struct Console {
    pub cpu: Cpu,
    pub bus: Bus,
    pub frames: u64,
}

impl Console {
    pub fn new(cart: Cartridge) -> Console {
        let mut bus = Bus::new(cart);
        let cpu = Cpu::new(&mut bus);
        Console { cpu, bus, frames: 0 }
    }

    pub fn reset(&mut self) {
        self.bus.ppu = Ppu::new(self.bus.cart.mirroring);
        self.bus.apu = Apu::new();
        self.cpu = Cpu::new(&mut self.bus);
    }

    pub fn set_buttons(&mut self, pad: usize, buttons: u8) {
        self.bus.buttons[pad & 1] = buttons;
    }

    /// Run until the PPU finishes a frame. Returns false if the CPU jammed.
    pub fn step_frame(&mut self) -> bool {
        self.bus.ppu.frame_ready = false;
        let mut guard: u64 = 0;
        loop {
            let cyc = self.cpu.step(&mut self.bus);
            self.bus.ppu.step(cyc * 3, &self.bus.cart);
            self.bus.apu.step(cyc);
            if self.bus.ppu.nmi {
                self.bus.ppu.nmi = false;
                self.cpu.nmi_pending = true;
            }
            self.cpu.irq_line = self.bus.apu.irq();
            if self.bus.ppu.frame_ready {
                self.bus.ppu.frame_ready = false;
                break;
            }
            guard += cyc as u64;
            if self.cpu.jammed || guard > 300_000 {
                return false;
            }
        }
        self.frames += 1;
        true
    }

    pub fn framebuffer(&self) -> &[u32] {
        &self.bus.ppu.frame
    }
}
