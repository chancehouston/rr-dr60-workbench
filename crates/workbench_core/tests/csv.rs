//! Spec 002 [TR: FR-010, US2 AS5, US3 AS3]: the CSV exports and their sidecars.

mod support;

use std::sync::atomic::{AtomicBool, AtomicU64};

use support::*;
use workbench_core::analysis::spectrum;
use workbench_core::{csv, export, taps};

fn dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("rr-wb-csv-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn spectrum_csv_roundtrip() {
    let rate = 16000;
    let x = voiced(rate, 2.0, 0.5);
    let r = run(&request(rate, &x, |_| {}));
    let p = spectrum::pair(&x, &r, (0, x.len() as u64)).unwrap();
    let text = csv::spectrum_csv(&p, &r);
    let rows: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(rows[0], "freq_hz,original_db,render_db,ratio_db");
    assert_eq!(rows.len() - 1, p.original.power.len());
    for (k, line) in rows[1..].iter().enumerate() {
        let c: Vec<&str> = line.split(',').collect();
        assert_eq!(c[0].parse::<f64>().unwrap(), p.original.freq(k));
        assert_eq!(c[1].parse::<f64>().unwrap(), p.original.db(k), "bin {k}");
        assert_eq!(c[2].parse::<f64>().unwrap(), p.render.db(k), "bin {k}");
        match p.ratio_db(k) {
            Some(d) => assert_eq!(c[3].parse::<f64>().unwrap(), d),
            None => assert_eq!(c[3], "", "an empty cell where the ratio is not drawn"),
        }
    }
    assert!(text.contains("N 3200, Hann periodic"));
    let d = dir("spectrum");
    csv::write_with_sidecar(&mut export::RealFs, &d.join("s"), &text, &r).unwrap();
    let (c, side) = csv::paths(&d.join("s"));
    assert_eq!(std::fs::read_to_string(c).unwrap(), text);
    assert!(
        workbench_core::sidecar::read(&std::fs::read_to_string(side).unwrap()).is_ok(),
        "the sidecar beside it"
    );
}

#[test]
fn meters_csv_roundtrip() {
    let rate = 8000;
    let x = voiced(rate, 1.0, 0.5);
    let r = run(&request(rate, &x, |_| {}));
    let text = csv::meters_csv(&r);
    let rows: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(rows.len() - 1, r.meters.len());
    let header: Vec<&str> = rows[0].split(',').collect();
    assert_eq!(header.len(), 6 + 13);
    for (i, line) in rows[1..].iter().enumerate() {
        let c: Vec<&str> = line.split(',').collect();
        let m = &r.meters[i];
        assert_eq!(
            c[1..6],
            [
                m.in_span.0.to_string(),
                m.in_span.1.to_string(),
                m.out_span.0.to_string(),
                m.out_span.1.to_string(),
                u8::from(m.tail).to_string()
            ]
        );
        for (j, f) in workbench_core::meters::FIELDS.iter().enumerate() {
            assert_eq!(
                c[6 + j].parse::<f32>().unwrap(),
                (f.get)(&m.meters) as f32,
                "row {i} {}",
                f.id
            );
        }
    }
    let d = dir("meters");
    csv::write_with_sidecar(&mut export::RealFs, &d.join("m"), &text, &r).unwrap();
    assert!(
        workbench_core::sidecar::read(
            &std::fs::read_to_string(csv::paths(&d.join("m")).1).unwrap()
        )
        .is_ok()
    );
}

#[test]
fn tap_csv_roundtrip() {
    let rate = 6000;
    let x = voiced(rate, 0.5, 0.5);
    let req = request(rate, &x, |_| {});
    let t = taps::browse(&req, &AtomicU64::new(0), &AtomicBool::new(false));
    let text = csv::taps_csv(&t);
    let rows: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(rows.len() - 1, t.rows.len());
    // The table's own sidecar (the request's render) restores and reproduces the table.
    let side = t.sidecar.clone().expect("the request's own render");
    let inp = input_of(rate, &x);
    let mut q = workbench_core::panel::Panel::new(rate).unwrap();
    let rep = workbench_core::sidecar::restore(
        &workbench_core::sidecar::read(&side).unwrap(),
        &mut q,
        &inp,
    );
    let mut req2 = request_for(&inp, &q, rep.tail);
    req2.input = req.input.clone();
    req2.input_sha256 = req.input_sha256;
    let t2 = taps::browse(&req2, &AtomicU64::new(0), &AtomicBool::new(false));
    for (a, b) in t.rows.iter().zip(&t2.rows) {
        assert_eq!(
            a.outcome.as_ref().ok().map(|r| r.sha256),
            b.outcome.as_ref().ok().map(|r| r.sha256),
            "{}",
            a.tap
        );
    }
    let d = dir("taps");
    csv::write_with_sidecar_text(&mut export::RealFs, &d.join("t"), &text, &side).unwrap();
    assert_eq!(
        std::fs::read_to_string(csv::paths(&d.join("t")).1).unwrap(),
        side
    );
    for (row, line) in t.rows.iter().zip(&rows[1..]) {
        assert!(line.starts_with(&format!("{},", row.tap)));
        if let Ok(r) = &row.outcome {
            assert!(line.ends_with(&workbench_core::input::hex(&r.sha256)));
        } else {
            assert!(line.contains("refused"));
        }
    }
}
