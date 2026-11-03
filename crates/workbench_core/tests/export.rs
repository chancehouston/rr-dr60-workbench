//! Spec 001 T024 [TR: FR-012, US3 AS1; SC-006 overwrite]: the WAV + sidecar pair.

mod support;

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use support::*;
use workbench_core::export::{self, ExportError, ExportFs, RealFs};

#[test]
fn roundtrip() {
    let dir = tempdir("roundtrip");
    for rate in RATES {
        let x = voiced(rate, 0.6, 0.5);
        let inp = input_of(rate, &x);
        let panel = workbench_core::panel::Panel::new(rate).unwrap();
        let r = run(&request_for(&inp, &panel, true));
        let stem = dir.join(format!("r{rate}"));
        export::export(&mut RealFs, &stem, &r).unwrap();
        let (wav, side) = export::paths(&stem);
        let mut rd = hound::WavReader::open(&wav).unwrap();
        let spec = rd.spec();
        assert_eq!(
            (spec.channels, spec.sample_rate, spec.bits_per_sample),
            (1, rate, 32)
        );
        assert_eq!(spec.sample_format, hound::SampleFormat::Float);
        let back: Vec<f32> = rd.samples::<f32>().map(Result::unwrap).collect();
        assert_eq!(back.len(), r.samples.len());
        assert!(
            back.iter()
                .zip(r.samples.iter())
                .all(|(a, b)| a.to_bits() == b.to_bits()),
            "{rate}: bit-equal"
        );
        // The header: WAVE_FORMAT_EXTENSIBLE with the IEEE-float subformat GUID (research R-03).
        let bytes = std::fs::read(&wav).unwrap();
        assert_eq!(u16::from_le_bytes([bytes[20], bytes[21]]), 0xFFFE);
        let ieee = [
            0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38,
            0x9B, 0x71,
        ];
        assert_eq!(&bytes[44..60], &ieee);
        assert!(side.exists());
    }
}

#[test]
fn refuses_oversize() {
    assert_eq!(export::MAX_DATA_BYTES, u64::from(u32::MAX) - 60);
    let limit = export::MAX_DATA_BYTES / 4;
    assert!(export::check_size(limit).is_ok());
    assert!(matches!(
        export::check_size(limit + 1),
        Err(ExportError::TooLarge(_))
    ));
    assert!(export::check_size(0).is_ok());
}

/// A file system in memory that fails the k-th operation.
#[derive(Default)]
struct FailFs {
    files: BTreeMap<PathBuf, Vec<u8>>,
    ops: usize,
    fail_at: Option<usize>,
}

impl FailFs {
    fn step(&mut self) -> io::Result<()> {
        self.ops += 1;
        if Some(self.ops) == self.fail_at {
            Err(io::Error::other("injected"))
        } else {
            Ok(())
        }
    }
}

impl ExportFs for FailFs {
    fn write(&mut self, p: &Path, b: &[u8]) -> io::Result<()> {
        self.step()?;
        self.files.insert(p.to_path_buf(), b.to_vec());
        Ok(())
    }
    fn rename(&mut self, f: &Path, t: &Path) -> io::Result<()> {
        self.step()?;
        let v = self
            .files
            .remove(f)
            .ok_or_else(|| io::Error::other("missing"))?;
        self.files.insert(t.to_path_buf(), v);
        Ok(())
    }
    fn remove(&mut self, p: &Path) -> io::Result<()> {
        self.step()?;
        self.files.remove(p);
        Ok(())
    }
    fn exists(&self, p: &Path) -> bool {
        self.files.contains_key(p)
    }
}

#[test]
fn failure_leaves_no_pair() {
    let rate = 8000;
    let x = tone(rate, 0.2, 0.5);
    let inp = input_of(rate, &x);
    let panel = workbench_core::panel::Panel::new(rate).unwrap();
    let r = run(&request_for(&inp, &panel, true));
    let stem = Path::new("/x/out");
    let (wav, side) = export::paths(stem);
    // Count the operations of a clean run, with an old pair present.
    let old = || {
        let mut fs = FailFs::default();
        fs.files.insert(wav.clone(), b"old wav".to_vec());
        fs.files.insert(side.clone(), b"old side".to_vec());
        fs
    };
    let mut clean = old();
    export::export(&mut clean, stem, &r).unwrap();
    let total = clean.ops;
    assert!(total >= 4);
    for k in 1..=total {
        let mut fs = old();
        fs.fail_at = Some(k);
        let res = export::export(&mut fs, stem, &r);
        assert!(matches!(res, Err(ExportError::Io(_))), "op {k}");
        let w = fs.files.get(&wav).cloned();
        let s = fs.files.get(&side).cloned();
        let partials = fs
            .files
            .keys()
            .filter(|p| p.to_string_lossy().ends_with(".partial"))
            .count();
        assert_eq!(partials, 0, "op {k}: partial files left");
        // Either the old pair intact, or nothing: never a mixed pair or a lone file.
        let old_pair = w.as_deref() == Some(b"old wav".as_slice())
            && s.as_deref() == Some(b"old side".as_slice());
        let none = w.is_none() && s.is_none();
        assert!(
            old_pair || none,
            "op {k}: wav {:?} side {:?}",
            w.map(|v| v.len()),
            s.map(|v| v.len())
        );
    }
}

#[test]
fn overwrite_replaces_the_pair() {
    let dir = tempdir("overwrite");
    let rate = 16000;
    let a = input_of(rate, &tone(rate, 0.2, 0.5));
    let b = input_of(rate, &noise(rate, 0.3, 0.2));
    let panel = workbench_core::panel::Panel::new(rate).unwrap();
    let ra = run(&request_for(&a, &panel, true));
    let rb = run(&request_for(&b, &panel, true));
    let stem = dir.join("same.wav");
    export::export(&mut RealFs, &stem, &ra).unwrap();
    export::export(&mut RealFs, &stem, &rb).unwrap();
    let (wav, side) = export::paths(&stem);
    let n = hound::WavReader::open(&wav).unwrap().len() as usize;
    assert_eq!(n, rb.samples.len());
    let text = std::fs::read_to_string(side).unwrap();
    assert!(text.contains(&workbench_core::input::hex(&rb.output_sha256)));
}

fn tempdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rr-wb-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}
