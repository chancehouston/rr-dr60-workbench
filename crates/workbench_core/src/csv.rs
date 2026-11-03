//! The CSV exports (spec 002 D12; contracts/csv.md): `#` method lines, a header row, and the
//! render's sidecar beside each file.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::analysis::spectrum::SpectrumPair;
use crate::export::ExportFs;
use crate::meters::{FIELDS, Kind};
use crate::render::Render;
use crate::taps::TapTable;

fn engine_line(r: &Render) -> String {
    format!(
        "# engine {} {}, rate {} Hz\n",
        crate::engine_version(),
        crate::engine_commit(),
        r.request.rate_hz
    )
}

/// The spectra and their ratio.
pub fn spectrum_csv(p: &SpectrumPair, r: &Render) -> String {
    let mut s = String::from("# rr-dr60-workbench spectrum 1\n");
    s += &engine_line(r);
    let _ = writeln!(
        s,
        "# N {}, Hann periodic, 50 % overlap, power average, dB re full-scale sine per 5 Hz bin (ENBW 7.5 Hz); range in [{}, {}) out [{}, {}); frames original {}, render {} (dropped at splices or muted regions {}); guard at splice and region edges {} samples",
        p.original.n,
        p.range_in.0,
        p.range_in.1,
        p.range_out.0,
        p.range_out.1,
        p.original.frames,
        p.render.frames,
        p.dropped_frames,
        r.playback_latency_samples
    );
    s += "freq_hz,original_db,render_db,ratio_db\n";
    for k in 0..p.original.power.len() {
        let ratio = p.ratio_db(k).map_or_else(String::new, |d| d.to_string());
        let _ = writeln!(
            s,
            "{},{},{},{}",
            p.original.freq(k),
            p.original.db(k),
            p.render.db(k),
            ratio
        );
    }
    s
}

/// The meter timeline.
pub fn meters_csv(r: &Render) -> String {
    let mut s = String::from("# rr-dr60-workbench meters 1\n");
    s += &engine_line(r);
    let kinds: Vec<&str> = FIELDS
        .iter()
        .map(|f| match f.kind {
            Kind::Max => "max",
            Kind::Count => "count",
            Kind::State => "state",
        })
        .collect();
    let _ = writeln!(
        s,
        "# block {} (10 ms); spans in host samples; kinds {}",
        r.request.block,
        kinds.join("/")
    );
    s += "row,in_start,in_end,out_start,out_end,tail";
    for f in &FIELDS {
        s += ",";
        s += f.id;
    }
    s += "\n";
    for (i, m) in r.meters.iter().enumerate() {
        let _ = write!(
            s,
            "{i},{},{},{},{},{}",
            m.in_span.0,
            m.in_span.1,
            m.out_span.0,
            m.out_span.1,
            u8::from(m.tail)
        );
        for f in &FIELDS {
            let _ = write!(s, ",{}", (f.get)(&m.meters) as f32);
        }
        s += "\n";
    }
    s
}

/// The tap table.
pub fn taps_csv(t: &TapTable) -> String {
    let mut s = String::from("# rr-dr60-workbench taps 1\n");
    let _ = writeln!(
        s,
        "# engine {} {}, rate {} Hz\n# fresh engine per tap; readings as 001's",
        crate::engine_version(),
        crate::engine_commit(),
        t.request.rate_hz
    );
    s += "tap,status,length,latency,tail,splices_or_regions,not_emitted,peak,sha256\n";
    for row in &t.rows {
        match &row.outcome {
            Ok(r) => {
                let _ = writeln!(
                    s,
                    "{},ok,{},{},{},{},{},{},{}",
                    row.tap,
                    r.length,
                    r.latency,
                    r.tail,
                    r.splices_or_regions,
                    r.not_emitted,
                    r.peak,
                    crate::input::hex(&r.sha256)
                );
            }
            Err(e) => {
                let _ = writeln!(s, "{},refused: {},,,,,,,", row.tap, e.replace(',', ";"));
            }
        }
    }
    s
}

/// The paths for a CSV stem: `<stem>.csv` and `<stem>.sidecar.txt`.
pub fn paths(stem: &Path) -> (PathBuf, PathBuf) {
    let base = stem.with_extension("");
    (base.with_extension("csv"), {
        let mut p = base.into_os_string();
        p.push(".sidecar.txt");
        PathBuf::from(p)
    })
}

/// Writes a CSV and a sidecar text beside it (the tap table's own).
///
/// # Errors
///
/// The file system's.
pub fn write_with_sidecar_text(
    fs: &mut impl ExportFs,
    stem: &Path,
    csv: &str,
    sidecar: &str,
) -> std::io::Result<()> {
    let (c, side) = paths(stem);
    fs.write(&c, csv.as_bytes())?;
    fs.write(&side, sidecar.as_bytes())
}

/// Writes a CSV and the render's sidecar beside it.
///
/// # Errors
///
/// The file system's.
pub fn write_with_sidecar(
    fs: &mut impl ExportFs,
    stem: &Path,
    csv: &str,
    r: &Render,
) -> std::io::Result<()> {
    let (c, side) = paths(stem);
    fs.write(&c, csv.as_bytes())?;
    fs.write(&side, crate::sidecar::write(r).as_bytes())
}
