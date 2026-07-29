use crate::error::{MvbdError, Result};
use crate::models::StreamInfo;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

static ACTIVE_PIDS: LazyLock<Mutex<Vec<u32>>> = LazyLock::new(|| Mutex::new(Vec::new()));

fn register_pid(pid: u32) {
    ACTIVE_PIDS.lock().unwrap().push(pid);
}

fn unregister_pid(pid: u32) {
    let mut pids = ACTIVE_PIDS.lock().unwrap();
    if let Some(pos) = pids.iter().position(|p| *p == pid) {
        pids.remove(pos);
    }
}

/// Kills every ffmpeg/mkvmerge child process currently tracked by the
/// process registry. Called on Ctrl+C (CLI) or Cancel (GUI).
pub fn kill_all_procs() {
    let pids: Vec<u32> = ACTIVE_PIDS.lock().unwrap().clone();
    for pid in pids {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .output();
    }
}

pub struct DownloadCallbacks {
    pub on_audio_progress: Box<dyn Fn(f64) + Send + Sync>,
    pub on_video_progress: Box<dyn Fn(f64) + Send + Sync>,
    pub on_mux_start: Box<dyn Fn() + Send + Sync>,
}

fn parse_hls_duration(playlist: &str) -> f64 {
    playlist
        .lines()
        .filter_map(|line| line.strip_prefix("#EXTINF:"))
        .filter_map(|rest| rest.split(',').next())
        .filter_map(|s| s.trim().parse::<f64>().ok())
        .sum()
}

async fn hls_duration(url: &str) -> Option<f64> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .ok()?;
    let resp = client.get(url).send().await.ok()?;
    let resp = resp.error_for_status().ok()?;
    let text = resp.text().await.ok()?;
    let total = parse_hls_duration(&text);
    (total > 0.0).then_some(total)
}

/// Progress percent against the known HLS duration, capped at 99% — the
/// final 100% is only emitted once ffmpeg has actually exited successfully.
fn progress_pct(sec: f64, duration: f64) -> f64 {
    (sec / duration * 100.0).min(99.0)
}

async fn ffmpeg_dl(
    ffmpeg_path: &Path,
    url: &str,
    output_file: &Path,
    threads: usize,
    timeout_s: u64,
    on_progress: &(dyn Fn(f64) + Send + Sync),
    cancel: &CancellationToken,
) -> Result<()> {
    let duration = hls_duration(url).await;
    let file_name = output_file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    let threads_str = threads.to_string();
    let mut cmd = Command::new(ffmpeg_path);
    cmd.args([
        "-y",
        "-nostdin",
        "-loglevel",
        "error",
        "-threads",
        threads_str.as_str(),
        "-i",
        url,
        "-c",
        "copy",
        "-progress",
        "pipe:1",
        "-nostats",
    ]);
    cmd.arg(output_file);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn()?;
    let pid = child.id();
    if let Some(pid) = pid {
        register_pid(pid);
    }

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let stderr_task = tokio::spawn(async move {
        let mut buf = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut buf).await;
        buf
    });

    let mut lines = BufReader::new(stdout).lines();
    let start = tokio::time::Instant::now();
    let mut last_s = 0.0f64;

    let outcome: Result<()> = loop {
        if cancel.is_cancelled() {
            let _ = child.start_kill();
            break Err(MvbdError::new("Interrompu"));
        }
        if start.elapsed() > Duration::from_secs(timeout_s) {
            let _ = child.start_kill();
            break Err(MvbdError::new(format!(
                "Timeout ({timeout_s}s) pour {file_name}"
            )));
        }

        match tokio::time::timeout(Duration::from_millis(200), lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                if let Some(rest) = line.strip_prefix("out_time_ms=") {
                    if let (Some(duration), Ok(us)) = (duration, rest.trim().parse::<i64>()) {
                        let sec = us as f64 / 1_000_000.0;
                        if sec > last_s {
                            on_progress(progress_pct(sec, duration));
                            last_s = sec;
                        }
                    }
                }
            }
            Ok(Ok(None)) => break Ok(()),
            Ok(Err(e)) => break Err(e.into()),
            Err(_) => {}
        }
    };

    if let Some(pid) = pid {
        unregister_pid(pid);
    }
    outcome?;

    let status = child.wait().await?;
    if !status.success() {
        let stderr_buf = stderr_task.await.unwrap_or_default();
        let truncated: String = stderr_buf.trim().chars().take(200).collect();
        return Err(MvbdError::new(format!(
            "ffmpeg a échoué ({file_name}): {truncated}"
        )));
    }

    on_progress(100.0);
    Ok(())
}

/// Downloads the audio and video HLS streams in parallel via ffmpeg, then
/// muxes them into the final MKV (via `crate::mux::mux_to_mkv`).
/// Mirrors `cli/downloader.py::download_and_mux`.
pub async fn download_and_mux(
    stream_info: &StreamInfo,
    output_dir: &Path,
    ffmpeg_path: &Path,
    mkvmerge_path: &Path,
    download_threads: usize,
    ffmpeg_timeout_s: u64,
    keep_temp: bool,
    callbacks: &DownloadCallbacks,
    cancel: &CancellationToken,
) -> Result<PathBuf> {
    tokio::fs::create_dir_all(output_dir).await?;

    let tmp_dir = tempfile::Builder::new().prefix("moodle-dl-").tempdir()?;
    let audio_file = tmp_dir.path().join("audio.m4a");
    let video_file = tmp_dir.path().join("video.mp4");

    let (audio_res, video_res) = tokio::join!(
        ffmpeg_dl(
            ffmpeg_path,
            &stream_info.audio_m3u8,
            &audio_file,
            download_threads,
            ffmpeg_timeout_s,
            callbacks.on_audio_progress.as_ref(),
            cancel,
        ),
        ffmpeg_dl(
            ffmpeg_path,
            &stream_info.video_m3u8,
            &video_file,
            download_threads,
            ffmpeg_timeout_s,
            callbacks.on_video_progress.as_ref(),
            cancel,
        ),
    );
    audio_res?;
    video_res?;

    if cancel.is_cancelled() {
        return Err(MvbdError::new("Interrompu"));
    }

    (callbacks.on_mux_start)();

    let final_output = crate::util::unique_output_path(output_dir, &stream_info.title);
    crate::mux::mux_to_mkv(
        mkvmerge_path,
        &video_file,
        &audio_file,
        &final_output,
        ffmpeg_timeout_s.max(300),
    )
    .await?;

    if keep_temp {
        let parent = final_output.parent().unwrap_or_else(|| Path::new("."));
        let stem = final_output
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("output");
        tokio::fs::copy(&audio_file, parent.join(format!("{stem}.audio.m4a"))).await?;
        tokio::fs::copy(&video_file, parent.join(format!("{stem}.video.mp4"))).await?;
    }

    Ok(final_output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hls_extinf_durations() {
        let playlist = "#EXTM3U\n#EXTINF:6.006,\nseg0.ts\n#EXTINF:6.006,\nseg1.ts\n#EXTINF:3.5,\nseg2.ts\n";
        let total = parse_hls_duration(playlist);
        assert!((total - 15.512).abs() < 1e-6);
    }

    #[test]
    fn ignores_non_extinf_lines() {
        let playlist = "#EXTM3U\n#EXT-X-VERSION:3\nseg0.ts\n";
        assert_eq!(parse_hls_duration(playlist), 0.0);
    }

    #[test]
    fn caps_progress_at_99_percent() {
        assert_eq!(progress_pct(120.0, 100.0), 99.0);
        assert!((progress_pct(50.0, 100.0) - 50.0).abs() < 1e-9);
    }
}
