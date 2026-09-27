//! The phones' mix on its own low-latency output stream to the Mac's default speakers.

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use kara_core::mic::{Mixer, MOST_FRAMES};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

const FRAMES: u32 = 256;

/// Keeps the stream playing until dropped.
pub struct Output {
    _stop: mpsc::Sender<()>,
    broken: Arc<AtomicBool>,
}

impl Output {
    /// Whether the stream stopped with an error, e.g. its device went away.
    pub fn broken(&self) -> bool {
        self.broken.load(Ordering::Relaxed)
    }
}

/// Starts playing a new Mixer on the default output device; returns the mixer to feed.
pub fn start(floor_ms: f64) -> Result<(Output, Arc<Mutex<Mixer>>)> {
    let (ready_tx, ready_rx) = mpsc::channel();
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let broken = Arc::new(AtomicBool::new(false));
    let report = broken.clone();
    std::thread::spawn(move || match open(floor_ms, report) {
        Ok((stream, mixer)) => {
            let _ = ready_tx.send(Ok(mixer));
            let _ = stop_rx.recv();
            drop(stream);
        }
        Err(e) => {
            let _ = ready_tx.send(Err(e));
        }
    });
    let mixer = ready_rx.recv()??;
    Ok((Output { _stop: stop_tx, broken }, mixer))
}

/// Opens the default output with a callback that only locks the mixer and renders, in blocks the mixer takes without allocating.
fn open(floor_ms: f64, broken: Arc<AtomicBool>) -> Result<(cpal::Stream, Arc<Mutex<Mixer>>)> {
    let device = cpal::default_host().default_output_device().context("no output device")?;
    let config = device.default_output_config()?;
    let channels = usize::from(config.channels());
    let rate = config.sample_rate();
    let buffer_size = match config.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } if (*min..=*max).contains(&FRAMES) => cpal::BufferSize::Fixed(FRAMES),
        _ => cpal::BufferSize::Default,
    };
    let mixer = Arc::new(Mutex::new(Mixer::new(rate.0, floor_ms)));
    let feed = mixer.clone();
    let mut mono = vec![0.0; MOST_FRAMES];
    let stream = device.build_output_stream(
        &cpal::StreamConfig { channels: config.channels(), sample_rate: rate, buffer_size },
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            for block in data.chunks_mut(channels * MOST_FRAMES) {
                let mono = &mut mono[..block.len() / channels];
                feed.lock().unwrap().render(mono);
                for (frame, &x) in block.chunks_mut(channels).zip(mono.iter()) {
                    frame.fill(x);
                }
            }
        },
        move |_| broken.store(true, Ordering::Relaxed),
        None,
    )?;
    stream.play()?;
    Ok((stream, mixer))
}
