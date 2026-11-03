//! Spec 001 T030 [TR: FR-016; research R-13]: every workbench string passes the engine's wording
//! denylist, read from the pinned engine's harness source, plus the supplementary list.
//!
//! The strings checked: everything in `workbench_core::text`, and every string literal in the
//! workbench's sources outside comments (a dated restatement of T030: the UI's literals are
//! checked in place rather than banned).

use std::path::{Path, PathBuf};
use std::process::Command;

/// The pinned engine checkout's root (the parent of `crates/rr_dr60`).
fn engine_root() -> PathBuf {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata");
    let json = String::from_utf8(out.stdout).unwrap();
    let at = json.find("\"name\":\"rr_dr60\"").expect("rr_dr60");
    let rest = &json[at..];
    let key = "\"manifest_path\":\"";
    let m = rest.find(key).unwrap() + key.len();
    let path = &rest[m..m + rest[m..].find('"').unwrap()];
    Path::new(path).parent().unwrap().join("../..")
}

/// The string literals of a `pub const NAME: &[&str] = &[ … ];` block.
fn const_list(src: &str, name: &str) -> Vec<String> {
    let start = src
        .find(&format!("pub const {name}: &[&str] = &["))
        .unwrap_or_else(|| panic!("{name} not found in the engine's harness"));
    let body = &src[start..start + src[start..].find("];").unwrap()];
    let list: Vec<String> = body
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();
    assert!(!list.is_empty(), "{name} parsed empty");
    list
}

/// The suffix set from `denylist_regex`: `(e|es|s|…)?\b`.
fn suffixes(src: &str) -> Vec<String> {
    let at = src.find("fn denylist_regex").expect("denylist_regex");
    let body = &src[at..];
    let open = body.find("{}(").unwrap() + 3;
    let close = open + body[open..].find(")?").unwrap();
    let set: Vec<String> = body[open..close].split('|').map(str::to_owned).collect();
    assert!(set.len() > 5, "suffix set parsed: {set:?}");
    set
}

struct Denylist {
    stems: Vec<String>,
    phrases: Vec<String>,
    suffixes: Vec<String>,
}

impl Denylist {
    fn load() -> Self {
        let p = engine_root().join("crates/rr_dr60_harness/src/manifest_checks.rs");
        let src = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        Self {
            stems: const_list(&src, "DENYLIST_STEMS"),
            phrases: const_list(&src, "DENYLIST_PHRASES"),
            suffixes: suffixes(&src),
        }
    }

    /// The first hit in `s`, by the engine's rule (whole words: a stem plus an optional suffix;
    /// phrases on word boundaries) and the supplementary list.
    fn hit(&self, s: &str) -> Option<String> {
        let lower = s.to_lowercase();
        let words: Vec<&str> = lower
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|w| !w.is_empty())
            .collect();
        for w in &words {
            for stem in &self.stems {
                if let Some(rest) = w.strip_prefix(stem.as_str())
                    && (rest.is_empty() || self.suffixes.iter().any(|x| x == rest))
                {
                    return Some((*w).to_owned());
                }
            }
            for sup in ["accura", "reveal", "uncover"] {
                if w.starts_with(sup) {
                    return Some((*w).to_owned());
                }
            }
        }
        for (i, w) in words.iter().enumerate() {
            if *w == "measured" && (i == 0 || words[i - 1] != "not") {
                return Some("measured".into());
            }
        }
        let joined = format!(" {} ", words.join(" "));
        for ph in self
            .phrases
            .iter()
            .map(String::as_str)
            .chain(["hidden voice"])
        {
            if joined.contains(&format!(" {ph} ")) {
                return Some(ph.to_owned());
            }
        }
        None
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
