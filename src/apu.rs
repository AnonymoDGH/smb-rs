//! NES APU: two pulses, triangle, noise, frame sequencer, resampling to 44.1 kHz.
//! DMC is stubbed (Super Mario Bros. never enables it).

pub const CPU_HZ: f64 = 1_789_773.0;
pub const SAMPLE_HZ: f64 = 44_100.0;

const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
const DUTY: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0],
    [0, 1, 1, 0, 0, 0, 0, 0],
    [0, 1, 1, 1, 1, 0, 0, 0],
    [1, 0, 0, 1, 1, 1, 1, 1],
];
const TRIANGLE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
    12, 13, 14, 15,
];
const NOISE_PERIODS: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];

#[derive(Default, Clone)]
struct Envelope {
    start: bool,
    divider: u8,
    decay: u8,
    constant: bool,
    /// Length-counter halt bit ($4000/$4004/$400C bit 5); it also loops the envelope.
    loop_flag: bool,
    volume: u8,
}

impl Envelope {
    fn clock(&mut self) {
        if self.start {
            self.start = false;
            self.decay = 15;
            self.divider = self.volume;
        } else if self.divider == 0 {
            self.divider = self.volume;
            if self.decay > 0 {
                self.decay -= 1;
            } else if self.loop_flag {
                self.decay = 15;
            }
        } else {
            self.divider -= 1;
        }
    }
    fn output(&self) -> u8 {
        if self.constant {
            self.volume
        } else {
            self.decay
        }
    }
}

#[derive(Default, Clone)]
struct Pulse {
    enabled: bool,
    duty: u8,
    seq: u8,
    timer: u16,
    period: u16,
    length: u8,
    halt: bool,
    env: Envelope,
    sweep_enabled: bool,
    sweep_period: u8,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_divider: u8,
    sweep_reload: bool,
    is_pulse2: bool,
}

impl Pulse {
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.period;
            self.seq = (self.seq + 1) & 7;
        } else {
            self.timer -= 1;
        }
    }
    fn clock_length(&mut self) {
        if !self.halt && self.length > 0 {
            self.length -= 1;
        }
    }
    fn clock_sweep(&mut self) {
        if self.sweep_divider == 0 && self.sweep_enabled && self.sweep_shift > 0 {
            let delta = self.period >> self.sweep_shift;
            let target = if self.sweep_negate {
                self.period.wrapping_sub(delta + if self.is_pulse2 { 0 } else { 1 })
            } else {
                self.period + delta
            };
            if self.period >= 8 && target <= 0x7FF {
                self.period = target & 0x7FF;
            }
            self.sweep_divider = self.sweep_period;
        } else if self.sweep_divider > 0 {
            self.sweep_divider -= 1;
        } else {
            self.sweep_divider = self.sweep_period;
        }
        if self.sweep_reload {
            self.sweep_reload = false;
            self.sweep_divider = self.sweep_period;
        }
    }
    fn output(&self) -> u8 {
        if !self.enabled || self.length == 0 || self.period < 8 || self.period > 0x7FF {
            return 0;
        }
        if DUTY[self.duty as usize][self.seq as usize] == 0 {
            return 0;
        }
        self.env.output()
    }
}

#[derive(Default, Clone)]
struct Triangle {
    enabled: bool,
    timer: u16,
    period: u16,
    seq: u8,
    length: u8,
    halt: bool,
    linear_reload: bool,
    linear_counter: u8,
    linear_reload_value: u8,
    control: bool,
}

impl Triangle {
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.period;
            if self.length > 0 && self.linear_counter > 0 {
                self.seq = (self.seq + 1) & 31;
            }
        } else {
            self.timer -= 1;
        }
    }
    fn clock_linear(&mut self) {
        if self.linear_reload {
            self.linear_counter = self.linear_reload_value;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }
        if !self.control {
            self.linear_reload = false;
        }
    }
    fn clock_length(&mut self) {
        if !self.halt && self.length > 0 {
            self.length -= 1;
        }
    }
    fn output(&self) -> u8 {
        if !self.enabled || self.length == 0 || self.linear_counter == 0 || self.period < 2 {
            return 0;
        }
        TRIANGLE[self.seq as usize]
    }
}

#[derive(Default, Clone)]
struct Noise {
    enabled: bool,
    timer: u16,
    period: u16,
    shift: u16,
    mode: bool,
    length: u8,
    halt: bool,
    env: Envelope,
}

impl Noise {
    fn clock_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.period;
            let bit = if self.mode { 6 } else { 1 };
            let feedback = (self.shift & 1) ^ ((self.shift >> bit) & 1);
            self.shift = (self.shift >> 1) | (feedback << 14);
        } else {
            self.timer -= 1;
        }
    }
    fn clock_length(&mut self) {
        if !self.halt && self.length > 0 {
            self.length -= 1;
        }
    }
    fn output(&self) -> u8 {
        if !self.enabled || self.length == 0 || self.shift & 1 != 0 {
            return 0;
        }
        self.env.output()
    }
}

pub struct Apu {
    pulse1: Pulse,
    pulse2: Pulse,
    triangle: Triangle,
    noise: Noise,
    frame_cycle: u32,
    frame_mode5: bool,
    /// Half-rate APU phase: pulse and noise timers tick every second CPU cycle.
    apu_phase: bool,
    frame_irq_inhibit: bool,
    frame_irq: bool,
    sample_acc: f64,
    pub samples: Vec<f32>,
    pub master_volume: f32,
}

impl Apu {
    pub fn new() -> Apu {
        Apu {
            pulse1: Pulse { is_pulse2: false, ..Default::default() },
            pulse2: Pulse { is_pulse2: true, ..Default::default() },
            triangle: Triangle { seq: 0, ..Default::default() },
            noise: Noise { shift: 1, ..Default::default() },
            frame_cycle: 0,
            frame_mode5: false,
            apu_phase: false,
            frame_irq_inhibit: false,
            frame_irq: false,
            sample_acc: 0.0,
            samples: Vec::with_capacity(4096),
            master_volume: 0.35,
        }
    }

    pub fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x4000 => {
                self.pulse1.duty = val >> 6;
                self.pulse1.halt = val & 0x20 != 0;
                self.pulse1.env.loop_flag = val & 0x20 != 0;
                self.pulse1.env.constant = val & 0x10 != 0;
                self.pulse1.env.volume = val & 0x0F;
            }
            0x4001 => {
                self.pulse1.sweep_enabled = val & 0x80 != 0;
                self.pulse1.sweep_period = (val >> 4) & 7;
                self.pulse1.sweep_negate = val & 0x08 != 0;
                self.pulse1.sweep_shift = val & 7;
                self.pulse1.sweep_reload = true;
            }
            0x4002 => {
                self.pulse1.period = (self.pulse1.period & 0x700) | val as u16;
            }
            0x4003 => {
                self.pulse1.period = (self.pulse1.period & 0x0FF) | (((val & 7) as u16) << 8);
                if self.pulse1.enabled {
                    self.pulse1.length = LENGTH_TABLE[(val >> 3) as usize];
                }
                self.pulse1.seq = 0;
                self.pulse1.env.start = true;
            }
            0x4004 => {
                self.pulse2.duty = val >> 6;
                self.pulse2.halt = val & 0x20 != 0;
                self.pulse2.env.loop_flag = val & 0x20 != 0;
                self.pulse2.env.constant = val & 0x10 != 0;
                self.pulse2.env.volume = val & 0x0F;
            }
            0x4005 => {
                self.pulse2.sweep_enabled = val & 0x80 != 0;
                self.pulse2.sweep_period = (val >> 4) & 7;
                self.pulse2.sweep_negate = val & 0x08 != 0;
                self.pulse2.sweep_shift = val & 7;
                self.pulse2.sweep_reload = true;
            }
            0x4006 => {
                self.pulse2.period = (self.pulse2.period & 0x700) | val as u16;
            }
            0x4007 => {
                self.pulse2.period = (self.pulse2.period & 0x0FF) | (((val & 7) as u16) << 8);
                if self.pulse2.enabled {
                    self.pulse2.length = LENGTH_TABLE[(val >> 3) as usize];
                }
                self.pulse2.seq = 0;
                self.pulse2.env.start = true;
            }
            0x4008 => {
                self.triangle.control = val & 0x80 != 0;
                self.triangle.linear_reload_value = val & 0x7F;
            }
            0x400A => {
                self.triangle.period = (self.triangle.period & 0x700) | val as u16;
            }
            0x400B => {
                self.triangle.period = (self.triangle.period & 0x0FF) | (((val & 7) as u16) << 8);
                if self.triangle.enabled {
                    self.triangle.length = LENGTH_TABLE[(val >> 3) as usize];
                }
                self.triangle.linear_reload = true;
            }
            0x400C => {
                self.noise.halt = val & 0x20 != 0;
                self.noise.env.loop_flag = val & 0x20 != 0;
                self.noise.env.constant = val & 0x10 != 0;
                self.noise.env.volume = val & 0x0F;
            }
            0x400E => {
                self.noise.mode = val & 0x80 != 0;
                self.noise.period = NOISE_PERIODS[(val & 0x0F) as usize];
            }
            0x400F => {
                if self.noise.enabled {
                    self.noise.length = LENGTH_TABLE[(val >> 3) as usize];
                }
                self.noise.env.start = true;
            }
            0x4015 => {
                self.pulse1.enabled = val & 1 != 0;
                self.pulse2.enabled = val & 2 != 0;
                self.triangle.enabled = val & 4 != 0;
                self.noise.enabled = val & 8 != 0;
                if val & 1 == 0 {
                    self.pulse1.length = 0;
                }
                if val & 2 == 0 {
                    self.pulse2.length = 0;
                }
                if val & 4 == 0 {
                    self.triangle.length = 0;
                }
                if val & 8 == 0 {
                    self.noise.length = 0;
                }
            }
            0x4017 => {
                self.frame_mode5 = val & 0x80 != 0;
                self.frame_irq_inhibit = val & 0x40 != 0;
                if self.frame_irq_inhibit {
                    self.frame_irq = false;
                }
                self.frame_cycle = 0;
            }
            _ => {}
        }
    }

    pub fn irq(&self) -> bool {
        self.frame_irq
    }

    /// Reading $4015 acknowledges the frame-counter IRQ.
    pub fn clear_frame_irq(&mut self) {
        self.frame_irq = false;
    }

    pub fn step(&mut self, cycles: u32) {
        for _ in 0..cycles {
            self.clock_frame_sequencer();

            // Pulse and noise timers count APU cycles (1.79 MHz), i.e. one tick
            // every two CPU cycles; the triangle timer advances every CPU cycle.
            if self.apu_phase {
                self.pulse1.clock_timer();
                self.pulse2.clock_timer();
                self.noise.clock_timer();
            }
            self.apu_phase = !self.apu_phase;
            self.triangle.clock_timer();

            self.sample_acc += SAMPLE_HZ / CPU_HZ;
            if self.sample_acc >= 1.0 {
                self.sample_acc -= 1.0;
                let s = self.mix();
                self.samples.push(s);
            }
        }
    }

    fn clock_frame_sequencer(&mut self) {
        self.frame_cycle += 1;
        if self.frame_mode5 {
            // 5-step: quarter at 7457/14913/22371/37281, half at 14913/37281,
            // no IRQ, and the sequence restarts after 37281 CPU cycles.
            match self.frame_cycle {
                7457 | 22371 => self.quarter_frame(),
                14913 | 37281 => {
                    self.quarter_frame();
                    self.half_frame();
                }
                _ => {}
            }
            if self.frame_cycle >= 37282 {
                self.frame_cycle = 0;
            }
        } else {
            // 4-step: quarter at 7457/14913/22371/29829, half at 14913/29829,
            // and the frame IRQ is raised at 29829 unless inhibited.
            match self.frame_cycle {
                7457 | 22371 => self.quarter_frame(),
                14913 | 29829 => {
                    self.quarter_frame();
                    self.half_frame();
                }
                _ => {}
            }
            if self.frame_cycle >= 29830 {
                self.frame_cycle = 0;
                if !self.frame_irq_inhibit {
                    self.frame_irq = true;
                }
            }
        }
    }

    fn quarter_frame(&mut self) {
        self.pulse1.env.clock();
        self.pulse2.env.clock();
        self.noise.env.clock();
        self.triangle.clock_linear();
    }

    fn half_frame(&mut self) {
        self.pulse1.clock_length();
        self.pulse2.clock_length();
        self.triangle.clock_length();
        self.noise.clock_length();
        self.pulse1.clock_sweep();
        self.pulse2.clock_sweep();
    }

    fn mix(&self) -> f32 {
        let p1 = self.pulse1.output() as f32;
        let p2 = self.pulse2.output() as f32;
        let t = self.triangle.output() as f32;
        let n = self.noise.output() as f32;
        let pulse_out = if p1 + p2 > 0.0 {
            95.88 / (8128.0 / (p1 + p2) + 100.0)
        } else {
            0.0
        };
        let tnd = if t > 0.0 || n > 0.0 {
            159.79 / (1.0 / (t / 8227.0 + n / 12241.0) + 100.0)
        } else {
            0.0
        };
        ((pulse_out + tnd) * self.master_volume).clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Count how many times the pulse-1 sequencer steps while `cycles` CPU
    /// cycles elapse.
    fn pulse_steps(apu: &mut Apu, cycles: u32) -> u32 {
        let mut steps = 0;
        let mut last = apu.pulse1.seq;
        for _ in 0..cycles {
            apu.step(1);
            if apu.pulse1.seq != last {
                steps += 1;
                last = apu.pulse1.seq;
            }
        }
        steps
    }

    fn apu_with_pulse(period: u16) -> Apu {
        let mut apu = Apu::new();
        apu.write(0x4015, 0x01); // enable pulse 1
        apu.write(0x4000, 0x00); // no loop, no constant volume, volume 0
        apu.write(0x4002, (period & 0xFF) as u8);
        apu.write(0x4003, ((period >> 8) & 7) as u8);
        apu
    }

    #[test]
    fn pulse_timer_runs_at_apu_rate() {
        // Period 253 => one sequencer step every (253 + 1) * 2 = 508 CPU cycles.
        let mut apu = apu_with_pulse(253);
        let steps = pulse_steps(&mut apu, 8 * 508);
        assert!(
            (7..=9).contains(&steps),
            "pulse stepped {steps} times in 4064 CPU cycles (expected ~8)"
        );
    }

    #[test]
    fn frame_sequencer_4step_timing() {
        let mut apu = apu_with_pulse(100);
        apu.step(29830);
        // Quarter frames at 7457/14913/22371/29829: the first consumes the
        // start flag, the other three decrement the decay.
        assert_eq!(apu.pulse1.env.decay, 12);
        assert!(apu.irq(), "the 4-step sequence must set the frame IRQ at 29829");
    }

    #[test]
    fn frame_sequencer_5step_timing() {
        let mut apu = apu_with_pulse(100);
        apu.write(0x4017, 0x80);
        apu.write(0x4003, 0x00);
        apu.step(37282);
        // Quarter frames at 7457/14913/22371/37281, and no frame IRQ.
        assert_eq!(apu.pulse1.env.decay, 12);
        assert!(!apu.irq());
    }

    #[test]
    fn envelope_loops_on_halt_bit() {
        let mut env = Envelope { start: true, volume: 0, constant: false, ..Default::default() };
        env.clock(); // start: decay = 15
        for _ in 0..15 {
            env.clock();
        }
        assert_eq!(env.decay, 0);
        env.clock();
        assert_eq!(env.decay, 0, "without the loop bit a flat envelope stays silent");
        env.loop_flag = true;
        env.clock();
        assert_eq!(env.decay, 15, "the loop bit restarts the envelope");
    }
}
