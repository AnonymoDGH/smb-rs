//! cpal output stream fed by the APU ring buffer.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub struct AudioOut {
    pub queue: Arc<Mutex<VecDeque<f32>>>,
    _stream: cpal::Stream,
}

impl AudioOut {
    pub fn new() -> Option<AudioOut> {
        let host = cpal::default_host();
        let device = host.default_output_device()?;
        let supported = device.default_output_config().ok()?;
        let sample_rate = supported.sample_rate().0;
        let channels = supported.channels() as usize;
        let config: cpal::StreamConfig = supported.into();

        let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::with_capacity(16384)));
        let q = queue.clone();
        let mut frac = 0.0f64;
        let step = crate::apu::SAMPLE_HZ / sample_rate as f64;

        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    let mut q = q.lock().unwrap();
                    for frame in data.chunks_mut(channels) {
                        let s = if q.is_empty() {
                            0.0
                        } else {
                            let a = q[0];
                            frac += step;
                            while frac >= 1.0 {
                                q.pop_front();
                                frac -= 1.0;
                            }
                            a
                        };
                        for ch in frame.iter_mut() {
                            *ch = s;
                        }
                    }
                },
                |err| eprintln!("audio stream error: {err}"),
                None,
            )
            .ok()?;
        stream.play().ok()?;
        Some(AudioOut { queue, _stream: stream })
    }

    pub fn push(&self, samples: &[f32]) {
        let mut q = self.queue.lock().unwrap();
        for &s in samples {
            if q.len() > 32768 {
                q.pop_front();
            }
            q.push_back(s);
        }
    }
}
