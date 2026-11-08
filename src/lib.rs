//! smb-rs: a from-scratch NES (2C02 + 6502 + NROM) emulator in Rust, built to
//! run and reverse-engineer Super Mario Bros. (World).

pub mod apu;
#[cfg(feature = "audio")]
pub mod audio;
pub mod cartridge;
pub mod console;
pub mod cpu;
pub mod pngout;
pub mod ppu;

pub use cartridge::Cartridge;
pub use console::{
    Console, BUTTON_A, BUTTON_B, BUTTON_DOWN, BUTTON_LEFT, BUTTON_RIGHT, BUTTON_SELECT,
    BUTTON_START, BUTTON_UP,
};
