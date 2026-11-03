//! The capture's rules that are not the engine's (spec 007): the rate check, the capture folder,
//! the input-gap counter, the monitor's fill with its latency ceiling, and the digital-silence
//! watch. The input callback's body is `live::LiveCore::push`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rtrb::Consumer;

/// Whether a device's nominal rate is one of the engine's host rates (D1); `None` when it is,
/// otherwise the refusal in words.
pub fn rate_refusal(rate_hz: f64) -> Option<String> {
    let ok = rr_dr60::SUPPORTED_HOST_RATES
        .iter()
        .any(|&r| f64::from(r) == rate_hz);
    (!ok).then(|| {
        format!(
            "The input device runs at {rate_hz} Hz, which the engine does not take ({}): set the device's rate in Audio MIDI Setup",
            rr_dr60::SUPPORTED_HOST_RATES
                .iter()
                .map(|r| format!("{} Hz", r))
                .collect::<Vec<_>>()
                .join(", ")
        )
    })
}

/// The capture folder (coach review 11): `~/Library/Application Support/rr-dr60-workbench/captures`,
/// or under the temporary folder when there is no home.
pub fn folder() -> PathBuf {
    std::env::var_os("HOME").map_or_else(
        || std::env::temp_dir().join("rr-dr60-workbench-captures"),
        |h| Path::new(&h).join("Library/Application Support/rr-dr60-workbench/captures"),
    )
}

/// The first unused `capture-<n>.wav` in `dir` (n from 1).
pub fn next_path(dir: &Path) -> PathBuf {
    (1u64..)
        .map(|n| dir.join(format!("capture-{n}.wav")))
        .find(|p| !p.exists())
        .expect("an unused name")
}

/// Counts input frames the device did not deliver (coach review 6, hardware review B4): each
/// callback's first sample time against the previous callback's end. A callback whose sample
/// time is not valid (`kAudioTimeStampSampleTimeValid` clear) re-bases the counter; a step back
/// (the device's timeline reset) is counted apart and re-bases it. Real-time safe (atomics only).
#[derive(Debug, Default)]
pub struct GapCounter {
    /// The next expected sample time (bits of an f64; 0 before the first callback).
    next: AtomicU64,
    /// Gaps seen.
    pub gaps: AtomicU64,
    /// Frames missing in all.
    pub missing: AtomicU64,
    /// Timeline resets (a step back).
    pub resets: AtomicU64,
}

impl GapCounter {
    /// Notes a callback of `frames` starting at `sample_time`, `valid` when the time stamp's
    /// sample time is.
    pub fn observe(&self, sample_time: f64, frames: usize, valid: bool) {
        if !valid {
            self.next.store(0, Ordering::Relaxed);
            return;
        }
        let prev = f64::from_bits(self.next.load(Ordering::Relaxed));
        if prev > 0.0 {
            if sample_time > prev + 0.5 {
                self.gaps.fetch_add(1, Ordering::Relaxed);
                self.missing
                    .fetch_add((sample_time - prev).round() as u64, Ordering::Relaxed);
            } else if sample_time + 0.5 < prev {
                self.resets.fetch_add(1, Ordering::Relaxed);
            }
        }
        self.next
            .store((sample_time + frames as f64).to_bits(), Ordering::Relaxed);
    }
}

/// The monitor's state across output callbacks (D4).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct MonitorState {
    /// The queue has reached its target depth since the last underrun.
    pub primed: bool,
}

/// The monitor's fill (D4; coach review 7, hardware reviews Q8, B1, B2). Real-time safe.
/// - The bounds follow the callbacks: the target is at least one output buffer plus a block,
///   the ceiling at least the target plus an output buffer, the largest input burst
///   ([`crate::live::MAX_PART`]) and a block, so large device buffers do not drop and underrun
///   all the time.
/// - It plays nothing until the target waits (priming), and primes again after an underrun.
/// - Above the ceiling it drops whole `block`s, the oldest, rounded down so at least the target
///   remains, counted in `drops` (samples).
/// - An empty queue plays zeros, counted in `underruns` (samples) only when the gate is not
///   `paused`: a paused gate emits nothing.
#[allow(clippy::too_many_arguments)]
pub fn monitor_fill(
    q: &mut Consumer<f32>,
    out: &mut [f32],
    m: &mut MonitorState,
    target: usize,
    ceiling: usize,
    block: usize,
    paused: bool,
    underruns: &AtomicU64,
    drops: &AtomicU64,
) {
    let block = block.max(1);
    let target = target.max(out.len() + block);
    let ceiling = ceiling.max(target + out.len() + crate::live::MAX_PART + block);
    let waiting = q.slots();
    if !m.primed {
        if waiting < target {
            out.fill(0.0);
            return;
        }
        m.primed = true;
    }
    if waiting > ceiling {
        let extra = ((waiting - target) / block) * block;
        if extra > 0
            && let Ok(chunk) = q.read_chunk(extra)
        {
            chunk.commit_all();
            drops.fetch_add(extra as u64, Ordering::Relaxed);
        }
    }
    let mut short = 0u64;
    for v in out.iter_mut() {
        *v = match q.pop() {
            Ok(x) => x,
            Err(_) => {
                short += 1;
                0.0
            }
        };
    }
    if short > 0 {
        m.primed = false;
        if !paused {
            underruns.fetch_add(short, Ordering::Relaxed);
        }
    }
}

/// The digital-silence watch (D8; coach review 12): evaluated once, when `rate` captured samples
/// have been seen; a later non-zero sample clears it; it never returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SilenceWatch {
    rate: u64,
    seen: u64,
    any: bool,
    cleared: bool,
}

impl SilenceWatch {
    /// A watch at `rate`.
    pub fn new(rate: u32) -> Self {
        Self {
            rate: u64::from(rate),
            seen: 0,
            any: false,
            cleared: false,
        }
    }

    /// Notes captured samples.
    pub fn observe(&mut self, x: &[f32]) {
        let nonzero = x.iter().any(|v| *v != 0.0);
        if self.seen < self.rate {
            self.any |= nonzero;
        } else if nonzero {
            self.cleared = true;
        }
        self.seen += x.len() as u64;
    }

    /// Whether the notice is shown: the first second was all zeros, and nothing since cleared it.
    pub fn silent(&self) -> bool {
        self.seen >= self.rate && !self.any && !self.cleared
    }
}
