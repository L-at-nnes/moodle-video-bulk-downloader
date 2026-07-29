use crate::error::{MvbdError, Result};
use std::path::Path;

/// Muxes a video-only and an audio-only file into a single MKV via `mkvmerge`.
pub async fn mux_to_mkv(
    mkvmerge_path: &Path,
    video: &Path,
    audio: &Path,
    output: &Path,
    timeout_s: u64,
) -> Result<()> {
    let output = output
        .to_str()
        .ok_or_else(|| MvbdError::new("output path is not valid UTF-8"))?;
    let video = video
        .to_str()
        .ok_or_else(|| MvbdError::new("video path is not valid UTF-8"))?;
    let audio = audio
        .to_str()
        .ok_or_else(|| MvbdError::new("audio path is not valid UTF-8"))?;

    crate::tools::run_cmd(
        mkvmerge_path,
        &["-o", output, video, audio],
        Some(timeout_s),
    )
    .await
}
