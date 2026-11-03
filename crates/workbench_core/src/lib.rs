//! The workbench's logic, with no UI dependency (constitution principle V).
//!
//! Spec 001, the offline render session: [`input`] opens and converts a file, [`panel`] presents
//! and checks every manifest control, [`render`] runs a fresh engine over the input and a tail,
//! [`ab`] maps positions between the takes, [`monitor`] is the playback callback's body, and
//! [`export`] and [`sidecar`] write a reproducible pair. Every string is in [`text`].

pub mod ab;
pub mod analysis;
pub mod csv;
pub mod device;
pub mod export;
pub mod input;
pub mod live;
pub mod meters;
pub mod monitor;
pub mod panel;
pub mod render;
pub mod session;
pub mod sidecar;
pub mod stages;
pub mod taps;
pub mod text;

/// The engine version this build links (the pinned commit's `rr_dr60::VERSION`).
pub fn engine_version() -> &'static str {
    rr_dr60::VERSION
}

/// The pinned engine commit, read from `Cargo.lock` at build time (research R-12).
pub fn engine_commit() -> &'static str {
    env!("RR_DR60_COMMIT")
}

/// The workbench version.
pub fn workbench_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
