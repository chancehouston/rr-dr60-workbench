//! Spec 001 T029 [TR: FR-017; research R-14]: the engine's public `Pipeline` methods the
//! workbench calls, as a ratio, and the spec's Engine reach list asserted.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The pinned `rr_dr60`'s source directory, from `cargo metadata`.
fn engine_src() -> PathBuf {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata");
    let json = String::from_utf8(out.stdout).unwrap();
    // The package entry for rr_dr60: `"name":"rr_dr60",…"manifest_path":"…/rr_dr60/Cargo.toml"`.
    let at = json
        .find("\"name\":\"rr_dr60\"")
        .expect("rr_dr60 in metadata");
    let rest = &json[at..];
    let key = "\"manifest_path\":\"";
    let m = rest.find(key).unwrap() + key.len();
    let path = &rest[m..m + rest[m..].find('"').unwrap()];
    Path::new(path).parent().unwrap().join("src")
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

/// `pub fn` names at the top level of every `impl Pipeline {` block, excluding `#[doc(hidden)]`
/// items and `__` names (the engine's test hooks, which no caller can reach).
fn pipeline_methods() -> Vec<String> {
    let mut files = Vec::new();
    rs_files(&engine_src(), &mut files);
    let mut names = Vec::new();
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap();
        let lines: Vec<&str> = src.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            if lines[i].trim_start().starts_with("impl Pipeline {") && lines[i].starts_with("impl")
            {
                let mut depth = 0i32;
                let mut hidden = false;
                for line in &lines[i..] {
                    let t = line.trim_start();
                    if depth == 1 {
                        if t.starts_with("#[doc(hidden)]") {
                            hidden = true;
                        }
                        if let Some(rest) = t.strip_prefix("pub fn ") {
                            let name: String = rest
                                .chars()
                                .take_while(|c| c.is_alphanumeric() || *c == '_')
                                .collect();
                            if !hidden && !name.starts_with("__") {
                                names.push(name);
                            }
                            hidden = false;
                        } else if t.starts_with("fn ")
                            || t.starts_with("pub(crate) fn ")
                            || t == "}"
                        {
                            hidden = false;
                        }
                    }
                    depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
                    i += 1;
                    if depth == 0 {
                        break;
                    }
                }
            } else {
                i += 1;
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// The workbench's own sources (both crates' `src`).
fn workbench_source() -> String {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("crates");
    let mut files = Vec::new();
    rs_files(&root, &mut files);
    files
        .iter()
        .filter(|p| p.components().any(|c| c.as_os_str() == "src"))
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The spec's Engine reach list, as restated in its Clarifications (2026-11-02, implementation).
const LISTED: [&str; 42] = [
    "start_recording",
    "record_with_changes",
    "stop_recording_with_changes",
    "max_record_events",
    "erase",
    "erase_all",
    "export_len",
    "export",
    "import",
    "memory_status",
    "recording_length",
    "recording_counter_of",
    "recording_digest",
    "start_playback",
    "play_with_changes",
    "stop_playback",
    "playback_length_samples",
    "playback_length_samples_in_mode",
    "record_latency_samples",
    "state",
    "rest_meters",
    "control_value",
    "set_control_value",
    "pending_changes",
    "clear_schedule",
    "meters",
    "rest_meters",
    "new",
    "process",
    "process_with_events_and_changes",
    "stream_position",
    "input_position",
    "schedule_change",
    "reconfigure",
    "latency_samples",
    "drain_samples",
    "max_events",
    "settings",
    "control_value",
    "set_control_value",
    "recording_counter",
    "footprint_bytes",
];

/// FR-017's items outside `Pipeline`, and the stage manifest (coach 10).
const OUTSIDE: [&str; 7] = [
    "controls::controls()",
    "validate()",
    "setting_control(",
    "samples_from_i24(",
    "onepass_output_to_input(",
    "control_map::set_settings_value(",
    "rr_dr60::manifest()",
];

#[test]
fn outside_items_are_used() {
    let src = workbench_source();
    for item in OUTSIDE {
        assert!(
            src.contains(item),
            "FR-017 lists {item}, but the workbench does not call it"
        );
    }
}

#[test]
fn reach() {
    let methods = pipeline_methods();
    let src = workbench_source();
    // A name the workbench defines itself (`fn export(`, `fn play(`) counts only as
    // `Pipeline::name(`, so the workbench's own methods are not mistaken for the engine's.
    let called: Vec<&String> = methods
        .iter()
        .filter(|m| {
            let own = src.contains(&format!("fn {m}("));
            src.contains(&format!("Pipeline::{m}(")) || (!own && src.contains(&format!(".{m}(")))
        })
        .collect();
    let uncalled: Vec<&String> = methods.iter().filter(|m| !called.contains(m)).collect();
    println!("Pipeline reach: {}/{}", called.len(), methods.len());
    println!("called: {called:?}");
    println!("uncalled: {uncalled:?}");
    assert_eq!(
        methods.len(),
        70,
        "the public, reachable Pipeline methods at engine 0.92.0 (81 less the 11 `__` test hooks)"
    );
    for m in LISTED {
        assert!(
            methods.iter().any(|x| x == m),
            "{m} is not a Pipeline method"
        );
        assert!(
            called.iter().any(|x| *x == m),
            "the spec lists {m}, but the workbench does not call it"
        );
    }
}
