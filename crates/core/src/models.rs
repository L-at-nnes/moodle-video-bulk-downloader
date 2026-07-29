use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkEntry {
    pub url: String,
    pub course: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamInfo {
    pub title: String,
    pub audio_m3u8: String,
    pub video_m3u8: String,
    pub pub_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    Queued,
    Extracting,
    Downloading,
    Muxing,
    Retrying,
    Done,
    Error,
}

/// Options shared by both the CLI and the GUI to drive a download run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunOptions {
    pub output_dir: std::path::PathBuf,
    pub concurrency: usize,
    pub download_threads: usize,
    pub capture_wait_ms: u64,
    pub ffmpeg_timeout_s: u64,
    pub headless: bool,
    pub keep_temp: bool,
    pub dry_run: bool,
    pub set_file_date: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            output_dir: std::path::PathBuf::from("dl"),
            concurrency: 1,
            download_threads: 4,
            capture_wait_ms: 8000,
            ffmpeg_timeout_s: 1800,
            headless: true,
            keep_temp: false,
            dry_run: false,
            set_file_date: true,
        }
    }
}

/// One raw cookie parsed from a cookie file (name/value only — domain is
/// derived at injection time from the target URL, see `cookies.rs`).
#[derive(Debug, Clone)]
pub struct RawCookie {
    pub name: String,
    pub value: String,
}
