//! Spec 001 T031 [TR: FR-015]: no wall-clock time or OS randomness in the workbench's sources.

use std::path::{Path, PathBuf};

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn no_clock_or_os_randomness() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("crates");
    let mut files = Vec::new();
    rs_files(&root, &mut files);
    let srcs: Vec<PathBuf> = files
        .into_iter()
        .filter(|p| p.components().any(|c| c.as_os_str() == "src"))
        .collect();
    assert!(srcs.len() >= 10);
    for f in srcs {
        let s = std::fs::read_to_string(&f).unwrap();
        for banned in [
            "SystemTime",
            "Instant::now",
            "rand::",
            "getrandom",
            "thread_rng",
            "RandomState",
        ] {
            assert!(!s.contains(banned), "{} uses {banned}", f.display());
        }
    }
}

#[test]
fn analysis_only_reads() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for f in [
        "analysis/spectrum.rs",
        "analysis/envelope.rs",
        "analysis/spectrogram.rs",
        "meters.rs",
    ] {
        let s = std::fs::read_to_string(root.join(f)).unwrap();
        for banned in [
            "Pipeline",
            "fresh_engine(",
            "process_with_events(",
            "set_control_value(",
            "export::",
        ] {
            assert!(
                !s.contains(banned),
                "{f} must only read samples, but uses {banned}"
            );
        }
    }
}
