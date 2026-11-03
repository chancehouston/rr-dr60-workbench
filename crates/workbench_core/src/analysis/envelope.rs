//! Min/max envelopes for the waveform strips (spec 002 D5; research R-05).

/// Tiers of (min, max) pairs at 1, 16, 256 and 4096 samples per pair.
#[derive(Clone, Debug)]
pub struct Envelope {
    len: u64,
    tiers: Vec<(u64, Vec<(f32, f32)>)>,
}

impl Envelope {
    /// Builds the tiers of `x` once (the finest tier is the samples themselves, not stored).
    pub fn new(x: &[f32]) -> Self {
        let mut tiers = Vec::new();
        let mut prev: Vec<(f32, f32)> = x
            .chunks(16)
            .map(|c| {
                c.iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), &v| {
                        (a.min(v), b.max(v))
                    })
            })
            .collect();
        let mut per = 16u64;
        loop {
            let next: Vec<(f32, f32)> = prev
                .chunks(16)
                .map(|c| {
                    c.iter()
                        .fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), &(lo, hi)| {
                            (a.min(lo), b.max(hi))
                        })
                })
                .collect();
            tiers.push((per, std::mem::take(&mut prev)));
            if per >= 4096 || next.is_empty() {
                if per < 4096 && !next.is_empty() {
                    tiers.push((per * 16, next));
                }
                break;
            }
            prev = next;
            per *= 16;
        }
        Self {
            len: x.len() as u64,
            tiers,
        }
    }

    /// The samples covered.
    pub fn len(&self) -> u64 {
        self.len
    }

    /// The tiers' bytes (the memory test).
    pub fn bytes(&self) -> usize {
        self.tiers.iter().map(|(_, v)| v.len() * 8).sum()
    }

    /// Whether it covers nothing.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// `cols` columns of `[min, max]` over `range` of `x`; each column's pair bounds every sample
    /// of its span (`envelope_bounds`). Empty columns read (0, 0).
    pub fn columns(&self, x: &[f32], range: (u64, u64), cols: usize) -> Vec<(f32, f32)> {
        let (a, b) = (range.0.min(self.len), range.1.min(self.len));
        if cols == 0 || b <= a {
            return vec![(0.0, 0.0); cols];
        }
        let span = (b - a) as f64 / cols as f64;
        let tier = self
            .tiers
            .iter()
            .rev()
            .find(|(per, _)| (*per as f64) <= span);
        (0..cols)
            .map(|c| {
                let s = a + (c as f64 * span) as u64;
                let e = (a + ((c + 1) as f64 * span) as u64).min(b).max(s + 1);
                minmax(x, tier, s, e)
            })
            .collect()
    }
}

/// The exact min/max of `x[s..e]`, using a tier's pairs for the aligned middle.
fn minmax(x: &[f32], tier: Option<&(u64, Vec<(f32, f32)>)>, s: u64, e: u64) -> (f32, f32) {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    let mut take = |v: f32| {
        lo = lo.min(v);
        hi = hi.max(v);
    };
    match tier {
        Some((per, pairs)) => {
            let first = s.div_ceil(*per);
            let last = e / per;
            if first >= last {
                for &v in &x[s as usize..e as usize] {
                    take(v);
                }
            } else {
                for &v in &x[s as usize..(first * per) as usize] {
                    take(v);
                }
                for &(l, h) in &pairs[first as usize..last as usize] {
                    take(l);
                    take(h);
                }
                for &v in &x[(last * per) as usize..e as usize] {
                    take(v);
                }
            }
        }
        None => {
            for &v in &x[s as usize..e as usize] {
                take(v);
            }
        }
    }
    if lo > hi { (0.0, 0.0) } else { (lo, hi) }
}
