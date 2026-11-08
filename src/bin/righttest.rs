use smb_rs::{Cartridge, Console, BUTTON_A, BUTTON_B, BUTTON_RIGHT, BUTTON_START};
use std::path::Path;

const ROM: &str = r"C:\Users\USUARIO\Downloads\Super Mario Bros. (World)\Super Mario Bros. (World).nes";

fn main() {
    let cart = Cartridge::load(Path::new(ROM)).expect("rom");
    let mut con = Console::new(cart);
    // boot: run until we think in gameplay
    for f in 0..1800usize {
        let pulse = f % 240 < 6 && f > 30;
        con.set_buttons(0, if pulse { BUTTON_START } else { 0 });
        con.step_frame();
        con.bus.apu.samples.clear();
        if f > 0 && f % 300 == 0 {
            let x = (con.bus.ram[0x6D] as i32) * 256 + con.bus.ram[0x86] as i32;
            eprintln!("boot f={f} x={x} st={:02X} op={:02X} t={:02X}", con.bus.ram[0x000E], con.bus.ram[0x0770], con.bus.ram[0x07F8]);
        }
    }
    // hold right+B for 600 frames
    let mut snaps = Vec::new();
    for f in 0..600usize {
        con.set_buttons(0, BUTTON_RIGHT | BUTTON_B);
        con.step_frame();
        con.bus.apu.samples.clear();
        if f % 50 == 0 {
            let x = (con.bus.ram[0x6D] as i32) * 256 + con.bus.ram[0x86] as i32;
            snaps.push(format!("f={f} x={x} st={:02X}", con.bus.ram[0x000E]));
        }
    }
    for s in snaps { println!("{s}"); }

    // --- ahora: restaurar a un snapshot y correr RIGHT otra vez ---
    let cart2 = Cartridge::load(std::path::Path::new(ROM)).expect("rom");
    let mut con2 = Console::new(cart2);
    for f in 0..1800usize {
        let pulse = f % 240 < 6 && f > 30;
        con2.set_buttons(0, if pulse { BUTTON_START } else { 0 });
        con2.step_frame();
        con2.bus.apu.samples.clear();
        let x = (con2.bus.ram[0x6D] as i32) * 256 + con2.bus.ram[0x86] as i32;
        let st = con2.bus.ram[0x000E];
        let t = con2.bus.ram[0x07F8];
        if x > 30 && st == 0x08 && t > 0 {
            eprintln!("snap f={f} x={x} st=08 t={:02X}", t);
            break;
        }
    }
    // guardar estado bruto: ram+cpu+ppu minimales para simular restore
    let ram_snap = con2.bus.ram;
    println!("--- tras snapshot, holding RIGHT ---");
    for f in 0..300usize {
        con2.set_buttons(0, BUTTON_RIGHT | BUTTON_B);
        con2.step_frame();
        con2.bus.apu.samples.clear();
        if f % 50 == 0 {
            let x = (con2.bus.ram[0x6D] as i32) * 256 + con2.bus.ram[0x86] as i32;
            println!("post-snap f={f} x={x} st={:02X}", con2.bus.ram[0x000E]);
        }
    }
    println!("done");
}
