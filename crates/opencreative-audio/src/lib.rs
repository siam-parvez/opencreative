//! What the computer is playing, as a spectrum: the speakers' output is
//! recorded through PipeWire (`pw-record`, or PulseAudio's `parec`) and
//! split into frequency bands for music-synced screens and lights.
//!
//! Recording the output needs no extra permissions on a desktop session;
//! nothing is saved, only the last few milliseconds are kept for analysis.

use std::collections::VecDeque;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use thiserror::Error;

const RATE: u32 = 48_000;
/// Samples per analysis, about 43 ms: enough for bass notes, short enough
/// to keep up with beats.
const WINDOW: usize = 2048;
const LOW_HZ: f32 = 40.0;
const HIGH_HZ: f32 = 16_000.0;
/// How many decibels below the loudest recent band count as silence.
const RANGE_DB: f32 = 55.0;
/// Quieter than this is treated as silence, however long it lasts.
const SILENCE_DB: f32 = -75.0;
/// How fast a bar falls back, per second, as a fraction of its height.
const FALL_PER_SEC: f32 = 2.2;
/// How fast a peak cap falls, per second.
const PEAK_FALL_PER_SEC: f32 = 0.6;
/// After this long without samples, the recorder is assumed stuck.
const STALL: Duration = Duration::from_secs(3);

#[derive(Debug, Error)]
pub enum AudioError {
    #[error(
        "can't record what's playing: neither pw-record (PipeWire) nor parec \
         (PulseAudio) could be started"
    )]
    NoRecorder,
    #[error("the audio recorder stopped")]
    Stopped,
}

/// One analysis: bar heights and the peak caps above them, all 0 to 1.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bands {
    pub levels: Vec<f32>,
    pub peaks: Vec<f32>,
}

impl Bands {
    /// Overall loudness, 0 to 1, weighted toward the bass (for lights).
    pub fn energy(&self) -> f32 {
        if self.levels.is_empty() {
            return 0.0;
        }
        let n = self.levels.len() as f32;
        let (sum, weight) = self
            .levels
            .iter()
            .enumerate()
            .fold((0.0, 0.0), |(s, w), (i, v)| {
                let k = 1.0 - 0.6 * i as f32 / n;
                (s + v * k, w + k)
            });
        sum / weight
    }
}

/// Records the speakers' output and turns it into bands on request.
pub struct Spectrum {
    child: Child,
    samples: Arc<Mutex<Shared>>,
    fft: Arc<dyn Fft<f32>>,
    hann: Vec<f32>,
    /// The loudest band lately, in dB, so quiet and loud music both fill
    /// the bars.
    ceiling_db: f32,
    last: Option<(Instant, Bands)>,
}

struct Shared {
    recent: VecDeque<f32>,
    /// When samples last arrived.
    fed: Instant,
    ended: bool,
}

impl Spectrum {
    pub fn start() -> Result<Self, AudioError> {
        let mut child = recorder()?;
        let stdout = child.stdout.take().ok_or(AudioError::NoRecorder)?;
        let samples = Arc::new(Mutex::new(Shared {
            recent: VecDeque::with_capacity(WINDOW * 2),
            fed: Instant::now(),
            ended: false,
        }));
        let shared = Arc::clone(&samples);
        thread::Builder::new()
            .name("audio-capture".into())
            .spawn(move || feed(stdout, &shared))
            .map_err(|_| AudioError::NoRecorder)?;
        Ok(Self {
            child,
            samples,
            fft: FftPlanner::new().plan_fft_forward(WINDOW),
            hann: hann(),
            ceiling_db: SILENCE_DB + RANGE_DB,
            last: None,
        })
    }

    /// The spectrum right now in `count` bands, low to high, smoothed so
    /// bars rise at once and fall gently.
    pub fn bands(&mut self, count: usize) -> Result<Bands, AudioError> {
        let window: Vec<f32> = {
            let shared = self.samples.lock().expect("audio lock");
            if shared.ended || shared.fed.elapsed() > STALL {
                return Err(AudioError::Stopped);
            }
            let skip = shared.recent.len().saturating_sub(WINDOW);
            shared.recent.iter().skip(skip).copied().collect()
        };
        let db = self.analyse(&window, count);

        // Follow the loudest band: up at once, down slowly.
        let loudest = db.iter().copied().fold(SILENCE_DB, f32::max);
        self.ceiling_db = if loudest > self.ceiling_db {
            loudest
        } else {
            (self.ceiling_db - 0.15).max(SILENCE_DB + RANGE_DB * 0.5)
        };
        let floor = self.ceiling_db - RANGE_DB;
        let fresh: Vec<f32> = db
            .iter()
            .map(|d| ((d - floor) / RANGE_DB).clamp(0.0, 1.0))
            .collect();

        let now = Instant::now();
        let bands = match self.last.take() {
            Some((then, prev)) if prev.levels.len() == count => {
                let dt = now.duration_since(then).as_secs_f32().min(0.25);
                let levels: Vec<f32> = fresh
                    .iter()
                    .zip(&prev.levels)
                    .map(|(&new, &old)| new.max(old - FALL_PER_SEC * dt * old.max(0.2)).max(0.0))
                    .collect();
                let peaks = levels
                    .iter()
                    .zip(&prev.peaks)
                    .map(|(&level, &peak)| level.max(peak - PEAK_FALL_PER_SEC * dt))
                    .collect();
                Bands { levels, peaks }
            }
            _ => Bands {
                peaks: fresh.clone(),
                levels: fresh,
            },
        };
        self.last = Some((now, bands.clone()));
        Ok(bands)
    }

    /// Loudness of each band in dB.
    fn analyse(&self, window: &[f32], count: usize) -> Vec<f32> {
        let mut buf: Vec<Complex<f32>> = (0..WINDOW)
            .map(|i| {
                // Too few samples yet: pad with silence at the start.
                let s = (i + window.len())
                    .checked_sub(WINDOW)
                    .map_or(0.0, |j| window[j]);
                Complex::new(s * self.hann[i], 0.0)
            })
            .collect();
        self.fft.process(&mut buf);
        let bin_hz = RATE as f32 / WINDOW as f32;
        // A full-scale sine comes out at 0 dB.
        let scale = 4.0 / WINDOW as f32;
        band_edges(count)
            .windows(2)
            .map(|edge| {
                let lo = ((edge[0] / bin_hz).floor() as usize).max(1);
                let hi = ((edge[1] / bin_hz).ceil() as usize).clamp(lo + 1, WINDOW / 2);
                let amplitude = buf[lo..hi]
                    .iter()
                    .map(|c| c.norm() * scale)
                    .fold(0.0f32, f32::max);
                20.0 * amplitude.max(1e-9).log10()
            })
            .collect()
    }
}

impl Drop for Spectrum {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn hann() -> Vec<f32> {
    (0..WINDOW)
        .map(|i| {
            let x = std::f32::consts::TAU * i as f32 / (WINDOW - 1) as f32;
            0.5 - 0.5 * x.cos()
        })
        .collect()
}

/// `count + 1` band edges in Hz, evenly spaced on a log scale, the way
/// ears hear pitch.
fn band_edges(count: usize) -> Vec<f32> {
    let ratio = HIGH_HZ / LOW_HZ;
    (0..=count)
        .map(|i| LOW_HZ * ratio.powf(i as f32 / count.max(1) as f32))
        .collect()
}

/// Starts recording the default output as mono 32-bit floats on stdout.
fn recorder() -> Result<Child, AudioError> {
    let rate = RATE.to_string();
    let attempts: [(&str, Vec<&str>); 2] = [
        (
            "pw-record",
            vec![
                "-P",
                "{ stream.capture.sink=true node.name=opencreative-visualizer }",
                "--format",
                "f32",
                "--rate",
                &rate,
                "--channels",
                "1",
                "-",
            ],
        ),
        (
            "parec",
            vec![
                "-d",
                "@DEFAULT_MONITOR@",
                "--format=float32le",
                "--rate",
                &rate,
                "--channels=1",
                "--latency-msec=20",
                "--client-name=OpenCreative",
            ],
        ),
    ];
    for (program, args) in attempts {
        match Command::new(program)
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => {
                log::info!("recording the speakers with {program}");
                return Ok(child);
            }
            Err(e) => log::debug!("{program}: {e}"),
        }
    }
    Err(AudioError::NoRecorder)
}

/// Reads samples until the recorder exits, keeping the latest window.
fn feed(mut stdout: impl Read, shared: &Mutex<Shared>) {
    let mut bytes = [0u8; 4096];
    let mut carry: Vec<u8> = Vec::with_capacity(4100);
    loop {
        let n = match stdout.read(&mut bytes) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        carry.extend_from_slice(&bytes[..n]);
        let whole = carry.len() / 4 * 4;
        let mut shared = shared.lock().expect("audio lock");
        for chunk in carry[..whole].as_chunks::<4>().0 {
            let s = f32::from_le_bytes(*chunk);
            shared.recent.push_back(if s.is_finite() { s } else { 0.0 });
        }
        let excess = shared.recent.len().saturating_sub(WINDOW);
        shared.recent.drain(..excess);
        shared.fed = Instant::now();
        drop(shared);
        carry.drain(..whole);
    }
    shared.lock().expect("audio lock").ended = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spectrum_for_test() -> Spectrum {
        // `true` stands in for the recorder; the samples are fed by hand.
        let child = Command::new("true").spawn().unwrap();
        Spectrum {
            child,
            samples: Arc::new(Mutex::new(Shared {
                recent: VecDeque::new(),
                fed: Instant::now(),
                ended: false,
            })),
            fft: FftPlanner::new().plan_fft_forward(WINDOW),
            hann: hann(),
            ceiling_db: SILENCE_DB + RANGE_DB,
            last: None,
        }
    }

    #[test]
    fn edges_span_the_range_on_a_log_scale() {
        let edges = band_edges(4);
        assert_eq!(edges.len(), 5);
        assert!((edges[0] - LOW_HZ).abs() < 0.01);
        assert!((edges[4] - HIGH_HZ).abs() < 1.0);
        let r1 = edges[1] / edges[0];
        let r2 = edges[3] / edges[2];
        assert!((r1 - r2).abs() < 1e-3);
    }

    #[test]
    fn a_tone_lights_its_own_band() {
        let mut s = spectrum_for_test();
        let tone = (0..WINDOW)
            .map(|i| 0.5 * (std::f32::consts::TAU * 1000.0 * i as f32 / RATE as f32).sin());
        s.samples.lock().unwrap().recent.extend(tone);
        let bands = s.bands(16).unwrap();
        let loudest = (0..16)
            .max_by(|&a, &b| bands.levels[a].total_cmp(&bands.levels[b]))
            .unwrap();
        let edges = band_edges(16);
        assert!(
            edges[loudest] <= 1100.0 && edges[loudest + 1] >= 900.0,
            "band {loudest} ({}-{} Hz)",
            edges[loudest],
            edges[loudest + 1]
        );
        assert!(bands.levels[0] < 0.3, "{:?}", bands.levels);
    }

    #[test]
    fn silence_is_flat_and_a_stall_is_an_error() {
        let mut s = spectrum_for_test();
        s.samples.lock().unwrap().recent.extend(vec![0.0; WINDOW]);
        let bands = s.bands(8).unwrap();
        assert!(bands.levels.iter().all(|&v| v == 0.0), "{bands:?}");
        s.samples.lock().unwrap().ended = true;
        assert!(matches!(s.bands(8), Err(AudioError::Stopped)));
    }
}
