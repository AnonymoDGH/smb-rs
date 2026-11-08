use smb_rs::{Cartridge, Console, BUTTON_RIGHT, BUTTON_B, BUTTON_START};
use std::path::Path;

const ROM: &str = r"C:\Users\USUARIO\Downloads\Super Mario Bros. (World)\Super Mario Bros. (World).nes";

fn st(con: &Console) -> String {
    let x = (con.bus.ram[0x6D] as i32) * 256 + con.bus.ram[0x86] as i32;
    format!(
        "x={x:4} st={:02X} g={:02X} t={:02X}{:02X}{:02X} op={:02X} pc={:04X}",
        con.bus.ram[0x000E],
        con.bus.ram[0x001D],
        con.bus.ram[0x07F8],
        con.bus.ram[0x07F9],
        con.bus.ram[0x07FA],
        con.bus.ram[0x0770],
        con.cpu.pc
    )
}

fn step(con: &mut Console, b: u8) {
    con.set_buttons(0, b);
    con.step_frame();
    con.bus.apu.samples.clear();
}

fn main() {
    let cart = Cartridge::load(Path::new(ROM)).expect("rom");
    let mut con = Console::new(cart);

    println!("=== 1) SIN INPUT 0..400 ===");
    for f in 0..400usize {
        step(&mut con, 0);
        if f % 50 == 0 {
            println!("f={f:4} {}", st(&con));
        }
    }
    println!("=== 2) START 2 frames, luego nada 400..900 ===");
    step(&mut con, BUTTON_START);
    step(&mut con, BUTTON_START);
    for f in 400..900usize {
        step(&mut con, 0);
        if f % 50 == 0 {
            println!("f={f:4} {}", st(&con));
        }
    }
    println!("=== 3) HOLD RIGHT+B 900..1100 ===");
    for f in 900..1100usize {
        step(&mut con, BUTTON_RIGHT | BUTTON_B);
        if f % 25 == 0 {
            println!("f={f:4} {}", st(&con));
        }
    }
    println!("=== 4) NADA 1100..1300 ===");
    for f in 1100..1300usize {
        step(&mut con, 0);
        if f % 50 == 0 {
            println!("f={f:4} {}", st(&con));
        }
    }
}
