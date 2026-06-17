import argparse
import concurrent.futures
import os
import signal
import threading
import time
from pathlib import Path
from typing import Iterable, Optional

from playwright.sync_api import sync_playwright
from rich.console import Console

from .auth import create_storage_state
from .constants import DEFAULT_OUTPUT_DIR
from .downloader import download_and_mux, kill_all_procs
from .input_parsing import build_entries
from .models import CliError, LinkEntry
from .scraper import scan_course_for_ubicast_links
from .streams import extract_streams_from_page
from .system_tools import ensure_local_mkvtoolnix, find_ffmpeg
from .ui import DownloadUI
from .utils import sanitize_name, unique_output_path

MAX_RETRIES = 3
RETRY_BASE_DELAY_S = 5  # delays: 5s, 10s (exponential × 2)

_shutdown = threading.Event()


def _sigint(signum, frame):
    _shutdown.set()
    kill_all_procs()
    os._exit(130)


def process_one_link(
    idx: int,
    entry: LinkEntry,
    storage_state_path: Path,
    output_root: Path,
    ffmpeg_path: str,
    mkvmerge_path: Path,
    headless: bool,
    capture_wait_ms: int,
    download_threads: int,
    ffmpeg_timeout_s: int,
    keep_temp: bool,
    dry_run: bool,
    set_file_date: bool,
    ui: DownloadUI,
) -> tuple[LinkEntry, Optional[Path], Optional[str]]:
    last_error: Optional[str] = None

    for attempt in range(MAX_RETRIES):
        if _shutdown.is_set():
            return entry, None, "Interrompu"
        try:
            if attempt > 0:
                ui.mark_retry(idx, attempt)
                time.sleep(RETRY_BASE_DELAY_S * (2 ** (attempt - 1)))

            ui.set_status(idx, "extracting")

            with sync_playwright() as p:
                browser = p.chromium.launch(headless=headless)
                stream_info = extract_streams_from_page(
                    browser=browser,
                    storage_state_path=str(storage_state_path),
                    url=entry.url,
                    wait_ms=capture_wait_ms,
                    shutdown=_shutdown,
                )
                browser.close()

            if _shutdown.is_set():
                return entry, None, "Interrompu"

            ui.set_title(idx, stream_info.title)
            target_dir = output_root / sanitize_name(entry.course, "cours")

            if dry_run:
                final_output = unique_output_path(target_dir, stream_info.title)
                ui.mark_done(idx, str(final_output))
                return entry, final_output, None

            ui.set_status(idx, "downloading")

            output_file = download_and_mux(
                stream_info=stream_info,
                output_dir=target_dir,
                ffmpeg_path=ffmpeg_path,
                mkvmerge_path=mkvmerge_path,
                download_threads=download_threads,
                ffmpeg_timeout_s=ffmpeg_timeout_s,
                keep_temp=keep_temp,
                on_audio_progress=lambda pct: ui.set_progress(idx, audio=pct),
                on_video_progress=lambda pct: ui.set_progress(idx, video=pct),
                on_mux_start=lambda: ui.set_status(idx, "muxing"),
                shutdown=_shutdown,
            )

            if _shutdown.is_set():
                return entry, None, "Interrompu"

            if set_file_date and stream_info.pub_date:
                try:
                    ts = stream_info.pub_date.timestamp()
                    os.utime(str(output_file), (ts, ts))
                except Exception:
                    pass

            ui.mark_done(idx, str(output_file))
            return entry, output_file, None

        except Exception as exc:
            last_error = str(exc)

    ui.mark_error(idx, last_error or "Erreur inconnue")
    return entry, None, last_error


def parse_args(argv: Optional[Iterable[str]] = None) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Télécharge les replays Moodle/UbiCast et les muxe en MKV.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--url", action="append", default=[], metavar="URL", help="URL Moodle (répétable)")
    p.add_argument("--input", type=Path, metavar="FILE", help="Fichier texte avec cours et URLs")
    p.add_argument("--cookie-file", type=Path, default=Path("cookies.txt"), metavar="FILE", help="Fichier de cookies Netscape/KEY=VALUE")
    p.add_argument("--output-dir", type=Path, default=Path(DEFAULT_OUTPUT_DIR), metavar="DIR", help="Répertoire de sortie")
    p.add_argument("--concurrency", type=int, default=1, metavar="N", help="Liens traités en parallèle")
    p.add_argument("--download-threads", type=int, default=4, metavar="N", help="Threads ffmpeg par flux")
    p.add_argument("--capture-wait-ms", type=int, default=8000, metavar="MS", help="Attente maximale pour capturer m3u8 (ms)")
    p.add_argument("--ffmpeg-timeout", type=int, default=1800, metavar="S", help="Timeout par téléchargement audio/vidéo (s)")
    p.add_argument("--show-browser", action="store_true", help="Affiche le navigateur (debug)")
    p.add_argument("--keep-temp", action="store_true", help="Conserve les fichiers audio/vidéo séparés")
    p.add_argument("--dry-run", action="store_true", help="Simule sans télécharger")
    p.add_argument(
        "--set-file-date", dest="set_file_date",
        action=argparse.BooleanOptionalAction, default=True,
        help="Date le fichier selon la date de publication de la vidéo (--no-set-file-date pour désactiver)",
    )
    return p.parse_args(argv)


def main(argv: Optional[Iterable[str]] = None) -> int:
    args = parse_args(argv)
    signal.signal(signal.SIGINT, _sigint)

    if args.concurrency < 1:
        raise CliError("--concurrency doit être >= 1")
    if args.download_threads < 1:
        raise CliError("--download-threads doit être >= 1")
    if args.ffmpeg_timeout < 60:
        raise CliError("--ffmpeg-timeout doit être >= 60")

    project_root = Path.cwd()
    storage_state_path = project_root / "playwright-state.json"
    console = Console()

    with console.status("[bold yellow]Authentification via cookies…[/bold yellow]"):
        auth_mode = create_storage_state(
            storage_state_path=storage_state_path,
            cookie_file=args.cookie_file,
            headless=not args.show_browser,
        )

    entries = build_entries(args.url, args.input)

    with console.status("[bold yellow]Scan des pages de cours…[/bold yellow]"):
        with sync_playwright() as p:
            browser = p.chromium.launch(headless=not args.show_browser)
            expanded: list[LinkEntry] = []
            for entry in entries:
                if "/course/view.php" in entry.url or "/course/" in entry.url:
                    try:
                        found = scan_course_for_ubicast_links(browser, storage_state_path, entry.url)
                        if found:
                            expanded.extend(found)
                            continue
                    except Exception:
                        pass
                expanded.append(entry)
            browser.close()
    entries = expanded

    ffmpeg_path = find_ffmpeg(project_root)
    mkvmerge_path = ensure_local_mkvtoolnix(project_root)
    args.output_dir.mkdir(parents=True, exist_ok=True)

    success_count = 0
    errors: list[str] = []

    with DownloadUI(entries, args.concurrency, args.download_threads, auth_mode, console=console) as ui:
        with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as executor:
            futures = [
                executor.submit(
                    process_one_link,
                    i, entry, storage_state_path, args.output_dir,
                    ffmpeg_path, mkvmerge_path,
                    not args.show_browser, args.capture_wait_ms,
                    args.download_threads, args.ffmpeg_timeout,
                    args.keep_temp, args.dry_run, args.set_file_date, ui,
                )
                for i, entry in enumerate(entries)
            ]
            try:
                for fut in concurrent.futures.as_completed(futures):
                    _entry, _output_file, error = fut.result()
                    if error:
                        errors.append(f"{_entry.url}: {error}")
                    else:
                        success_count += 1
            except KeyboardInterrupt:
                _shutdown.set()
                kill_all_procs()
                os._exit(130)

    storage_state_path.unlink(missing_ok=True)

    if errors:
        console.print(f"\n[red bold]✗ {len(errors)} erreur(s):[/red bold]")
        for err in errors:
            console.print(f"  [red]• {err[:120]}[/red]")
        console.print(f"\n[bold]{success_count}/{len(entries)} réussi(s)[/bold]")
        return 1

    console.print(f"\n[green bold]✓ {success_count}/{len(entries)} vidéo(s) téléchargée(s)[/green bold]")
    return 0
