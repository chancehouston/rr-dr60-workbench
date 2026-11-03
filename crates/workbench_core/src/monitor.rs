//! The playback callback's body (spec 001 FR-011; research R-05). [`fill`] reads three takes in
//! memory and a few atomics. It allocates nothing, locks nothing and drops nothing, so the
//! CoreAudio callback can call it directly.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// No pending request.
pub const NO_REQUEST: u64 = u64::MAX;
/// The take's two bits (62 and 63) of the cursor word.
const TAKE_SHIFT: u32 = 62;
const TAKE_MASK: u64 = 0b11 << TAKE_SHIFT;

/// Which take plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Take {
    /// The selected channel after conversion.
    Original,
    /// The engine's render (A).
    Render,
    /// The comparison render B (spec 005b D5).
    B,
}

/// Packs a take and a position into one cursor word (bits 62 – 63 the take, bits 0 – 61 the
/// position).
pub const fn pack(take: Take, position: u64) -> u64 {
    debug_assert!(position < 1 << TAKE_SHIFT, "a position below 2^62");
    let t: u64 = match take {
        Take::Original => 0,
        Take::Render => 1,
        Take::B => 2,
    };
    (t << TAKE_SHIFT) | (position & !TAKE_MASK)
}

/// Unpacks a cursor word.
pub const fn unpack(word: u64) -> (Take, u64) {
    let take = match (word & TAKE_MASK) >> TAKE_SHIFT {
        0 => Take::Original,
        1 => Take::Render,
        _ => Take::B,
    };
    (take, word & !TAKE_MASK)
}

/// The state the UI and the callback share. Only atomics.
#[derive(Debug)]
pub struct Shared {
    /// The current take and position, written by the callback.
    pub cursor: AtomicU64,
    /// A cursor the UI asks for; swapped in at the top of the next buffer.
    pub request: AtomicU64,
    /// The old take's position the request was mapped from: the callback adds the samples
    /// played since then, so a callback between the UI's read and the swap neither repeats nor
    /// skips (hardware review B2).
    pub request_base: AtomicU64,
    /// The monitor volume's f32 bits (playback only, never exported).
    pub volume_bits: AtomicU32,
    /// Whether samples are played (otherwise zeros, the cursor held).
    pub playing: AtomicBool,
    /// Device overloads counted by the platform layer.
    pub overloads: AtomicU64,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            cursor: AtomicU64::new(pack(Take::Original, 0)),
            request: AtomicU64::new(NO_REQUEST),
            request_base: AtomicU64::new(NO_REQUEST),
            volume_bits: AtomicU32::new(1.0f32.to_bits()),
            playing: AtomicBool::new(false),
            overloads: AtomicU64::new(0),
        }
    }
}

impl Shared {
    /// Asks the callback to move to `take` at `position` from its next buffer (a jump: no base).
    pub fn request(&self, take: Take, position: u64) {
        self.request_base.store(NO_REQUEST, Ordering::Relaxed);
        self.request.store(pack(take, position), Ordering::Release);
    }

    /// Asks for `take` at `mapped`, mapped from the old take's position `base`: the callback
    /// starts at `mapped + (now − base)`, the samples played since the UI read the cursor.
    pub fn request_from(&self, take: Take, mapped: u64, base: u64) {
        self.request_base.store(base, Ordering::Relaxed);
        self.request.store(pack(take, mapped), Ordering::Release);
    }

    /// The current take and position.
    pub fn cursor(&self) -> (Take, u64) {
        unpack(self.cursor.load(Ordering::Acquire))
    }

    /// Sets the monitor volume.
    pub fn set_volume(&self, v: f32) {
        self.volume_bits.store(v.to_bits(), Ordering::Relaxed);
    }
}

/// The two takes, shared by `Arc` clones (no copies).
#[derive(Clone, Debug)]
pub struct Takes {
    /// The original (the selected channel after conversion).
    pub original: Arc<[f32]>,
    /// The render (A).
    pub render: Arc<[f32]>,
    /// The comparison render B (empty when none).
    pub b: Arc<[f32]>,
    /// Their rate.
    pub rate_hz: u32,
}

/// Fills one device buffer: the same samples to `left` and `right` (an exact copy, R-04), times
/// the monitor volume, zeros past a take's end or while stopped.
pub fn fill(takes: &Takes, shared: &Shared, left: &mut [f32], right: &mut [f32]) {
    let req = shared.request.swap(NO_REQUEST, Ordering::AcqRel);
    let mut word = shared.cursor.load(Ordering::Relaxed);
    if req != NO_REQUEST {
        // `request` and `request_base` are two words: a jump that lands between this swap and
        // this load turns a mapped request into a jump, or the reverse adds a stale delta, for one
        // buffer at most. Accepted (spec 005b review B6): the UI sends one request per action.
        let base = shared.request_base.load(Ordering::Relaxed);
        let (_, now) = unpack(word);
        let (t, mapped) = unpack(req);
        word = if base == NO_REQUEST {
            req
        } else {
            pack(t, mapped + now.saturating_sub(base))
        };
    }
    let (take, mut pos) = unpack(word);
    if !shared.playing.load(Ordering::Relaxed) {
        left.fill(0.0);
        right.fill(0.0);
        shared.cursor.store(word, Ordering::Release);
        return;
    }
    let src: &[f32] = match take {
        Take::Original => &takes.original,
        Take::Render => &takes.render,
        Take::B => &takes.b,
    };
    let vol = f32::from_bits(shared.volume_bits.load(Ordering::Relaxed));
    let len = src.len() as u64;
    for (l, r) in left.iter_mut().zip(right.iter_mut()) {
        let s = if pos < len {
            let v = src[pos as usize] * vol;
            pos += 1;
            v
        } else {
            0.0
        };
        *l = s;
        *r = s;
    }
    shared.cursor.store(pack(take, pos), Ordering::Release);
}
