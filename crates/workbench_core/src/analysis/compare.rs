//! Two renders compared (spec 005b D2 – D4): a parameter diff of their requests, a null test and
//! a spectral difference. The null test and the spectral difference are the workbench's own
//! analyses of the engine's outputs: nothing is played or exported from them.

use rr_dr60::control_map::{self, ControlReading};

use crate::analysis::spectrum::{self, SpectrumPair};
use crate::input::Channel;
use crate::render::{Render, RenderRequest, Scheduled};

/// The reading a request made a control with (D2): the setting from its settings, otherwise its
/// engine value.
pub fn reading(req: &RenderRequest, i: u32) -> Option<ControlReading> {
    control_map::settings_value(&req.settings, i)
        .ok()
        .or_else(|| {
            req.engine_values
                .iter()
                .find(|(j, _)| *j == i)
                .map(|(_, r)| *r)
        })
}

/// A control whose readings differ (D2).
#[derive(Clone, Debug, PartialEq)]
pub struct ControlDiff {
    /// The identifier.
    pub identifier: &'static str,
    /// A's reading.
    pub a: Option<ControlReading>,
    /// B's reading.
    pub b: Option<ControlReading>,
}

/// The parameter diff of two requests (D2). Each field is `Some((a, b))` only when it differs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParamDiff {
    /// The controls, in manifest order.
    pub controls: Vec<ControlDiff>,
    /// The converted inputs' SHA-256 (hex).
    pub input: Option<(String, String)>,
    /// The channels.
    pub channel: Option<(Channel, Channel)>,
    /// The starts.
    pub start: Option<(u64, u64)>,
    /// The lengths (`None`: to the end).
    pub len: Option<(Option<u64>, Option<u64>)>,
    /// The tail switches.
    pub tail: Option<(bool, bool)>,
    /// Changes A has that B lacks, in position order.
    pub a_only: Vec<Scheduled>,
    /// Changes B has that A lacks, in position order.
    pub b_only: Vec<Scheduled>,
}

impl ParamDiff {
    /// Whether nothing differs.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

fn key(c: &Scheduled) -> (u8, u64, u32, u64, u64) {
    let clock = match c.clock {
        rr_dr60::changes::Clock::Input => 0,
        _ => 1,
    };
    (clock, c.position, c.control, c.tag, c.value.to_bits())
}

/// `x` less `y`, as multisets, in (clock, position, control, tag) order.
fn multiset_minus(x: &[Scheduled], y: &[Scheduled]) -> Vec<Scheduled> {
    let mut rest: Vec<_> = y.iter().map(key).collect();
    let mut out: Vec<Scheduled> = x
        .iter()
        .filter(|c| {
            let k = key(c);
            match rest.iter().position(|r| *r == k) {
                Some(at) => {
                    rest.swap_remove(at);
                    false
                }
                None => true,
            }
        })
        .copied()
        .collect();
    out.sort_by_key(key);
    out
}

fn differ<T: PartialEq + Clone>(a: &T, b: &T) -> Option<(T, T)> {
    (a != b).then(|| (a.clone(), b.clone()))
}

/// The parameter diff of two requests (D2). `ControlReading` equality treats −0.0 as 0.0.
pub fn param_diff(a: &RenderRequest, b: &RenderRequest) -> ParamDiff {
    let controls = rr_dr60::controls::controls()
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let (ra, rb) = (reading(a, i as u32), reading(b, i as u32));
            (ra != rb).then_some(ControlDiff {
                identifier: e.identifier,
                a: ra,
                b: rb,
            })
        })
        .collect();
    ParamDiff {
        controls,
        input: differ(
            &crate::input::hex(&a.input_sha256),
            &crate::input::hex(&b.input_sha256),
        ),
        channel: differ(&a.channel, &b.channel),
        start: differ(&a.start, &b.start),
        len: differ(&a.len, &b.len),
        tail: differ(&a.tail, &b.tail),
        a_only: multiset_minus(&a.changes, &b.changes),
        b_only: multiset_minus(&b.changes, &a.changes),
    }
}

/// How B is aligned to A (D3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Align {
    /// By the latencies and starts: s = (L_B − L_A) − (S_B − S_A).
    #[default]
    Latency,
    /// At the first sample of each (s = 0).
    SampleZero,
}

/// The latency-aligned shift: B's index for A's index i is i + s (D3).
pub fn shift(a: &Render, b: &Render) -> i64 {
    (i64::from(b.latency_samples) - i64::from(a.latency_samples))
        - (b.request.start as i64 - a.request.start as i64)
}

/// The figures of one span (D3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Figures {
    /// A's first index of the span.
    pub from: u64,
    /// Samples compared.
    pub samples: u64,
    /// Samples whose bits differ.
    pub differing: u64,
    /// A's index of the first differing sample.
    pub first: Option<u64>,
    /// The peak of |d| in dBFS (−∞ when d is 0 everywhere); it can exceed 0 dBFS, up to +6 dBFS
    /// with the output clamp on.
    pub peak_dbfs: f64,
    /// A's index of the peak.
    pub peak_at: u64,
    /// The RMS of d in dB re RMS 1.0.
    pub rms_db: f64,
    /// RMS(d) ÷ RMS(A) in dB; `None` when A is silent over the span.
    pub depth_db: Option<f64>,
}

fn db_amp(x: f64) -> f64 {
    if x > 0.0 {
        20.0 * x.log10()
    } else {
        f64::NEG_INFINITY
    }
}

/// The figures of `a[from .. to]` against `b[i + s]` (64-bit, sequential sums).
pub fn figures(a: &[f32], b: &[f32], s: i64, from: u64, to: u64) -> Figures {
    let (mut sd, mut sa, mut peak, mut peak_at) = (0.0f64, 0.0f64, 0.0f64, from);
    let (mut differing, mut first) = (0u64, None);
    for i in from..to {
        let x = a[i as usize];
        let y = b[(i as i64 + s) as usize];
        if x.to_bits() != y.to_bits() {
            differing += 1;
            first.get_or_insert(i);
        }
        let d = f64::from(x) - f64::from(y);
        sd += d * d;
        sa += f64::from(x) * f64::from(x);
        if d.abs() > peak {
            peak = d.abs();
            peak_at = i;
        }
    }
    let n = to.saturating_sub(from);
    let mean = |v: f64| if n == 0 { 0.0 } else { v / n as f64 };
    Figures {
        from,
        samples: n,
        differing,
        first,
        peak_dbfs: db_amp(peak),
        peak_at,
        rms_db: db_amp(mean(sd).sqrt()),
        depth_db: (sa > 0.0).then(|| 10.0 * (sd / sa).log10()),
    }
}

/// The overlap of A and B under shift `s`: A's indices [lo, hi).
pub fn overlap(len_a: u64, len_b: u64, s: i64) -> (u64, u64) {
    let lo = (-s).max(0) as u64;
    let hi = (len_a as i64).min(len_b as i64 - s).max(lo as i64) as u64;
    (lo, hi)
}

/// A render's gate reports in input terms: (input position, input length). The reports'
/// positions are on the clock before the coder, so they map with the latency less the heard
/// shift (review B2; engine 0.95, spec 028).
fn reports(r: &Render) -> Vec<(u64, u64)> {
    let pre = r.latency_samples - r.splice_shift_samples;
    r.events
        .iter()
        .map(|e| {
            let (i, _) = rr_dr60::timeline::onepass_output_to_input(
                e.output_position,
                pre,
                &r.events,
                r.drop_mode,
            );
            (r.request.start + i, e.input_length)
        })
        .collect()
}

/// A's output index where the two renders' gates first differ (D3), or `None` when they agree:
/// the first differing report, the earlier of A's and B's; or, when the gate is paused at the
/// end on one side only, that side's end (a pause that never resumes reports no splice: review
/// B1). The prefix is on A's report clock (before the coder): B's report position o maps to
/// A's as o + H_B − H_A − s, H the heard shift (the coder's delay plus the playback half's
/// latency, engine 0.95, spec 028), and a paused end is placed H before the take's end. With the
/// coder on this is earlier than the splice is heard by H, which covers the frame the coder
/// decides it in (a conservative prefix).
pub fn prefix_end(a: &Render, b: &Render, s: i64) -> Option<u64> {
    let (ca, cb) = (
        i64::from(a.splice_shift_samples),
        i64::from(b.splice_shift_samples),
    );
    let at_a = |o: u64| o;
    let at_b = |o: u64| (o as i64 + cb - ca - s).max(0) as u64;
    let (ra, rb) = (reports(a), reports(b));
    let by_report = (0..ra.len().max(rb.len()))
        .find(|&j| ra.get(j) != rb.get(j))
        .and_then(|j| {
            let x = a.events.get(j).map(|e| at_a(e.output_position));
            let y = b.events.get(j).map(|e| at_b(e.output_position));
            match (x, y) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            }
        });
    let by_end = (a.paused_at_end != b.paused_at_end).then(|| {
        if a.paused_at_end {
            (a.samples.len() as i64 - ca).max(0) as u64
        } else {
            at_b((b.samples.len() as i64 - cb).max(0) as u64)
        }
    });
    match (by_report, by_end) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    }
}

/// The null test (D3).
#[derive(Clone, Debug, PartialEq)]
pub struct NullTest {
    /// The alignment used.
    pub align: Align,
    /// The shift: B's index for A's i is i + s.
    pub s: i64,
    /// Equal lengths and equal bits, unaligned.
    pub raw_identical: bool,
    /// The lengths.
    pub len_a: u64,
    /// B's length.
    pub len_b: u64,
    /// The whole overlap; `None`: no overlap (coach review 2).
    pub whole: Option<Figures>,
    /// The prefix up to the first differing gate report; `None`: the reports agree (the same as
    /// the overlap) or there is no overlap.
    pub prefix: Option<Figures>,
    /// A's index of a live take's first budget skip on either side (a label: the take's samples
    /// are continuous engine output, the skip's silence went to the device only; review B3).
    pub cut_at_skip: Option<u64>,
    /// The gate is paused at the end on one side only (A's, B's): that side stops early and
    /// reports no splice for it (review B1).
    pub ends_paused: Option<(bool, bool)>,
    /// A side is a live take whose capture overflowed.
    pub incomplete: bool,
    /// The starts differ (A's, B's): the null then has a floor from the setup, not the settings
    /// (each render's hiss stream, the converter's phase and the coder's frame grid start at its
    /// own first sample; hardware review Q3).
    pub starts_differ: Option<(u64, u64)>,
}

/// The first budget skip of a live take, as a position in its own output.
fn first_skip(r: &Render) -> Option<u64> {
    r.live.as_ref()?.skips.iter().map(|(p, _)| *p).min()
}

/// A comparison refused (D3, D4), in words by `Display`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The renders are at different host rates.
    RatesDiffer(u32, u32),
    /// A side holds no spectrum frame.
    TooShort,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RatesDiffer(a, b) => write!(
                f,
                "A is at {a} Hz and B at {b} Hz: the null test and the spectral difference need one rate"
            ),
            Self::TooShort => write!(
                f,
                "A side holds no 200 ms run outside its splices and muted regions: no spectral difference"
            ),
        }
    }
}

/// The null test of A against B (D3).
///
/// # Errors
///
/// The renders are at different rates.
pub fn null_test(a: &Render, b: &Render, align: Align) -> Result<NullTest, Refused> {
    let (ra, rb) = (a.request.rate_hz, b.request.rate_hz);
    if ra != rb {
        return Err(Refused::RatesDiffer(ra, rb));
    }
    let s = match align {
        Align::Latency => shift(a, b),
        Align::SampleZero => 0,
    };
    let (len_a, len_b) = (a.samples.len() as u64, b.samples.len() as u64);
    let (lo, hi) = overlap(len_a, len_b, s);
    let skip_b = first_skip(b).map(|p| (p as i64 - s).max(0) as u64);
    let cut_at_skip = match (first_skip(a), skip_b) {
        (Some(x), Some(y)) => Some(x.min(y)),
        (x, y) => x.or(y),
    };
    let whole = (hi > lo).then(|| figures(&a.samples, &b.samples, s, lo, hi));
    let prefix = whole.and_then(|_| {
        prefix_end(a, b, s).map(|e| figures(&a.samples, &b.samples, s, lo, e.clamp(lo, hi)))
    });
    let incomplete = |r: &Render| r.live.as_ref().is_some_and(|l| l.incomplete);
    Ok(NullTest {
        align,
        s,
        raw_identical: len_a == len_b
            && a.samples
                .iter()
                .zip(b.samples.iter())
                .all(|(x, y)| x.to_bits() == y.to_bits()),
        len_a,
        len_b,
        whole,
        prefix,
        cut_at_skip,
        ends_paused: (a.paused_at_end != b.paused_at_end)
            .then_some((a.paused_at_end, b.paused_at_end)),
        incomplete: incomplete(a) || incomplete(b),
        starts_differ: (a.request.start != b.request.start)
            .then_some((a.request.start, b.request.start)),
    })
}

/// A side's runs for the spectrum: 002's runs over the whole render.
pub fn side_runs(r: &Render) -> Vec<(u64, u64)> {
    spectrum::render_runs((0, r.samples.len() as u64), r)
}

/// The spectral difference (D4): each side's spectrum over its own runs (002's method), the ratio
/// B ÷ A by 002's `ratio_db` (the `original` field holds A, `render` B).
///
/// # Errors
///
/// The rates differ, or a side holds no frame.
pub fn spectral(a: &Render, b: &Render) -> Result<SpectrumPair, Refused> {
    let (ra, rb) = (a.request.rate_hz, b.request.rate_hz);
    if ra != rb {
        return Err(Refused::RatesDiffer(ra, rb));
    }
    let sa = spectrum::welch(&a.samples, ra, &side_runs(a)).map_err(|_| Refused::TooShort)?;
    let sb = spectrum::welch(&b.samples, rb, &side_runs(b)).map_err(|_| Refused::TooShort)?;
    // `range_in` and `range_out` here are A's and B's whole output ranges (not an input range).
    Ok(SpectrumPair {
        original: sa,
        render: sb,
        range_in: (0, a.samples.len() as u64),
        range_out: (0, b.samples.len() as u64),
        dropped_frames: 0,
    })
}
