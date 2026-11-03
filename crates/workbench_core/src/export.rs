//! The export pair: the render as a 32-bit float WAV and its sidecar (spec 001 FR-012; research
//! R-03). Both are written as partial files, then renamed; a failure leaves no pair and no
//! mixed pair.

use std::io::Cursor;
use std::path::{Path, PathBuf};

use crate::render::Render;
use crate::sidecar;

/// The file operations the export uses; the tests inject failures through it.
pub trait ExportFs {
    /// Creates (or truncates) `path`, writes `bytes`, flushes and closes it.
    fn write(&mut self, path: &Path, bytes: &[u8]) -> std::io::Result<()>;
    /// Renames `from` to `to`, replacing `to`.
    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()>;
    /// Removes `path` if it exists.
    fn remove(&mut self, path: &Path) -> std::io::Result<()>;
    /// Whether `path` exists.
    fn exists(&self, path: &Path) -> bool;
}

/// The real file system.
#[derive(Debug, Default)]
pub struct RealFs;

impl ExportFs for RealFs {
    fn write(&mut self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        use std::io::Write;
        let mut f = std::fs::File::create(path)?;
        f.write_all(bytes)?;
        f.sync_all()
    }
    fn rename(&mut self, from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::rename(from, to)
    }
    fn remove(&mut self, path: &Path) -> std::io::Result<()> {
        match std::fs::remove_file(path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

/// A refused or failed export.
#[derive(Debug)]
pub enum ExportError {
    /// The data would exceed the WAV format's size limit.
    TooLarge(u64),
    /// A file operation failed; nothing new was left behind.
    Io(std::io::Error),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge(n) => write!(
                f,
                "The render is too long for one WAV file ({n} bytes of samples; the limit is {})",
                MAX_DATA_BYTES
            ),
            Self::Io(e) => write!(f, "The export failed and was removed: {e}"),
        }
    }
}

impl std::error::Error for ExportError {}

/// hound writes WAVE_FORMAT_EXTENSIBLE for f32 (a 68-byte header; RIFF size = 60 + data).
pub const MAX_DATA_BYTES: u64 = u32::MAX as u64 - 60;

/// Refuses a render whose samples exceed the WAV format's size limit (research R-03).
///
/// # Errors
///
/// [`ExportError::TooLarge`].
pub fn check_size(samples: u64) -> Result<(), ExportError> {
    let data = samples.saturating_mul(4);
    if data > MAX_DATA_BYTES {
        return Err(ExportError::TooLarge(data));
    }
    Ok(())
}

/// The two paths for a stem (`name` → `name.wav`, `name.sidecar.txt`; a `.wav` on the stem is
/// dropped first).
pub fn paths(stem: &Path) -> (PathBuf, PathBuf) {
    let stem = if stem
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
    {
        stem.with_extension("")
    } else {
        stem.to_path_buf()
    };
    let mut wav = stem.clone().into_os_string();
    wav.push(".wav");
    let mut side = stem.into_os_string();
    side.push(".sidecar.txt");
    (PathBuf::from(wav), PathBuf::from(side))
}

/// The WAV bytes of `samples`: mono, 32-bit IEEE float, at `rate_hz`.
///
/// # Errors
///
/// [`ExportError::TooLarge`] above the format's limit.
pub fn wav_bytes(samples: &[f32], rate_hz: u32) -> Result<Vec<u8>, ExportError> {
    check_size(samples.len() as u64)?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate_hz,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let mut cur = Cursor::new(Vec::with_capacity(samples.len() * 4 + 68));
    {
        let mut w = hound::WavWriter::new(&mut cur, spec)
            .map_err(|e| ExportError::Io(std::io::Error::other(e)))?;
        for &s in samples {
            w.write_sample(s)
                .map_err(|e| ExportError::Io(std::io::Error::other(e)))?;
        }
        w.finalize()
            .map_err(|e| ExportError::Io(std::io::Error::other(e)))?;
    }
    Ok(cur.into_inner())
}

/// Writes the pair for `render` at `stem` (FR-012).
///
/// # Errors
///
/// The size refusal, or a file operation's failure after which neither a new pair nor a mixed
/// pair remains.
pub fn export(fs: &mut impl ExportFs, stem: &Path, render: &Render) -> Result<(), ExportError> {
    let wav = wav_bytes(&render.samples, render.request.rate_hz)?;
    let side = sidecar::write(render);
    let (wav_path, side_path) = paths(stem);
    let mut wav_part = wav_path.clone().into_os_string();
    wav_part.push(".partial");
    let wav_part = PathBuf::from(wav_part);
    let mut side_part = side_path.clone().into_os_string();
    side_part.push(".partial");
    let side_part = PathBuf::from(side_part);

    let cleanup = |fs: &mut dyn ExportFsDyn| {
        let _ = fs.remove_dyn(&wav_part);
        let _ = fs.remove_dyn(&side_part);
    };
    if let Err(e) = fs
        .write(&wav_part, &wav)
        .and_then(|()| fs.write(&side_part, side.as_bytes()))
    {
        cleanup(fs);
        return Err(ExportError::Io(e));
    }
    // The old sidecar goes first, so a reader never sees the new WAV beside the old sidecar.
    if let Err(e) = fs.remove(&side_path) {
        cleanup(fs);
        return Err(ExportError::Io(e));
    }
    if let Err(e) = fs.rename(&wav_part, &wav_path) {
        // The old WAV has lost its sidecar: remove it too, so no orphan remains.
        let _ = fs.remove(&wav_path);
        cleanup(fs);
        return Err(ExportError::Io(e));
    }
    if let Err(e) = fs.rename(&side_part, &side_path) {
        let _ = fs.remove(&wav_path);
        cleanup(fs);
        return Err(ExportError::Io(e));
    }
    Ok(())
}

/// Object-safe removal for the cleanup closure.
trait ExportFsDyn {
    fn remove_dyn(&mut self, p: &Path) -> std::io::Result<()>;
}

impl<T: ExportFs> ExportFsDyn for T {
    fn remove_dyn(&mut self, p: &Path) -> std::io::Result<()> {
        self.remove(p)
    }
}
