//! Local model files and the rules that decide when one is ready.
//!
//! A download lands in a sibling `.<file>.part` and is renamed onto the final
//! name only after the body is complete and clears the size floor. A leftover
//! part file stays on disk so a failed download is visible as partial.

use std::path::{Path, PathBuf};

use crate::error::CoreError;

/// A finished model file must be at least this large. Anything smaller is a
/// truncated or empty body, never a usable model.
pub const MIN_READY_BYTES: u64 = 1024;

/// Where a model file stands on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadState {
    /// No file and no leftover part.
    Missing,
    /// A `.<file>.part` sibling exists. The model is not usable.
    Partial,
    /// The final file exists and is at least [`MIN_READY_BYTES`].
    Ready,
}

impl DownloadState {
    /// Wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Partial => "partial",
            Self::Ready => "ready",
        }
    }
}

/// Classify `dir/file_name` without touching the network.
///
/// A final file shorter than [`MIN_READY_BYTES`] is not ready, even when a
/// part file is also present. The part file wins only while the final file is
/// absent or too small.
#[must_use]
pub fn download_state(dir: &Path, file_name: &str) -> DownloadState {
    if file_len(&final_path(dir, file_name)) >= MIN_READY_BYTES {
        return DownloadState::Ready;
    }
    if part_path(dir, file_name).is_file() {
        return DownloadState::Partial;
    }
    DownloadState::Missing
}

/// The model file once a download has finished.
#[must_use]
pub fn final_path(dir: &Path, file_name: &str) -> PathBuf {
    dir.join(file_name)
}

/// The sibling written while a download is in flight.
///
/// The part name starts with a dot so it cannot collide with a published
/// model file name.
#[must_use]
pub fn part_path(dir: &Path, file_name: &str) -> PathBuf {
    dir.join(format!(".{file_name}.part"))
}

/// Write `bytes` and rename them onto the final name once they clear the floor.
///
/// A body shorter than [`MIN_READY_BYTES`] is kept as the part file and the
/// call fails. The final name is never created for a short body.
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] when the directory cannot be
/// created, the part file cannot be written, or the rename fails.
pub fn finish(dir: &Path, file_name: &str, bytes: &[u8]) -> Result<PathBuf, CoreError> {
    std::fs::create_dir_all(dir).map_err(|error| {
        CoreError::InvalidConfiguration(format!(
            "cannot create model directory {}: {error}",
            dir.display()
        ))
    })?;
    let part = part_path(dir, file_name);
    std::fs::write(&part, bytes).map_err(|error| {
        CoreError::InvalidConfiguration(format!(
            "cannot write model download {}: {error}",
            part.display()
        ))
    })?;
    if bytes.len() < usize::try_from(MIN_READY_BYTES).unwrap_or(usize::MAX) {
        return Err(CoreError::InvalidConfiguration(format!(
            "downloaded model {file_name} is {} bytes, below the {} byte minimum",
            bytes.len(),
            MIN_READY_BYTES
        )));
    }
    let final_path = final_path(dir, file_name);
    std::fs::rename(&part, &final_path).map_err(|error| {
        CoreError::InvalidConfiguration(format!(
            "cannot finish model download {}: {error}",
            final_path.display()
        ))
    })?;
    Ok(final_path)
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}
