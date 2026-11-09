//! Piloto heurístico base (el mismo controlador balístico que `playbot`).
//!
//! Se usa para recorrer 1-1 y construir el mapa, y como línea base contra la
//! que comparar la ruta encontrada por `deepscan`. No inyecta trucos: solo
//! corre, salta pozos/tuberías/enemigos y se desatasca.

use crate::console::{BUTTON_A, BUTTON_B, BUTTON_LEFT, BUTTON_RIGHT};
use crate::route::sensors::scan_environment;

pub struct Pilot {
    jump_hold: usize,
    jump_cooldown: usize,
    target_clear_y: i32,
    jumping_over_pit: bool,
    air_frames: usize,
    prev_x: i32,
    stuck_cnt: usize,
    unstick_cnt: usize,
    pub last_buttons: u8,
    pub jumps: u32,
    pub unstucks: u32,
}

impl Default for Pilot {
    fn default() -> Self {
        Self::new()
    }
}

impl Pilot {
    pub fn new() -> Self {
        Self {
            jump_hold: 0,
            jump_cooldown: 0,
            target_clear_y: 0,
            jumping_over_pit: false,
            air_frames: 0,
            prev_x: 0,
            stuck_cnt: 0,
            unstick_cnt: 0,
            last_buttons: 0,
            jumps: 0,
            unstucks: 0,
        }
    }

    pub fn step(&mut self, ram: &[u8; 2048]) -> u8 {
        let x = (ram[0x6D] as i32) * 256 + (ram[0x86] as i32);
        let vx = (ram[0x57] as i8) as i32;
        let player_state = ram[0x001D] as i32;
        let on_ground = player_state == 0;

        if on_ground {
            self.air_frames = 0;
        } else {
            self.air_frames += 1;
        }

        // Detector de atasco (escalera grande de 1-1 en x≈2914).
        if on_ground && x == self.prev_x && vx.abs() < 6 {
            self.stuck_cnt += 1;
        } else {
            self.stuck_cnt = 0;
        }
        self.prev_x = x;

        if self.stuck_cnt > 40 && self.unstick_cnt == 0 {
            self.unstick_cnt = 38;
            self.unstucks += 1;
        }

        let mut buttons;
        if self.unstick_cnt > 0 {
            self.unstick_cnt -= 1;
            self.jump_hold = 0;
            if self.unstick_cnt > 28 {
                buttons = BUTTON_LEFT;
            } else if self.unstick_cnt > 26 {
                buttons = 0;
            } else {
                buttons = BUTTON_RIGHT | BUTTON_B | BUTTON_A;
            }
            self.last_buttons = buttons;
            return buttons;
        }

        buttons = BUTTON_RIGHT | BUTTON_B;

        let feet = {
            let ypix = ram[0x03B8] as i32;
            let power = ram[0x0756];
            (ypix + if power == 0 { 32 } else { 48 }).clamp(32, 239)
        };
        let scan = scan_environment(ram, x, feet);

        if on_ground {
            self.jumping_over_pit = false;
            let mut desired_hold: usize = 0;

            let pipe_takeoff = (14 + vx).clamp(24, 52);
            let enemy_takeoff = (10 + (vx * 3) / 4).clamp(20, 38);

            if scan.pit_dist <= 38 && scan.pit_width < 96 {
                desired_hold = (7 + (scan.pit_width as usize) / 4).clamp(7, 20);
                self.jumping_over_pit = true;
                self.target_clear_y = feet - 24;
            } else if scan.wall_dist <= pipe_takeoff {
                if vx < 12 {
                    desired_hold = 23;
                } else if scan.wall_height <= 18 {
                    desired_hold = 12;
                } else if scan.wall_height <= 34 {
                    desired_hold = 16;
                } else if scan.wall_height <= 50 {
                    desired_hold = 20;
                } else {
                    desired_hold = 25;
                }
                self.target_clear_y = scan.wall_top_y - 2;
            } else if scan.enemy_dist >= 0 && scan.enemy_dist <= enemy_takeoff {
                desired_hold = if scan.low_ceiling { 4 } else { 6 };
                self.target_clear_y = feet - scan.enemy_height - 4;
            } else if vx == 0 && scan.wall_dist < 64 {
                desired_hold = 18;
                self.target_clear_y = scan.wall_top_y - 4;
            }

            if scan.low_ceiling && scan.wall_height <= 18 && !self.jumping_over_pit {
                desired_hold = desired_hold.min(5);
            }

            if desired_hold > 0 && self.jump_hold == 0 && self.jump_cooldown == 0 {
                self.jump_hold = desired_hold;
                self.jumps += 1;
            }
        } else {
            let predicted_feet = feet + (ram[0x9F] as i8 as i32) * 3;
            if self.jump_hold > 0
                && !self.jumping_over_pit
                && self.air_frames >= 4
                && (ram[0x9F] as i8) < 0
                && predicted_feet <= self.target_clear_y
                && scan.wall_dist > 14
            {
                self.jump_hold = 0;
            }
        }

        if self.jump_cooldown > 0 {
            self.jump_cooldown -= 1;
        }
        if self.jump_hold > 0 {
            buttons |= BUTTON_A;
            self.jump_hold -= 1;
            if self.jump_hold == 0 {
                self.jump_cooldown = 3;
            }
        }

        self.last_buttons = buttons;
        buttons
    }
}
