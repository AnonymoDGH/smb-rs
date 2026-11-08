//! Headless harness: run N frames, apply scripted input, dump PNG screenshots.
//!
//!   nesdump <rom> <outdir> [--frames 60,180,360] [--script "200:start:1,210:start:0"]
//!           [--scale 2] [--every N] [--quiet]

use std::path::{Path, PathBuf};

use smb_rs::console::*;
use smb_rs::Cartridge;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: nesdump <rom> <outdir> [--frames a,b,c] [--script f:btn:1,...] [--scale N] [--every N]");
        std::process::exit(2);
    }
    let rom = PathBuf::from(&args[1]);
    let outdir = PathBuf::from(&args[2]);
    let mut frames: Vec<u64> = Vec::new();
    let mut scale = 2usize;
    let mut every: u64 = 0;
    let mut quiet = false;
    let mut state = false;
    let mut nestest = 0usize;
    let mut wav_path: Option<PathBuf> = None;
    let mut script_events: Vec<(u64, u8, bool)> = Vec::new();

    let mut i = 3;
    while i < args.len() {
        match args[i].as_str() {
            "--frames" => {
                i += 1;
                frames = args[i].split(',').filter_map(|s| s.trim().parse().ok()).collect();
            }
            "--scale" => {
                i += 1;
                scale = args[i].parse().unwrap_or(2);
            }
            "--every" => {
                i += 1;
                every = args[i].parse().unwrap_or(0);
            }
            "--quiet" => quiet = true,
            "--wav" => {
                i += 1;
                wav_path = Some(PathBuf::from(&args[i]));
            }
            "--state" => state = true,
            "--nestest" => {
                i += 1;
                nestest = args[i].parse().unwrap_or(5000);
            }
            "--script" => {
                i += 1;
                for ev in args[i].split(',') {
                    let parts: Vec<&str> = ev.split(':').collect();
                    if parts.len() != 3 {
                        continue;
                    }
                    let f: u64 = parts[0].parse().unwrap_or(0);
                    let btn = match parts[1].to_ascii_lowercase().as_str() {
                        "a" => BUTTON_A,
                        "b" => BUTTON_B,
                        "start" => BUTTON_START,
                        "select" => BUTTON_SELECT,
                        "left" => BUTTON_LEFT,
                        "right" => BUTTON_RIGHT,
                        "up" => BUTTON_UP,
                        "down" => BUTTON_DOWN,
                        _ => 0,
                    };
                    let down = parts[2] == "1";
                    script_events.push((f, btn, down));
                }
            }
            other => eprintln!("ignoring unknown arg {other}"),
        }
        i += 1;
    }
    if frames.is_empty() {
        frames = vec![1, 30, 60, 120, 240, 360, 600, 900];
    }
    let max_frame = frames.iter().copied().max().unwrap_or(1).max(every);

    let cart = match Cartridge::load(&rom) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cartridge error: {e}");
            std::process::exit(1);
        }
    };
    if !quiet {
        println!(
            "ROM {} | PRG {} B | CHR {} B | mapper {} | {:?} | prg_sha256 {}",
            rom.display(),
            cart.prg.len(),
            cart.chr.len(),
            cart.mapper,
            cart.mirroring,
            cart.prg_sha256
        );
    }
    let mut console = Console::new(cart);
    std::fs::create_dir_all(&outdir).ok();

    if nestest > 0 {
        // nestest.nes: force the documented entry point and trace like nestest.log.
        console.cpu.pc = 0xC000;
        console.cpu.sp = 0xFD;
        console.cpu.p = 0x24;
        console.cpu.a = 0;
        console.cpu.x = 0;
        console.cpu.y = 0;
        console.cpu.cycles = 7;
        for _ in 0..nestest {
            let pc = console.cpu.pc;
            let op = console.bus.read_debug(pc);
            println!(
                "{pc:04X} {op:02X} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} CYC:{}",
                console.cpu.a, console.cpu.x, console.cpu.y, console.cpu.p, console.cpu.sp,
                console.cpu.cycles
            );
            console.cpu.step(&mut console.bus);
        }
        return;
    }

    let mut events = script_events;
    events.sort_by_key(|e| e.0);

    let mut next = 0usize;
    let mut buttons = 0u8;
    let mut pcm: Vec<f32> = Vec::new();
    for frame in 0..=max_frame {
        while next < events.len() && events[next].0 == frame {
            if events[next].2 {
                buttons |= events[next].1;
            } else {
                buttons &= !events[next].1;
            }
            next += 1;
        }
        console.set_buttons(0, buttons);

        if !console.step_frame() {
            eprintln!(
                "CPU jammed at frame {} pc=${:04X} opcode context lost",
                frame, console.cpu.pc
            );
            break;
        }

        if wav_path.is_some() {
            pcm.extend_from_slice(&console.bus.apu.samples);
            console.bus.apu.samples.clear();
        }

        let dump = frames.contains(&frame) || (every > 0 && frame % every == 0);
        if dump {
            let name = outdir.join(format!("frame_{frame:05}.png"));
            smb_rs::pngout::write_png(&name, console.framebuffer(), 256, 240, scale);
            let crc = frame_crc(console.framebuffer());
            if !quiet {
                println!("frame {frame:5} -> {}  crc={crc:08X}", name.display());
            }
        }
    }
    if !quiet {
        println!(
            "done: {} frames, CPU cycles {}, PPU frames {}",
            console.frames, console.cpu.cycles, console.bus.ppu.frame_count
        );
    }
    if state {
        dump_state(&console);
    }
    if let Some(path) = &wav_path {
        write_wav(path, &pcm);
        if !quiet {
            println!("wrote {} samples to {}", pcm.len(), path.display());
        }
    }
}

fn write_wav(path: &Path, samples: &[f32]) {
    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&44_100u32.to_le_bytes());
    out.extend_from_slice(&(44_100u32 * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    if let Err(e) = std::fs::write(path, &out) {
        eprintln!("wav write failed: {e}");
    }
}

fn dump_state(c: &smb_rs::Console) {
    let p = &c.bus.ppu;
    println!("--- PPU state ---");
    println!("ctrl={:02X} mask={:02X} status={:02X} v={:04X} t={:04X} fine_x={}", p.ctrl, p.mask, p.status, p.v, p.t, p.fine_x);
    println!("sprite0 first hit scanline: {}", p.sprite0_hit_at);
    println!("palette: {:02X?}", &p.palette[..]);
    for nt in 0..2 {
        println!("nametable physical {nt} (rows 0..30):");
        for row in 0..30 {
            let tiles: Vec<String> = (0..32)
                .map(|col| format!("{:02X}", p.vram[nt * 1024 + row * 32 + col]))
                .collect();
            println!("  row{row:02}: {}", tiles.join(" "));
        }
    }
    println!("OAM (y<240):");
    for i in 0..64 {
        let y = p.oam[i * 4];
        if y < 240 {
            println!(
                "  #{i:02} y={y:3} tile={:02X} attr={:02X} x={:3}",
                p.oam[i * 4 + 1],
                p.oam[i * 4 + 2],
                p.oam[i * 4 + 3]
            );
        }
    }
    println!("CPU pc=${:04X} a={:02X} x={:02X} y={:02X} sp={:02X} p={:02X}", c.cpu.pc, c.cpu.a, c.cpu.x, c.cpu.y, c.cpu.sp, c.cpu.p);
    println!("--- RAM $0700-$07FF (nonzero) ---");
    let mut line = String::new();
    for addr in 0x0700u16..=0x07FF {
        let v = c.bus.ram[(addr as usize) & 0x7FF];
        if v != 0 {
            line.push_str(&format!("{addr:04X}={v:02X} "));
        }
        if line.len() > 150 {
            println!("{line}");
            line.clear();
        }
    }
    if !line.is_empty() {
        println!("{line}");
    }
}

fn frame_crc(buf: &[u32]) -> u32 {
    let mut h: u32 = 0x811C9DC5;
    for &px in buf {
        for b in px.to_le_bytes() {
            h ^= b as u32;
            h = h.wrapping_mul(0x01000193);
        }
    }
    h
}

#[allow(dead_code)]
fn _unused(_p: &Path) {}
