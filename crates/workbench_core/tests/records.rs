//! Spec 005 [TR: FR-001 – FR-004, SC-001, US1, US2]: processing records.

mod support;

use std::sync::atomic::{AtomicBool, AtomicU64};

use rr_dr60::changes::{ChangeEntry, Clock, MAX_CHANGES_PER_CALL};
use rr_dr60::record::{self, RecordReadout, RunData, StopFields, flags};
use rr_dr60::{Pipeline, Settings, StopResult, Stream, VasEvent};
use support::*;
use workbench_core::device::{Device, RecordEnd};
use workbench_core::input::Input;
use workbench_core::records::{self, FeedPoint, ReRenderError, Recipe, RunSource};
use workbench_core::render::{Scheduled, block_for};

fn idx(id: &str) -> u32 {
    rr_dr60::controls::controls()
        .iter()
        .position(|e| e.identifier == id)
        .unwrap() as u32
}

#[derive(Clone, Copy, Debug)]
struct Cell {
    codec: bool,
    changes: bool,
    mute: bool,
    self_stop: bool,
}

fn cells() -> Vec<Cell> {
    let mut v = Vec::new();
    for codec in [false, true] {
        for changes in [false, true] {
            for mute in [false, true] {
                for self_stop in [false, true] {
                    v.push(Cell {
                        codec,
                        changes,
                        mute,
                        self_stop,
                    });
                }
            }
        }
    }
    v
}

fn settings(rate: u32, cell: Cell) -> Settings {
    let mut p = workbench_core::panel::Panel::new(rate).unwrap();
    set_num(&mut p, "memory.capacity_s", 2.0);
    set_switch(&mut p, "speech_codec_enabled", cell.codec);
    set_choice(&mut p, "vas.mode", if cell.mute { "mute" } else { "drop" });
    assert!(p.renderable());
    p.snapshot().0
}

/// The stimulus: voice for 1.5 s, or for a self-stop a steady −20 dBFS tone of 3 s (the gate
/// stays open, so it fills the 2 s memory).
fn stimulus(rate: u32, cell: Cell) -> Vec<f32> {
    if cell.self_stop {
        tone(rate, 3.0, 0.1)
    } else {
        voiced(rate, 1.5, 0.5)
    }
}

/// The two record-side changes of a changes cell (the engine's harness's pair): the knee below
/// the voice's peaks at 0.3 s, the hiss +3 dB at 0.9 s.
fn cell_changes(rate: u32, cell: Cell) -> Vec<Scheduled> {
    if !cell.changes {
        return Vec::new();
    }
    let fs = u64::from(rate);
    vec![
        Scheduled {
            clock: Clock::Input,
            position: fs * 3 / 10,
            control: idx("agc.knee_dbfs"),
            value: -30.0,
            tag: 7,
        },
        Scheduled {
            clock: Clock::Input,
            position: fs * 9 / 10,
            control: idx("mic.hiss_offset_db"),
            value: 3.0,
            tag: 8,
        },
    ]
}

/// A recorded device: item 1 recorded from `input` at `start`.
fn recorded(rate: u32, cell: Cell, input: &Input, start: usize) -> (Device, u32) {
    let mut d = Device::new(settings(rate, cell)).unwrap();
    let out = d
        .record_from(
            &input.samples,
            RunSource::of(input, start as u64),
            start,
            None,
            &cell_changes(rate, cell),
        )
        .unwrap();
    let n = match out.end {
        RecordEnd::Kept { number, .. } => {
            assert!(!cell.self_stop, "{rate} {cell:?}: expected a self-stop");
            number
        }
        RecordEnd::SelfStopped { number, .. } => {
            assert!(cell.self_stop, "{rate} {cell:?}: unexpected self-stop");
            number
        }
        RecordEnd::DiscardedEmpty => panic!("{rate} {cell:?}: nothing kept"),
    };
    (d, n)
}

fn save(d: &Device, n: u32, input: &Input) -> records::RecordFiles {
    records::files(d, n, Some(input), "voice.record.txt").unwrap()
}

fn readout(bytes: &[u8]) -> RecordReadout {
    let mut r = RecordReadout::default();
    record::read(bytes, &mut r).unwrap();
    r
}

fn re_render(f: &records::RecordFiles, input: &Input) -> Result<records::Outcome, ReRenderError> {
    let prep = records::prepare(&f.record, &f.input_text, input)?;
    run_recipe(
        &prep.recipe,
        prep.readout.recording_digest,
        &input.samples[prep.span],
    )
}

fn run_recipe(
    recipe: &Recipe,
    expected: u64,
    fed: &[f32],
) -> Result<records::Outcome, ReRenderError> {
    records::re_render(
        recipe,
        expected,
        fed,
        &AtomicU64::new(0),
        &AtomicBool::new(false),
    )
}

/// A direct engine's record for the same input and calls (the oracle of AS1).
fn direct_record(rate: u32, cell: Cell, x: &[f32]) -> Vec<u8> {
    let mut p = Pipeline::new(settings(rate, cell)).unwrap();
    for c in cell_changes(rate, cell) {
        p.schedule_change(Stream::Record, c.position, c.control, c.value, c.tag)
            .unwrap();
    }
    p.start_recording().unwrap();
    let block = block_for(rate);
    let mut ev = vec![VasEvent::new(0, 0); p.max_record_events(rate as usize)];
    let mut log = vec![ChangeEntry::default(); MAX_CHANGES_PER_CALL];
    let (mut events, mut changes) = (Vec::new(), Vec::new());
    let mut pos = 0;
    let mut stop = None;
    while pos < x.len() {
        let n = block.min(x.len() - pos);
        let info = p
            .record_with_changes(&x[pos..pos + n], &mut ev, &mut log)
            .unwrap();
        events.extend_from_slice(&ev[..info.events]);
        changes.extend_from_slice(&log[..info.changes]);
        pos += n;
        if info.stopped.is_some() {
            stop = Some(StopFields::from(&info));
            break;
        }
    }
    let stop = stop.unwrap_or_else(|| {
        let out = p.stop_recording_with_changes(&mut ev, &mut log).unwrap();
        events.extend_from_slice(&ev[..out.flush.events]);
        changes.extend_from_slice(&log[..out.changes]);
        StopFields::from(&out)
    });
    let run = RunData {
        events: &events,
        changes: &changes,
        stop,
    };
    let mut out = vec![0u8; p.processing_record_len(1, &run).unwrap()];
    let n = p.processing_record_write(1, &run, &mut out).unwrap();
    out.truncate(n);
    out
}

#[test]
fn record_written_as_engine() {
    for rate in RATES {
        for cell in cells() {
            let x = stimulus(rate, cell);
            let input = input_of(rate, &x);
            let (d, n) = recorded(rate, cell, &input, 0);
            let f = save(&d, n, &input);
            assert_eq!(
                f.record,
                direct_record(rate, cell, &input.samples),
                "{rate} {cell:?}"
            );
            let mut r = RecordReadout::default();
            d.read_record(&f.record).unwrap();
            record::read(&f.record, &mut r).unwrap();
            assert_eq!(r.flags, 0, "{rate} {cell:?}: flags");
        }
    }
}

#[test]
fn re_render_reproduces() {
    for rate in RATES {
        for cell in cells() {
            let x = stimulus(rate, cell);
            let input = input_of(rate, &x);
            let (d, n) = recorded(rate, cell, &input, 0);
            let f = save(&d, n, &input);
            let o = re_render(&f, &input).unwrap();
            assert!(o.reproduces(), "{rate} {cell:?}: {o:?}");
            assert_eq!(
                o.expected,
                d.items()[n as usize - 1].digest,
                "{rate} {cell:?}"
            );
        }
    }
}

fn base(rate: u32, cell: Cell) -> (records::RecordFiles, Input, Recipe, u64) {
    let input = input_of(rate, &stimulus(rate, cell));
    let (d, n) = recorded(rate, cell, &input, 0);
    let f = save(&d, n, &input);
    let prep = records::prepare(&f.record, &f.input_text, &input).unwrap();
    let expected = prep.readout.recording_digest;
    (f, input, prep.recipe, expected)
}

#[test]
fn re_render_detects_deltas() {
    let plain = Cell {
        codec: false,
        changes: false,
        mute: false,
        self_stop: false,
    };
    for rate in RATES {
        let block = rate.div_ceil(6000);
        let differs = |cell: Cell, what: &str, edit: &dyn Fn(&mut Recipe)| {
            let (_, input, mut recipe, expected) = base(rate, cell);
            edit(&mut recipe);
            let o = run_recipe(&recipe, expected, &input.samples).unwrap();
            assert!(
                !o.reproduces(),
                "{rate} {what}: the delta did not change the digest"
            );
        };
        differs(plain, "seed", &|r| {
            r.settings.seed = r.settings.seed.wrapping_add(1)
        });
        differs(plain, "counter", &|r| r.counter += 1);
        differs(plain, "knee", &|r| r.settings.agc.knee_dbfs = -30.0);
        // The threshold: voice, then a 1 kHz tone of amplitude 0.006 (−44.4 dBFS), which the gate
        // reads between the record's −40 dBFS threshold and the delta's −43 through the
        // microphone stages (a probe at 6 kHz: 0.004 pauses at both, 0.0084 at neither), so it
        // pauses on it only without the delta.
        {
            let mut x = voiced(rate, 0.3, 0.5);
            x.extend(tone(rate, 1.6, 0.006));
            let input = input_of(rate, &x);
            let (d, n) = recorded(rate, plain, &input, 0);
            let f = save(&d, n, &input);
            let prep = records::prepare(&f.record, &f.input_text, &input).unwrap();
            let mut recipe = prep.recipe.clone();
            recipe.settings.vas.threshold_dbfs -= 3.0;
            let o = run_recipe(&recipe, prep.readout.recording_digest, &input.samples).unwrap();
            assert!(
                !o.reproduces(),
                "{rate} threshold: the delta did not change the digest"
            );
        }
        let changes_off = Cell {
            changes: true,
            ..plain
        };
        differs(changes_off, "hiss entry moved", &|r| {
            let h = idx("mic.hiss_offset_db");
            let e = r.changes.iter_mut().find(|e| e.control == h).unwrap();
            e.position += u64::from(block);
        });
        let changes_on = Cell {
            codec: true,
            changes: true,
            ..plain
        };
        differs(changes_on, "knee entry moved", &|r| {
            let k = idx("agc.knee_dbfs");
            let e = r.changes.iter_mut().find(|e| e.control == k).unwrap();
            e.position += u64::from(rate) / 10;
        });
        for codec in [false, true] {
            differs(Cell { codec, ..plain }, "stop earlier", &|r| {
                r.stop.stop_input_position -= 120 * u64::from(rate) / 6000;
            });
        }
        // No fill: a self-stop after one prior item, so the fill is needed (missing > 0).
        let ss = Cell {
            self_stop: true,
            ..plain
        };
        let mut d = Device::new(settings(rate, ss)).unwrap();
        d.record(&tone(rate, 0.5, 0.1), 0, None, &[]).unwrap();
        let input = input_of(rate, &stimulus(rate, ss));
        let out = d
            .record_from(&input.samples, RunSource::of(&input, 0), 0, None, &[])
            .unwrap();
        let RecordEnd::SelfStopped { number, .. } = out.end else {
            panic!("{rate}: {:?}", out.end)
        };
        let f = save(&d, number, &input);
        let prep = records::prepare(&f.record, &f.input_text, &input).unwrap();
        let fresh = Pipeline::new(prep.recipe.settings).unwrap();
        assert!(
            fresh.memory_status().remaining_samples > prep.recipe.remaining_at_start,
            "{rate}: the fill is needed"
        );
        let ok = run_recipe(&prep.recipe, prep.readout.recording_digest, &input.samples).unwrap();
        assert!(ok.reproduces(), "{rate}: with the fill");
        let mut nofill = prep.recipe.clone();
        nofill.fill = false;
        let o = run_recipe(&nofill, prep.readout.recording_digest, &input.samples).unwrap();
        assert!(!o.reproduces(), "{rate}: no fill");
        // The twin in the record's settings: the output clamp still reproduces.
        let (_, input, mut recipe, expected) = base(rate, plain);
        recipe.settings.output.clamp_enabled = !recipe.settings.output.clamp_enabled;
        assert!(
            run_recipe(&recipe, expected, &input.samples)
                .unwrap()
                .reproduces(),
            "{rate}: the clamp twin"
        );
    }
}

#[test]
fn live_value_twins_reproduce() {
    let plain = Cell {
        codec: false,
        changes: false,
        mute: false,
        self_stop: false,
    };
    for rate in [6000, 48_000] {
        let input = input_of(rate, &stimulus(rate, plain));
        for (id, v) in [
            (
                "volume",
                rr_dr60::control_map::ControlReading::Continuous(0.5),
            ),
            ("route", rr_dr60::control_map::ControlReading::Choice(1)),
            (
                "playback_speed",
                rr_dr60::control_map::ControlReading::Integer(5),
            ),
            (
                "battery_volts",
                rr_dr60::control_map::ControlReading::Continuous(2.4),
            ),
        ] {
            let mut d = Device::new(settings(rate, plain)).unwrap();
            d.set_value(idx(id), v).unwrap();
            d.record_from(&input.samples, RunSource::of(&input, 0), 0, None, &[])
                .unwrap();
            let f = save(&d, 1, &input);
            assert!(re_render(&f, &input).unwrap().reproduces(), "{rate} {id}");
        }
    }
}

#[test]
fn re_render_refuses_other_input() {
    let rate = 48_000;
    let cell = Cell {
        codec: false,
        changes: false,
        mute: false,
        self_stop: false,
    };
    let (f, input, _, _) = base(rate, cell);
    let other = input_of(rate, &tone(rate, 1.5, 0.2));
    let e = re_render(&f, &other).unwrap_err();
    assert_eq!(e, ReRenderError::OtherInput("voice.wav".into()));
    assert!(e.to_string().contains("voice.wav"));
    // Another channel of the same file's samples.
    let mut ch = input.clone();
    ch.channel = workbench_core::input::Channel::Mix;
    assert!(matches!(
        re_render(&f, &ch),
        Err(ReRenderError::OtherInput(_))
    ));
    // A record-input file of another record.
    let mut bytes = f.record.clone();
    bytes.push(b'\n');
    assert_eq!(
        records::prepare(&bytes, &f.input_text, &input).unwrap_err(),
        ReRenderError::NotThisRecord
    );
    // No record-input file.
    assert!(matches!(
        records::prepare(&f.record, "", &input),
        Err(ReRenderError::RecordInput(_))
    ));
}

#[test]
fn runs_follow_the_counter() {
    let rate = 8000;
    let cell = Cell {
        codec: false,
        changes: true,
        mute: false,
        self_stop: false,
    };
    let mut d = Device::new(settings(rate, cell)).unwrap();
    let a = input_of(rate, &voiced(rate, 0.5, 0.5));
    let b = input_of(rate, &voiced(rate, 0.6, 0.4));
    d.record_from(&a.samples, RunSource::of(&a, 0), 0, None, &[])
        .unwrap();
    d.record_from(
        &b.samples,
        RunSource::of(&b, 0),
        0,
        None,
        &cell_changes(rate, cell),
    )
    .unwrap();
    let before = d.processing_record(2).unwrap();
    d.erase(1).unwrap();
    let after = d.processing_record(1).unwrap();
    assert_eq!(before, after, "item 2 renumbered to 1 keeps its run");
    let f = save(&d, 1, &b);
    assert!(re_render(&f, &b).unwrap().reproduces());
}

#[test]
fn imported_item_record() {
    let rate = 16_000;
    let cell = Cell {
        codec: true,
        changes: false,
        mute: false,
        self_stop: false,
    };
    let input = input_of(rate, &stimulus(rate, cell));
    let (d, n) = recorded(rate, cell, &input, 0);
    let stored = d.export(n).unwrap();
    let mut e = Device::new(settings(rate, cell)).unwrap();
    let m = e.import(&stored).unwrap();
    assert!(e.run_of(m).is_none());
    let f = records::files(&e, m, None, "imported.record.txt").unwrap();
    let r = readout(&f.record);
    assert_eq!(r.origin, record::Origin::Imported);
    assert_ne!(r.flags & flags::IMPORTED_ORIGIN, 0);
    assert_eq!(r.recording_digest, e.items()[0].digest);
    let ri = records::read_record_input(&f.input_text).unwrap();
    assert!(ri.imported && ri.input_file.is_none() && ri.feed_point == FeedPoint::None);
    assert!(f.i24.is_none());
    assert_eq!(
        records::prepare(&f.record, &f.input_text, &input).unwrap_err(),
        ReRenderError::Imported
    );
}

#[test]
fn feed_point_rule() {
    let rate = 44_100;
    let cell = Cell {
        codec: true,
        changes: true,
        mute: false,
        self_stop: false,
    };
    // A 16-bit mono file is on the 24-bit grid.
    let x = stimulus(rate, cell);
    let input = workbench_core::input::open_bytes(
        "voice16.wav",
        wav_i16(rate, &to_i16(&x)),
        workbench_core::input::Channel::Index(0),
    )
    .unwrap();
    let (d, n) = recorded(rate, cell, &input, 0);
    let f = save(&d, n, &input);
    let ri = records::read_record_input(&f.input_text).unwrap();
    assert_eq!(ri.feed_point, FeedPoint::I24);
    let i24: Vec<i32> = f
        .i24
        .as_ref()
        .unwrap()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| i32::from_le_bytes(*c))
        .collect();
    let mut back = vec![0.0f32; i24.len()];
    rr_dr60::input::samples_from_i24(&i24, &mut back);
    let fed = ri.fed.unwrap() as usize;
    assert_eq!(back.len(), fed);
    assert!(
        back.iter()
            .zip(&input.samples[..fed])
            .all(|(a, b)| a.to_bits() == b.to_bits())
    );
    // The engine's own recipe feeds the decoded span from 0: the same digest.
    let prep = records::prepare(&f.record, &f.input_text, &input).unwrap();
    assert!(
        run_recipe(&prep.recipe, prep.readout.recording_digest, &back)
            .unwrap()
            .reproduces()
    );
    // A float input off the grid: f32, no span.
    let fl = workbench_core::input::open_bytes(
        "voice32.wav",
        wav_f32(rate, &x.iter().map(|v| v * 0.987_654_3).collect::<Vec<_>>()),
        workbench_core::input::Channel::Index(0),
    )
    .unwrap();
    assert!(!records::feed_point_i24(&fl.samples));
    let (d, n) = recorded(rate, cell, &fl, 0);
    let f = save(&d, n, &fl);
    assert_eq!(
        records::read_record_input(&f.input_text)
            .unwrap()
            .feed_point,
        FeedPoint::F32
    );
    assert!(f.i24.is_none());
    assert!(re_render(&f, &fl).unwrap().reproduces());
}

#[test]
fn re_render_from_offset() {
    let cell = Cell {
        codec: false,
        changes: true,
        mute: false,
        self_stop: false,
    };
    for rate in [6000, 48_000] {
        let mut x = tone(rate, 0.4, 0.05);
        x.extend(stimulus(rate, cell));
        let input = input_of(rate, &x);
        let start = (rate as usize * 2) / 5;
        let (d, n) = recorded(rate, cell, &input, start);
        let f = save(&d, n, &input);
        let ri = records::read_record_input(&f.input_text).unwrap();
        assert_eq!(ri.start, Some(start as u64));
        assert!(re_render(&f, &input).unwrap().reproduces(), "{rate}");
    }
}

#[test]
fn re_render_refills_schedule() {
    let rate = 8000;
    let cell = Cell {
        codec: false,
        changes: false,
        mute: true,
        self_stop: false,
    };
    let input = input_of(rate, &stimulus(rate, cell));
    // 72 hiss changes, 10 ms apart: more than the 64 pending.
    let changes: Vec<Scheduled> = (0..72)
        .map(|i| Scheduled {
            clock: Clock::Input,
            position: 100 + i * u64::from(rate) / 100,
            control: idx("mic.hiss_offset_db"),
            value: if i % 2 == 0 { 3.0 } else { -3.0 },
            tag: i + 1,
        })
        .collect();
    let mut d = Device::new(settings(rate, cell)).unwrap();
    d.record_from(&input.samples, RunSource::of(&input, 0), 0, None, &changes)
        .unwrap();
    let f = save(&d, 1, &input);
    let r = readout(&f.record);
    assert!(r.change_count >= 70, "{}", r.change_count);
    assert!(re_render(&f, &input).unwrap().reproduces());
}

#[test]
fn re_render_unknown_component_and_cancel() {
    let rate = 6000;
    let cell = Cell {
        codec: false,
        changes: false,
        mute: false,
        self_stop: false,
    };
    let (f, input, recipe, expected) = base(rate, cell);
    let mut r = readout(&f.record);
    r.flags |= flags::UNKNOWN_COMPONENT;
    assert_eq!(
        Recipe::of(&f.record, &r).unwrap_err(),
        ReRenderError::UnknownComponent
    );
    let e = records::re_render(
        &recipe,
        expected,
        &input.samples,
        &AtomicU64::new(0),
        &AtomicBool::new(true),
    )
    .unwrap_err();
    assert_eq!(e, ReRenderError::Cancelled);
}

#[test]
fn filler_has_the_length() {
    for enc in [rr_dr60::Encoding::Samples, rr_dr60::Encoding::Frames] {
        let units = 7u64;
        let samples = units * enc.unit_samples() as u64;
        let bytes = records::filler(enc, samples).unwrap();
        let mut s = Settings::new(6000);
        s.speech_codec_enabled = enc == rr_dr60::Encoding::Frames;
        let mut p = Pipeline::new(s).unwrap();
        let n = p.import(&bytes).unwrap();
        assert_eq!(p.recording_length(n).unwrap(), samples);
        let _ = StopResult::DiscardedEmpty;
    }
}

// --- US2 ---

#[test]
fn record_readout_shown() {
    let rate = 48_000;
    let cell = Cell {
        codec: true,
        changes: true,
        mute: false,
        self_stop: false,
    };
    let input = input_of(rate, &stimulus(rate, cell));
    let (d, n) = recorded(rate, cell, &input, 0);
    let f = save(&d, n, &input);
    let r = d.read_record(&f.record).unwrap();
    assert_eq!(r.host_rate_hz, rate);
    assert_eq!(r.recording_digest, d.items()[0].digest);
    assert_eq!(
        records::change_rows(&f.record, &r).len() as u64,
        r.change_count
    );
    assert!(records::flag_names(r.flags).is_empty());
    assert!(records::unknown_settings(&f.record, &r).is_empty());
    let run = d.run_of(n).unwrap();
    assert_eq!(records::change_rows(&f.record, &r), run.changes);
    assert_eq!(r.stop, run.stop);
}

#[test]
fn settings_diff_lists_differences() {
    let rate = 16_000;
    let cell = Cell {
        codec: false,
        changes: false,
        mute: false,
        self_stop: false,
    };
    let input = input_of(rate, &stimulus(rate, cell));
    let (d, n) = recorded(rate, cell, &input, 0);
    let f = save(&d, n, &input);
    let r = readout(&f.record);
    let mut panel = workbench_core::panel::Panel::new(rate).unwrap();
    set_num(&mut panel, "memory.capacity_s", 2.0);
    set_switch(&mut panel, "speech_codec_enabled", false);
    set_choice(&mut panel, "vas.mode", "drop");
    let vals = |p: &workbench_core::panel::Panel| -> Vec<_> {
        p.rows().iter().map(|r| (r.index, r.value)).collect()
    };
    assert!(
        records::settings_diff(&r, &vals(&panel))
            .unwrap()
            .unwrap()
            .is_empty()
    );
    set_num(&mut panel, "agc.knee_dbfs", -6.0);
    set_num(&mut panel, "vas.hang_ms", 300.0);
    let diff = records::settings_diff(&r, &vals(&panel)).unwrap().unwrap();
    let ids: Vec<_> = diff.iter().map(|d| d.identifier).collect();
    assert_eq!(ids, ["agc.knee_dbfs", "vas.hang_ms"]);
    // A live playback value is not in a record: never listed.
    set_num(&mut panel, "volume", 0.5);
    assert_eq!(
        records::settings_diff(&r, &vals(&panel))
            .unwrap()
            .unwrap()
            .len(),
        2
    );
    assert!(records::not_in_record().contains(&"volume"));
}

#[test]
fn check_against_item() {
    let rate = 8000;
    let cell = Cell {
        codec: false,
        changes: false,
        mute: false,
        self_stop: false,
    };
    let mut d = Device::new(settings(rate, cell)).unwrap();
    let a = input_of(rate, &voiced(rate, 0.5, 0.5));
    let b = input_of(rate, &voiced(rate, 0.7, 0.3));
    d.record_from(&a.samples, RunSource::of(&a, 0), 0, None, &[])
        .unwrap();
    d.record_from(&b.samples, RunSource::of(&b, 0), 0, None, &[])
        .unwrap();
    let f = save(&d, 2, &b);
    let mut r = d.read_record(&f.record).unwrap();
    assert!(d.check_record(2, &mut r).unwrap());
    assert_eq!(r.flags & flags::DIGEST_MISMATCH, 0);
    assert!(!d.check_record(1, &mut r).unwrap());
    assert_ne!(r.flags & flags::DIGEST_MISMATCH, 0);
    assert!(d.check_record(3, &mut r).is_err());
}

#[test]
fn record_refusals() {
    let rate = 6000;
    let cell = Cell {
        codec: false,
        changes: false,
        mute: false,
        self_stop: false,
    };
    let input = input_of(rate, &stimulus(rate, cell));
    let (d, n) = recorded(rate, cell, &input, 0);
    let f = save(&d, n, &input);
    let text = String::from_utf8(f.record.clone()).unwrap();
    // A changed byte (a seed digit).
    let at = text.find("seed = ").unwrap() + 7;
    let mut b = f.record.clone();
    b[at] = if b[at] == b'1' { b'2' } else { b'1' };
    assert!(d.read_record(&b).is_err());
    // A missing line.
    let cut: String = text
        .lines()
        .filter(|l| !l.starts_with("seed "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert!(d.read_record(cut.as_bytes()).is_err());
    // An unknown version.
    let v = text.replacen(
        &format!("{} = 2", record::VERSION_LINE_NAME),
        &format!("{} = 99", record::VERSION_LINE_NAME),
        1,
    );
    assert_ne!(v, text);
    let e = d.read_record(v.as_bytes()).unwrap_err();
    assert!(!e.to_string().is_empty());
}

#[test]
fn record_other_rate() {
    let cell = Cell {
        codec: false,
        changes: true,
        mute: false,
        self_stop: false,
    };
    let rate = 44_100;
    let input = input_of(rate, &stimulus(rate, cell));
    let (d, n) = recorded(rate, cell, &input, 0);
    let f = save(&d, n, &input);
    let other = Device::new(settings(48_000, cell)).unwrap();
    let r = other.read_record(&f.record).unwrap();
    assert_ne!(r.flags & flags::HOST_RATE_DIFFERS, 0);
    assert!(records::flag_names(r.flags).contains(&"HOST_RATE_DIFFERS".to_owned()));
    assert_ne!(r.flags & records::EXPECTED_DIFFER, 0);
    assert!(re_render(&f, &input).unwrap().reproduces());
}

#[test]
fn flag_names_every_bit() {
    let all = records::FLAG_NAMES.iter().fold(0, |a, (b, _)| a | b);
    assert_eq!(records::flag_names(all).len(), 8);
    for (b, name) in records::FLAG_NAMES {
        assert_eq!(records::flag_names(b), [name.to_owned()]);
    }
    assert_eq!(records::flag_names(1 << 20), ["0x100000".to_owned()]);
    assert_eq!(
        records::EXPECTED_DIFFER,
        flags::LIBRARY_DIFFERS
            | flags::STAGE_MODEL_DIFFERS
            | flags::HOST_RATE_DIFFERS
            | flags::UNKNOWN_SETTINGS
    );
}

#[test]
fn record_input_roundtrip_and_refusals() {
    let rate = 6000;
    let cell = Cell {
        codec: false,
        changes: false,
        mute: false,
        self_stop: false,
    };
    let (f, _, _, _) = base(rate, cell);
    let ri = records::read_record_input(&f.input_text).unwrap();
    assert_eq!(records::write_record_input(&ri), f.input_text);
    assert_eq!(ri.host_rate, rate);
    let missing: String = f
        .input_text
        .lines()
        .filter(|l| !l.starts_with("fed "))
        .map(|l| format!("{l}\n"))
        .collect();
    assert!(records::read_record_input(&missing).is_err());
    let unknown = format!("{}extra 1\n", f.input_text);
    assert!(records::read_record_input(&unknown).is_err());
    let v2 = f.input_text.replacen("record-input 1", "record-input 2", 1);
    assert!(records::read_record_input(&v2).is_err());
}

/// Plan review Q3: a hang change scheduled late raises the stop's report bound; the buffers are
/// sized when they are used, so the recording is kept and re-renders.
#[test]
fn late_hang_change_keeps_recording() {
    let cell = Cell {
        codec: true,
        changes: false,
        mute: true,
        self_stop: false,
    };
    for rate in [6000, 48_000] {
        let mut x = voiced(rate, 1.0, 0.5);
        x.extend(tone(rate, 0.3, 0.002));
        x.extend(voiced(rate, 0.2, 0.5));
        let input = input_of(rate, &x);
        let late = vec![Scheduled {
            clock: Clock::Input,
            position: u64::from(rate) * 9 / 10,
            control: idx("vas.hang_ms"),
            value: 50.0,
            tag: 3,
        }];
        let mut d = Device::new(settings(rate, cell)).unwrap();
        let out = d
            .record_from(&input.samples, RunSource::of(&input, 0), 0, None, &late)
            .unwrap();
        assert!(matches!(out.end, RecordEnd::Kept { .. }), "{rate}");
        let f = save(&d, 1, &input);
        assert!(re_render(&f, &input).unwrap().reproduces(), "{rate}");
    }
}

#[test]
fn companion_paths_beside_the_record() {
    let (t, i) = records::companion_paths(std::path::Path::new("/a/voice.record.txt"));
    assert_eq!(t, std::path::Path::new("/a/voice.record-input.txt"));
    assert_eq!(i, std::path::Path::new("/a/voice.record-input.i24"));
    let (t, _) = records::companion_paths(std::path::Path::new("/a/other.txt"));
    assert_eq!(t, std::path::Path::new("/a/other.record-input.txt"));
}

#[test]
fn negative_zero_equals_zero() {
    use rr_dr60::control_map::ControlReading;
    assert_eq!(
        ControlReading::Continuous(-0.0),
        ControlReading::Continuous(0.0)
    );
}
