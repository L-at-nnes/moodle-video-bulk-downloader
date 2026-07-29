use crate::error::{MvbdError, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

/// Absolute paths to the three external binaries the pipeline shells out to.
#[derive(Debug, Clone)]
pub struct ToolPaths {
    pub ffmpeg: PathBuf,
    pub mkvmerge: PathBuf,
    pub chromium: PathBuf,
}

pub async fn ensure_tools(project_root: &Path) -> Result<ToolPaths> {
    let ffmpeg = ensure_ffmpeg(project_root).await?;
    let mkvmerge = ensure_mkvmerge(project_root).await?;
    let chromium = ensure_chromium(project_root).await?;
    Ok(ToolPaths {
        ffmpeg,
        mkvmerge,
        chromium,
    })
}

/// Locates `tools/ffmpeg.exe`, falling back to a system `ffmpeg` on PATH
/// (copied into `tools/` for next time).
pub async fn ensure_ffmpeg(project_root: &Path) -> Result<PathBuf> {
    let tools_dir = project_root.join("tools");
    let local_ffmpeg = tools_dir.join("ffmpeg.exe");
    if local_ffmpeg.exists() {
        return Ok(local_ffmpeg);
    }

    if let Some(ffmpeg) = which("ffmpeg") {
        tokio::fs::create_dir_all(&tools_dir).await?;
        if !local_ffmpeg.exists() && tokio::fs::copy(&ffmpeg, &local_ffmpeg).await.is_ok() {
            return Ok(local_ffmpeg);
        }
        return Ok(ffmpeg);
    }

    Err(MvbdError::new(
        "ffmpeg not found. Install ffmpeg and make sure it's on PATH.",
    ))
}

fn find_installed_mkvmerge() -> Option<PathBuf> {
    if let Some(p) = which("mkvmerge") {
        return Some(p);
    }

    let mut candidates = vec![
        PathBuf::from(r"C:\Program Files\MKVToolNix\mkvmerge.exe"),
        PathBuf::from(r"C:\Program Files (x86)\MKVToolNix\mkvmerge.exe"),
    ];

    if let Ok(local_app) = std::env::var("LOCALAPPDATA") {
        let local = PathBuf::from(&local_app);
        candidates.push(local.join("Programs").join("MKVToolNix").join("mkvmerge.exe"));

        let pattern = local
            .join("Microsoft")
            .join("WinGet")
            .join("Packages")
            .join("*MKVToolNix*")
            .join("mkvmerge.exe");
        if let Some(pattern) = pattern.to_str() {
            if let Ok(entries) = glob::glob(pattern) {
                for entry in entries.flatten() {
                    candidates.push(entry);
                }
            }
        }
    }

    candidates.into_iter().find(|c| c.exists())
}

/// Locates `tools/mkvmerge.exe`, falling back to an installed MKVToolNix or
/// a silent `winget install`.
pub async fn ensure_mkvmerge(project_root: &Path) -> Result<PathBuf> {
    let tools_dir = project_root.join("tools");
    let local_mkvmerge = tools_dir.join("mkvmerge.exe");
    if local_mkvmerge.exists() {
        return Ok(local_mkvmerge);
    }

    let mut installed = find_installed_mkvmerge();
    if installed.is_none() {
        let winget = which("winget")
            .ok_or_else(|| MvbdError::new("MKVToolNix is missing and winget is not available."))?;

        run_cmd(
            &winget,
            &[
                "install",
                "--id",
                "MoritzBunkus.MKVToolNix",
                "--source",
                "winget",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--silent",
            ],
            None,
        )
        .await?;

        installed = find_installed_mkvmerge();
    }

    let installed =
        installed.ok_or_else(|| MvbdError::new("Could not find MKVToolNix after installation."))?;

    tokio::fs::create_dir_all(&tools_dir).await?;
    tokio::fs::copy(&installed, &local_mkvmerge).await?;

    if !local_mkvmerge.exists() {
        return Err(MvbdError::new("Failed to copy mkvmerge.exe into tools/"));
    }

    Ok(local_mkvmerge)
}

/// Locates `tools/chromium/`, downloading a matching revision via
/// `chromiumoxide_fetcher` if missing. Idempotent: `BrowserFetcher::fetch`
/// reuses the existing install when the executable is already present.
pub async fn ensure_chromium(project_root: &Path) -> Result<PathBuf> {
    let chromium_dir = project_root.join("tools").join("chromium");
    tokio::fs::create_dir_all(&chromium_dir).await?;

    let options = chromiumoxide_fetcher::BrowserFetcherOptions::builder()
        .with_path(chromium_dir)
        .build()
        .map_err(|e| MvbdError::new(e.to_string()))?;

    let installation = chromiumoxide_fetcher::BrowserFetcher::new(options)
        .fetch()
        .await
        .map_err(|e| MvbdError::new(e.to_string()))?;

    Ok(installation.executable_path)
}

/// Minimal `shutil.which` port: scans `PATH` for `name` (and `name` suffixed
/// with each `PATHEXT` extension on Windows).
fn which(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    let exts: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".EXE;.CMD;.BAT;.COM".to_string())
        .split(';')
        .map(|s| s.to_string())
        .collect();

    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
        for ext in &exts {
            let candidate = dir.join(format!("{name}{ext}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Port of `cli/utils.py::run_cmd`: runs `program` to completion, capturing
/// combined output, failing on non-zero exit or timeout.
pub(crate) async fn run_cmd(program: &Path, args: &[&str], timeout_s: Option<u64>) -> Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let child = cmd.spawn()?;
    let output = match timeout_s {
        Some(secs) => {
            tokio::time::timeout(Duration::from_secs(secs), child.wait_with_output())
                .await
                .map_err(|_| {
                    MvbdError::new(format!(
                        "Timeout after {secs}s for command: {} {}",
                        program.display(),
                        args.join(" ")
                    ))
                })??
        }
        None => child.wait_with_output().await?,
    };

    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MvbdError::new(format!(
            "Command failed: {} {}\nstdout: {}\nstderr: {}",
            program.display(),
            args.join(" "),
            stdout.trim(),
            stderr.trim()
        )));
    }

    Ok(())
}
