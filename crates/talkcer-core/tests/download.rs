//! File-state rules for a local model download.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use talkcer_core::{download_state, finish, DownloadState};

fn scratch() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "talkcer-download-{}-{}-{}",
        std::process::id(),
        nanos,
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn missing_file_is_missing() {
    let dir = TempDir(scratch());
    assert_eq!(download_state(&dir.0, "ggml-tiny.bin"), DownloadState::Missing);
}

#[test]
fn part_file_only_is_partial() {
    let dir = TempDir(scratch());
    fs::write(dir.0.join(".ggml-tiny.bin.part"), b"partial").unwrap();
    assert_eq!(download_state(&dir.0, "ggml-tiny.bin"), DownloadState::Partial);
}

#[test]
fn short_final_file_is_not_ready() {
    let dir = TempDir(scratch());
    fs::write(dir.0.join("ggml-tiny.bin"), vec![1; 10]).unwrap();
    assert_eq!(download_state(&dir.0, "ggml-tiny.bin"), DownloadState::Missing);
}

#[test]
fn final_file_at_the_floor_is_ready() {
    let dir = TempDir(scratch());
    fs::write(dir.0.join("ggml-tiny.bin"), vec![1; 2048]).unwrap();
    assert_eq!(download_state(&dir.0, "ggml-tiny.bin"), DownloadState::Ready);
}

#[test]
fn finish_renames_only_once_the_body_clears_the_floor() {
    let dir = TempDir(scratch());
    let error = finish(&dir.0, "ggml-tiny.bin", &[7; 10]).unwrap_err();
    assert!(error.to_string().contains("below the 1024 byte minimum"));
    assert!(dir.0.join(".ggml-tiny.bin.part").is_file());
    assert!(!dir.0.join("ggml-tiny.bin").exists());

    finish(&dir.0, "ggml-tiny.bin", &[9; 2048]).unwrap();
    assert!(dir.0.join("ggml-tiny.bin").is_file());
    assert!(!dir.0.join(".ggml-tiny.bin.part").exists());
    assert_eq!(download_state(&dir.0, "ggml-tiny.bin"), DownloadState::Ready);
}
