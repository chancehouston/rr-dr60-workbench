//! Exports the pinned engine commit as `RR_DR60_COMMIT` (spec 001 research R-12): read from the
//! workspace's `Cargo.lock`, so the commit in a sidecar is the code that was built.

use std::fs;

fn main() {
    let lock = "../../Cargo.lock";
    println!("cargo:rerun-if-changed={lock}");
    let text = fs::read_to_string(lock).expect("Cargo.lock at the workspace root");
    let mut in_engine = false;
    let mut commit = None;
    for line in text.lines() {
        if line == "[[package]]" {
            in_engine = false;
        } else if line == "name = \"rr_dr60\"" {
            in_engine = true;
        } else if in_engine && line.starts_with("source = \"git+") {
            commit = line
                .trim_end_matches('"')
                .rsplit_once('#')
                .map(|(_, sha)| sha.to_owned());
        }
    }
    let commit = commit.expect("rr_dr60 is a git dependency with a #<sha> source in Cargo.lock");
    assert!(
        commit.len() == 40 && commit.bytes().all(|b| b.is_ascii_hexdigit()),
        "rr_dr60's locked commit is not a 40-character sha: {commit}"
    );
    println!("cargo:rustc-env=RR_DR60_COMMIT={commit}");
}
