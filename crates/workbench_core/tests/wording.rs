//! Spec 001 T030 [TR: FR-016; research R-13]: every workbench string passes the engine's wording
//! lists (`rr_dr60::wording`, both tiers; spec 008 D4), plus the supplementary list.
//!
//! The strings checked: everything in `workbench_core::text`, and every string literal in the
//! workbench's sources outside comments (a dated restatement of T030: the UI's literals are
//! checked in place rather than banned).

use std::path::{Path, PathBuf};

/// The engine's two wording tiers (`rr_dr60::wording`, engine spec 026 D3; spec 008 D4) and the
/// workbench's supplementary list: any word starting with "accura", "reveal" or "uncover" (the
/// engine's claims tier lists exact forms; the workbench also refuses the other forms).
struct Denylist;

impl Denylist {
    fn load() -> Self {
        Self
    }

    /// The first hit in `s`: the engine's manifests' list, then its claims list, then the
    /// supplementary prefixes.
    fn hit(&self, s: &str) -> Option<String> {
        if let Some(h) = rr_dr60::wording::first_hit(s) {
            return Some(h.entry.to_owned());
        }
        if let Some(h) = rr_dr60::wording::first_claim_hit(s) {
            return Some(h.entry.to_owned());
        }
        let lower = s.to_lowercase();
        lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .find(|w| {
                ["accura", "reveal", "uncover"]
                    .iter()
                    .any(|p| w.starts_with(p))
            })
            .map(str::to_owned)
    }
}

/// String literals outside comments in a Rust source (a simple lexer: `"…"` with escapes).
fn literals(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in src.lines() {
        let t = line.trim_start();
        if t.starts_with("//") {
            continue;
        }
        let code = match line.find(" //") {
            Some(i) if line[..i].matches('"').count() % 2 == 0 => &line[..i],
            _ => line,
        };
        let mut chars = code.chars().peekable();
        let mut cur: Option<String> = None;
        while let Some(c) = chars.next() {
            match (&mut cur, c) {
                (None, '"') => cur = Some(String::new()),
                (None, '\'') => {
                    // A char literal or a lifetime: skip a quoted char like '"'.
                    if chars.peek() == Some(&'"') {
                        chars.next();
                    }
                }
                (Some(s), '\\') => {
                    if let Some(n) = chars.next() {
                        s.push(n);
                    }
                }
                (Some(s), '"') => {
                    out.push(std::mem::take(s));
                    cur = None;
                }
                (Some(s), c) => s.push(c),
                (None, _) => {}
            }
        }
    }
    out
}

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
fn the_matcher_works() {
    let d = Denylist::load();
    assert!(d.hit("a gate event").is_some());
    assert!(d.hit("Splices").is_none());
    assert!(
        d.hit("The sensitivity setting").is_none(),
        "the engine passes `sensitivity`"
    );
    assert!(d.hit("an accurate copy").is_some());
    assert!(d.hit("not measured").is_none());
    assert!(d.hit("measured on a unit").is_some());
    assert!(d.hit("listening for a voice").is_some());
    assert!(d.hit("hidden voices").is_some(), "the engine's claims tier");
    assert!(d.hit("revealing").is_some(), "the supplementary prefixes");
}

#[test]
fn every_workbench_string_passes() {
    let d = Denylist::load();
    let mut strings = workbench_core::text::all();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("crates");
    let mut files = Vec::new();
    rs_files(&root, &mut files);
    for f in files
        .iter()
        .filter(|p| p.components().any(|c| c.as_os_str() == "src"))
    {
        strings.extend(literals(&std::fs::read_to_string(f).unwrap()));
    }
    assert!(
        strings.len() > 50,
        "the scan found the strings: {}",
        strings.len()
    );
    let hits: Vec<(String, String)> = strings
        .iter()
        .filter_map(|s| d.hit(s).map(|h| (h, s.clone())))
        .collect();
    assert!(hits.is_empty(), "denylisted wording: {hits:#?}");
}

/// Spec 005 (post-implementation review 13): the engine's enum names the record view prints
/// with `{:?}` are not literals the scan sees, so they are checked here.
#[test]
fn record_view_debug_names_pass() {
    use rr_dr60::components::ComponentId;
    use rr_dr60::record::{Origin, StopHow};
    let d = Denylist::load();
    let names = [
        format!("{:?}", Origin::Engine),
        format!("{:?}", Origin::Imported),
        format!("{:?}", StopHow::StopCall),
        format!("{:?}", StopHow::MemoryFull),
        format!("{:?}", rr_dr60::Encoding::Frames),
        format!("{:?}", rr_dr60::Encoding::Samples),
        format!("{:?}", ComponentId::None),
        format!("{:?}", ComponentId::BandFilter),
    ];
    for n in &names {
        assert!(d.hit(n).is_none(), "{n}: {:?}", d.hit(n));
    }
    for (_, f) in workbench_core::records::FLAG_NAMES {
        assert!(d.hit(f).is_none(), "{f}");
    }
}
