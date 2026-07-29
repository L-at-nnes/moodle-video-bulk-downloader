mod tui;

use clap::Parser;
use crossterm::execute;
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use mvbd_core::{cookies, input, pipeline, tools, LinkEntry, RunOptions};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::path::PathBuf;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Télécharge les replays Moodle/UbiCast et les muxe en MKV.
#[derive(Parser, Debug)]
#[command(name = "mvbd", version, about)]
struct Args {
    /// URL Moodle (répétable)
    #[arg(long = "url", value_name = "URL")]
    urls: Vec<String>,

    /// Fichier texte avec cours et URLs
    #[arg(long, value_name = "FILE")]
    input: Option<PathBuf>,

    /// Fichier de cookies Netscape/KEY=VALUE
    #[arg(long = "cookie-file", value_name = "FILE", default_value = "cookies.txt")]
    cookie_file: PathBuf,

    /// Répertoire de sortie
    #[arg(long = "output-dir", value_name = "DIR", default_value = "dl")]
    output_dir: PathBuf,

    /// Liens traités en parallèle
    #[arg(long, default_value_t = 1)]
    concurrency: usize,

    /// Threads ffmpeg par flux
    #[arg(long = "download-threads", default_value_t = 4)]
    download_threads: usize,

    /// Attente maximale pour capturer m3u8 (ms)
    #[arg(long = "capture-wait-ms", default_value_t = 8000)]
    capture_wait_ms: u64,

    /// Timeout par téléchargement audio/vidéo (s)
    #[arg(long = "ffmpeg-timeout", default_value_t = 1800)]
    ffmpeg_timeout: u64,

    /// Affiche le navigateur (debug)
    #[arg(long = "show-browser")]
    show_browser: bool,

    /// Conserve les fichiers audio/vidéo séparés
    #[arg(long = "keep-temp")]
    keep_temp: bool,

    /// Simule sans télécharger
    #[arg(long = "dry-run")]
    dry_run: bool,

    /// Désactive le datage du fichier selon la date de publication de la vidéo
    #[arg(long = "no-set-file-date")]
    no_set_file_date: bool,

    /// Télécharge/prépare ffmpeg, mkvmerge et Chromium puis quitte (utilisé par le build portable)
    #[arg(long = "ensure-tools", hide = true)]
    ensure_tools: bool,
}

/// Windows consoles default to a legacy codepage (850/1252), which mangles
/// accented characters and the box-drawing glyphs the TUI uses. Switching to
/// UTF-8 before any output fixes it; other platforms are UTF-8 already.
#[cfg(windows)]
fn set_utf8_console() {
    #[link(name = "kernel32")]
    extern "system" {
        fn SetConsoleOutputCP(code_page_id: u32) -> i32;
        fn SetConsoleCP(code_page_id: u32) -> i32;
    }
    const CP_UTF8: u32 = 65001;
    unsafe {
        SetConsoleOutputCP(CP_UTF8);
        SetConsoleCP(CP_UTF8);
    }
}

#[cfg(not(windows))]
fn set_utf8_console() {}

#[tokio::main]
async fn main() {
    set_utf8_console();
    let args = Args::parse();
    match run(args).await {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }
}

async fn run(args: Args) -> mvbd_core::Result<i32> {
    let project_root = std::env::current_dir()?;

    if args.ensure_tools {
        let paths = tools::ensure_tools(&project_root).await?;
        println!("ffmpeg:   {}", paths.ffmpeg.display());
        println!("mkvmerge: {}", paths.mkvmerge.display());
        println!("chromium: {}", paths.chromium.display());
        return Ok(0);
    }

    if args.concurrency < 1 {
        return Err(mvbd_core::MvbdError::new("--concurrency doit être >= 1"));
    }
    if args.download_threads < 1 {
        return Err(mvbd_core::MvbdError::new("--download-threads doit être >= 1"));
    }
    if args.ffmpeg_timeout < 60 {
        return Err(mvbd_core::MvbdError::new("--ffmpeg-timeout doit être >= 60"));
    }

    let headless = !args.show_browser;

    println!("Préparation des outils (ffmpeg / mkvmerge / chromium)…");
    let tool_paths = tools::ensure_tools(&project_root).await?;

    let cookie_list = cookies::read_cookie_file(&args.cookie_file)?;
    let mut entries = input::build_entries(&args.urls, args.input.as_deref())?;

    println!("Authentification via cookies…");
    pipeline::verify_auth(&entries, &cookie_list, &tool_paths, headless).await?;

    println!("Scan des pages de cours…");
    entries = pipeline::expand_courses(entries, &cookie_list, &tool_paths, headless).await?;

    let opts = RunOptions {
        output_dir: args.output_dir,
        concurrency: args.concurrency,
        download_threads: args.download_threads,
        capture_wait_ms: args.capture_wait_ms,
        ffmpeg_timeout_s: args.ffmpeg_timeout,
        headless,
        keep_temp: args.keep_temp,
        dry_run: args.dry_run,
        set_file_date: !args.no_set_file_date,
    };

    run_with_tui(entries, cookie_list, tool_paths, opts).await
}

async fn run_with_tui(
    entries: Vec<LinkEntry>,
    cookies: Vec<mvbd_core::RawCookie>,
    tools: mvbd_core::ToolPaths,
    opts: RunOptions,
) -> mvbd_core::Result<i32> {
    let total = entries.len();
    let concurrency = opts.concurrency;
    let download_threads = opts.download_threads;

    let mut stdout = std::io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    let tui = Arc::new(tui::Tui::new(terminal, &entries, concurrency, download_threads));
    let cancel = CancellationToken::new();

    let ctrlc_cancel = cancel.clone();
    tokio::spawn(async move {
        loop {
            if tokio::signal::ctrl_c().await.is_err() {
                return;
            }
            ctrlc_cancel.cancel();
            mvbd_core::download::kill_all_procs();
        }
    });

    let tui_sink = tui.clone();
    let on_event: Arc<dyn Fn(mvbd_core::ProgressEvent) + Send + Sync> =
        Arc::new(move |event| tui_sink.handle(event));

    let summary = pipeline::run(entries, cookies, tools, opts, cancel.clone(), on_event).await;

    tui.teardown();
    disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)?;

    let summary = summary?;

    if cancel.is_cancelled() {
        return Ok(130);
    }

    if !summary.errors.is_empty() {
        eprintln!("\n{} erreur(s):", summary.errors.len());
        for (url, message) in &summary.errors {
            eprintln!("  • {url}: {}", &message[..message.len().min(120)]);
        }
        eprintln!("\n{}/{total} réussi(s)", summary.success_count);
        return Ok(1);
    }

    println!("\n✓ {}/{total} vidéo(s) téléchargée(s)", summary.success_count);
    Ok(0)
}
