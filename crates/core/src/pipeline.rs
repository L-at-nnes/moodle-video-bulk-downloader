use crate::browser::BrowserHandle;
use crate::download::{download_and_mux, DownloadCallbacks};
use crate::error::{MvbdError, Result};
use crate::models::{LinkEntry, RawCookie, RunOptions, Status};
use crate::scraper::{extract_streams_from_page, scan_course_for_links};
use crate::tools::ToolPaths;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const MAX_RETRIES: u32 = 3;
const RETRY_BASE_DELAY_S: u64 = 5; // delays: 5s, 10s (exponential x2)

/// One update emitted while a run progresses. The CLI renders these into a
/// ratatui table, the GUI forwards them as Tauri events.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum ProgressEvent {
    Status { idx: usize, status: Status },
    Title { idx: usize, title: String },
    Progress { idx: usize, audio: Option<f64>, video: Option<f64> },
    Retry { idx: usize, attempt: u32 },
    Done { idx: usize, output_path: String },
    Error { idx: usize, message: String },
}

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub success_count: usize,
    pub errors: Vec<(String, String)>,
}

type EventSink = Arc<dyn Fn(ProgressEvent) + Send + Sync>;

/// Checks the given cookies actually authenticate against the Moodle
/// instance derived from the first entry's URL, failing fast before any
/// per-entry work starts. Mirrors `cli/auth.py::create_storage_state`, but
/// without the hardcoded `moodle.unine.ch` host.
pub async fn verify_auth(
    entries: &[LinkEntry],
    cookies: &[RawCookie],
    tools: &ToolPaths,
    headless: bool,
) -> Result<()> {
    let Some(first) = entries.first() else {
        return Ok(());
    };
    let parsed = url::Url::parse(&first.url).map_err(|e| MvbdError::new(e.to_string()))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| MvbdError::new(format!("URL has no host: {}", first.url)))?;
    let base_url = format!("{}://{host}/", parsed.scheme());

    let browser = BrowserHandle::launch(&tools.chromium, headless).await?;
    let outcome = browser.new_authenticated_page(cookies, &base_url).await;

    let result = match outcome {
        Ok(page) => match page.url().await {
            Ok(Some(current)) if current.contains("login/index.php") => Err(MvbdError::new(
                "Authentication failed (invalid/expired cookies or login flow not completed).",
            )),
            Ok(_) => Ok(()),
            Err(e) => Err(e.into()),
        },
        Err(e) => Err(e),
    };
    browser.shutdown().await;
    result
}

/// Expands any course-page URLs in `entries` into their individual activity
/// links. Mirrors the course-scan step in `cli/app.py::main`.
pub async fn expand_courses(
    entries: Vec<LinkEntry>,
    cookies: &[RawCookie],
    tools: &ToolPaths,
    headless: bool,
) -> Result<Vec<LinkEntry>> {
    let mut expanded = Vec::with_capacity(entries.len());

    for entry in entries {
        let is_course_url = entry.url.contains("/course/view.php") || entry.url.contains("/course/");
        if !is_course_url {
            expanded.push(entry);
            continue;
        }

        let found = async {
            let browser = BrowserHandle::launch(&tools.chromium, headless).await?;
            let page = browser.new_authenticated_page(cookies, &entry.url).await?;
            let links = scan_course_for_links(&page, &entry.url).await;
            browser.shutdown().await;
            links
        }
        .await;

        match found {
            Ok(links) if !links.is_empty() => expanded.extend(links),
            _ => expanded.push(entry),
        }
    }

    Ok(expanded)
}

async fn process_one_link(
    idx: usize,
    entry: LinkEntry,
    cookies: Arc<Vec<RawCookie>>,
    tools: Arc<ToolPaths>,
    opts: Arc<RunOptions>,
    cancel: CancellationToken,
    on_event: EventSink,
) -> (LinkEntry, Option<PathBuf>, Option<String>) {
    let mut last_error: Option<String> = None;

    for attempt in 0..MAX_RETRIES {
        if cancel.is_cancelled() {
            return (entry, None, Some("Interrompu".to_string()));
        }

        if attempt > 0 {
            on_event(ProgressEvent::Retry { idx, attempt });
            let delay = Duration::from_secs(RETRY_BASE_DELAY_S * (1 << (attempt - 1)));
            tokio::select! {
                _ = tokio::time::sleep(delay) => {}
                _ = cancel.cancelled() => return (entry, None, Some("Interrompu".to_string())),
            }
        }

        on_event(ProgressEvent::Status { idx, status: Status::Extracting });

        match run_attempt(idx, &entry, &cookies, &tools, &opts, &cancel, &on_event).await {
            Ok(output) => return (entry, Some(output), None),
            Err(e) => last_error = Some(e.to_string()),
        }
    }

    let message = last_error.unwrap_or_else(|| "Erreur inconnue".to_string());
    on_event(ProgressEvent::Error { idx, message: message.clone() });
    (entry, None, Some(message))
}

async fn run_attempt(
    idx: usize,
    entry: &LinkEntry,
    cookies: &[RawCookie],
    tools: &ToolPaths,
    opts: &RunOptions,
    cancel: &CancellationToken,
    on_event: &EventSink,
) -> Result<PathBuf> {
    let stream_info = {
        let browser = BrowserHandle::launch(&tools.chromium, opts.headless).await?;
        let page_result = browser.new_authenticated_page(cookies, &entry.url).await;
        let extraction = match page_result {
            Ok(page) => extract_streams_from_page(&page, &entry.url, opts.capture_wait_ms, cancel).await,
            Err(e) => Err(e),
        };
        browser.shutdown().await;
        extraction?
    };

    if cancel.is_cancelled() {
        return Err(MvbdError::new("Interrompu"));
    }

    on_event(ProgressEvent::Title { idx, title: stream_info.title.clone() });
    let target_dir = opts.output_dir.join(crate::util::sanitize_name(&entry.course, "cours"));

    if opts.dry_run {
        let final_output = crate::util::unique_output_path(&target_dir, &stream_info.title);
        on_event(ProgressEvent::Done { idx, output_path: final_output.display().to_string() });
        return Ok(final_output);
    }

    on_event(ProgressEvent::Status { idx, status: Status::Downloading });

    let audio_sink = on_event.clone();
    let video_sink = on_event.clone();
    let mux_sink = on_event.clone();
    let callbacks = DownloadCallbacks {
        on_audio_progress: Box::new(move |pct| {
            audio_sink(ProgressEvent::Progress { idx, audio: Some(pct), video: None })
        }),
        on_video_progress: Box::new(move |pct| {
            video_sink(ProgressEvent::Progress { idx, audio: None, video: Some(pct) })
        }),
        on_mux_start: Box::new(move || mux_sink(ProgressEvent::Status { idx, status: Status::Muxing })),
    };

    let output_file = download_and_mux(
        &stream_info,
        &target_dir,
        &tools.ffmpeg,
        &tools.mkvmerge,
        opts.download_threads,
        opts.ffmpeg_timeout_s,
        opts.keep_temp,
        &callbacks,
        cancel,
    )
    .await?;

    if cancel.is_cancelled() {
        return Err(MvbdError::new("Interrompu"));
    }

    if opts.set_file_date {
        if let Some(pub_date) = stream_info.pub_date {
            let ts = filetime::FileTime::from_unix_time(pub_date.timestamp(), 0);
            let _ = filetime::set_file_times(&output_file, ts, ts);
        }
    }

    on_event(ProgressEvent::Done { idx, output_path: output_file.display().to_string() });
    Ok(output_file)
}

/// Runs the full pipeline (extract -> download -> mux, with retry/backoff)
/// over every entry, bounded by `opts.concurrency`, reporting progress via
/// `on_event`. Mirrors `cli/app.py::process_one_link` + its executor loop.
pub async fn run(
    entries: Vec<LinkEntry>,
    cookies: Vec<RawCookie>,
    tools: ToolPaths,
    opts: RunOptions,
    cancel: CancellationToken,
    on_event: EventSink,
) -> Result<RunSummary> {
    tokio::fs::create_dir_all(&opts.output_dir).await?;

    let cookies = Arc::new(cookies);
    let tools = Arc::new(tools);
    let concurrency = opts.concurrency.max(1);
    let opts = Arc::new(opts);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

    let mut handles = Vec::with_capacity(entries.len());
    for (idx, entry) in entries.into_iter().enumerate() {
        let semaphore = semaphore.clone();
        let cookies = cookies.clone();
        let tools = tools.clone();
        let opts = opts.clone();
        let cancel = cancel.clone();
        let on_event = on_event.clone();

        handles.push(tokio::spawn(async move {
            let _permit = semaphore.acquire_owned().await.expect("semaphore not closed");
            process_one_link(idx, entry, cookies, tools, opts, cancel, on_event).await
        }));
    }

    let mut success_count = 0;
    let mut errors = Vec::new();
    for handle in handles {
        match handle.await {
            Ok((entry, Some(_), None)) => {
                let _ = entry;
                success_count += 1;
            }
            Ok((entry, _, Some(message))) => errors.push((entry.url, message)),
            Ok((entry, None, None)) => errors.push((entry.url, "Erreur inconnue".to_string())),
            Err(join_err) => errors.push(("?".to_string(), join_err.to_string())),
        }
    }

    Ok(RunSummary { success_count, errors })
}
