//! Interactive frontend: minifb window + cpal audio.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use minifb::{Key, Scale, Window, WindowOptions};
use smb_rs::console::*;
use smb_rs::Cartridge;

#[cfg(feature = "audio")]
use smb_rs::audio::AudioOut;

const DEFAULT_ROM: &str = r"C:\Users\USUARIO\Downloads\Super Mario Bros. (World)\Super Mario Bros. (World).nes";
const FRAME_TIME: Duration = Duration::from_nanos(16_639_267); // 60.0988 Hz

fn main() {
    let mut args = std::env::args().skip(1);
    let rom_path = PathBuf::from(args.next().unwrap_or_else(|| DEFAULT_ROM.to_string()));
    // Optional window scale; with no argument the window is sized to the screen.
    let scale_arg = args.next().and_then(|s| s.parse::<usize>().ok());

    let cart = match Cartridge::load(&rom_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cartridge error: {e}");
            std::process::exit(1);
        }
    };
    println!("ROM   : {}", rom_path.display());
    println!("PRG   : {} bytes  sha256 {}", cart.prg.len(), cart.prg_sha256);
    println!("CHR   : {} bytes  mapper {}  mirroring {:?}", cart.chr.len(), cart.mapper, cart.mirroring);

    let mut console = Console::new(cart);

    let (win_w, win_h, win_scale) = match scale_arg {
        Some(s) if s >= 1 => (256 * s, 240 * s, Scale::X1),
        _ => (256, 240, Scale::FitScreen),
    };
    let mut window = Window::new(
        "Super Mario Bros. — Rust NES",
        win_w,
        win_h,
        WindowOptions { scale: win_scale, resize: true, ..Default::default() },
    )
    .expect("create window");
    window.set_target_fps(60);

    #[cfg(feature = "audio")]
    let audio = AudioOut::new();

    let mut paused = false;
    let mut last = Instant::now();
    let mut acc = Duration::ZERO;
    let mut screen: Vec<u32> = Vec::new();

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let mut buttons = 0u8;
        // WASD to move, Space to jump (A), Shift to run/fire (B).
        if window.is_key_down(Key::D) || window.is_key_down(Key::Right) { buttons |= BUTTON_RIGHT; }
        if window.is_key_down(Key::A) || window.is_key_down(Key::Left) { buttons |= BUTTON_LEFT; }
        if window.is_key_down(Key::W) || window.is_key_down(Key::Up) { buttons |= BUTTON_UP; }
        if window.is_key_down(Key::S) || window.is_key_down(Key::Down) { buttons |= BUTTON_DOWN; }
        if window.is_key_down(Key::Space) { buttons |= BUTTON_A; }
        if window.is_key_down(Key::RightShift) || window.is_key_down(Key::LeftShift) {
            buttons |= BUTTON_B;
        }
        if window.is_key_down(Key::Enter) { buttons |= BUTTON_START; }
        if window.is_key_down(Key::Tab) { buttons |= BUTTON_SELECT; }
        console.set_buttons(0, buttons);

        if window.is_key_pressed(Key::R, minifb::KeyRepeat::No) {
            console.reset();
            println!("reset");
        }
        if window.is_key_pressed(Key::P, minifb::KeyRepeat::No) {
            paused = !paused;
        }
        if window.is_key_pressed(Key::F12, minifb::KeyRepeat::No) {
            let name = format!("screenshot_{:05}.png", console.frames);
            smb_rs::pngout::write_png(std::path::Path::new(&name), console.framebuffer(), 256, 240, 2);
            println!("saved {name}");
        }

        let now = Instant::now();
        acc += now - last;
        last = now;
        if acc > FRAME_TIME * 4 {
            acc = FRAME_TIME * 4;
        }

        if !paused {
            while acc >= FRAME_TIME {
                if !console.step_frame() {
                    eprintln!("CPU jammed at pc=${:04X}", console.cpu.pc);
                    break;
                }
                acc -= FRAME_TIME;
                #[cfg(feature = "audio")]
                if let Some(a) = &audio {
                    let samples = std::mem::take(&mut console.bus.apu.samples);
                    a.push(&samples);
                }
            }
        }

        // Integer-scale the 256x240 frame to the current window size, keeping
        // the 4:3 aspect ratio and centering it with black bars. This makes the
        // window fill the screen at any size, including when maximized.
        let (ww, wh) = window.get_size();
        if ww > 0 && wh > 0 {
            let scale = (ww / 256).min(wh / 240).max(1);
            let dw = (256 * scale).min(ww);
            let dh = (240 * scale).min(wh);
            let ox = ww.saturating_sub(dw) / 2;
            let oy = wh.saturating_sub(dh) / 2;
            screen.clear();
            screen.resize(ww * wh, 0);
            let src = console.framebuffer();
            for y in 0..dh {
                let srow = (y / scale) * 256;
                let drow = (oy + y) * ww + ox;
                for x in 0..dw {
                    screen[drow + x] = src[srow + x / scale];
                }
            }
            if window.update_with_buffer(&screen, ww, wh).is_err() {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
