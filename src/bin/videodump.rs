//! videodump <rom> <outdir> --frames N --every 1 --scale 2 --script "f:btn:v,..."
//! Writes frame_XXXXX.png + telemetry.csv with per-frame speed/inputs for video overlay.
//! Telemetry columns: frame,buttons,pad_bits,x_spd,x_moveforce,y_spd,x_pos,x_page,y_pos,state,facing,moving,x_abs,interval,screentimer,timer_hi
use std::path::{Path, PathBuf};
use smb_rs::console::*;
use smb_rs::Cartridge;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 { eprintln!("usage: videodump <rom> <outdir> [--frames N] [--script f:btn:1,...] [--scale N]"); std::process::exit(2); }
    let rom = PathBuf::from(&args[1]);
    let outdir = PathBuf::from(&args[2]);
    let mut max_frame: u64 = 600;
    let mut scale: usize = 2;
    let mut quiet = false;
    let mut script_events: Vec<(u64,u8,bool)> = Vec::new();
    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--frames" => { i+=1; max_frame = args[i].parse().unwrap_or(600); }
            "--scale" => { i+=1; scale = args[i].parse().unwrap_or(2); }
            "--quiet" => quiet = true,
            "--script" => { i+=1; for ev in args[i].split(',') { let p: Vec<&str> = ev.split(':').collect(); if p.len()!=3 {continue;} let f: u64 = p[0].parse().unwrap_or(0); let btn = match p[1].to_ascii_lowercase().as_str() { "a"=>BUTTON_A, "b"=>BUTTON_B, "start"=>BUTTON_START, "select"=>BUTTON_SELECT, "left"=>BUTTON_LEFT, "right"=>BUTTON_RIGHT, "up"=>BUTTON_UP, "down"=>BUTTON_DOWN, _=>0 }; script_events.push((f, btn, p[2]=="1")); } }
            o => eprintln!("ignoring {o}"),
        }
        i+=1;
    }
    let cart = Cartridge::load(&rom).expect("rom");
    let mut con = Console::new(cart);
    std::fs::create_dir_all(&outdir).ok();
    let mut ev = script_events; ev.sort_by_key(|e| e.0);
    let mut next = 0usize;
    let mut buttons = 0u8;
    let mut csv = String::new();
    csv.push_str("frame,buttons,x_spd,x_moveforce,y_spd,x_pos,x_page,y_pos,state,facing,moving,x_abs,interval,screentimer,timer_hi,x_abs_dec\n");
    for frame in 0..=max_frame {
        while next < ev.len() && ev[next].0 == frame { if ev[next].2 { buttons |= ev[next].1; } else { buttons &= !ev[next].1; } next+=1; }
        con.set_buttons(0, buttons);
        if !con.step_frame() { eprintln!("jammed {frame}"); break; }
        let r = &con.bus.ram;
        let x_spd = r[0x57] as i8 as i32;
        let y_spd = r[0x9F] as i8 as i32;
        let x_moveforce = r[0x0705 & 0x7FF] as i32; // $0705
        // Player_X_MoveForce $0705, Player_X_Speed $57, Player_Y_Speed $9F, Player_X_Position $86, Player_PageLoc $06D? use $006D? actually $06D is page? use ram[0x6D]
        let x_pos = r[0x86] as i32;
        let x_page = r[0x6D] as i32;
        let y_pos = r[0xCE] as i32;
        let state = r[0x1D] as i32;
        let facing = r[0x33] as i32;
        let moving = r[0x45] as i32;
        let x_abs = r[0x0700 & 0x7FF] as i32;
        let interval = r[0x077F & 0x7FF] as i32;
        let screentimer = r[0x07A0 & 0x7FF] as i32;
        let timer_hi = r[0x07F8 & 0x7FF] as i32;
        csv.push_str(&format!("{frame},{buttons},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n", x_spd, x_moveforce, y_spd, x_pos, x_page, y_pos, state, facing, moving, x_abs, interval, screentimer, timer_hi, x_spd.abs()));
        let name = outdir.join(format!("frame_{frame:05}.png"));
        smb_rs::pngout::write_png(&name, con.framebuffer(), 256, 240, scale);
        if !quiet && frame % 60 == 0 { println!("frame {frame} spd={x_spd} moveforce={x_moveforce} y={y_spd} state={state} interval={interval}"); }
    }
    std::fs::write(outdir.join("telemetry.csv"), csv).expect("csv");
    println!("done {} frames -> {}", max_frame, outdir.display());
}
