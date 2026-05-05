import argparse
import concurrent.futures
import threading
from pathlib import Path
from typing import Iterable, Optional

from playwright.sync_api import sync_playwright
from tqdm import tqdm

from .auth import create_storage_state
from .constants import DEFAULT_OUTPUT_DIR
from .downloader import download_and_mux
from .input_parsing import build_entries
from .models import CliError, LinkEntry
from .streams import extract_streams_from_page, parse_video_height_from_url
from .system_tools import ensure_local_mkvtoolnix, find_ffmpeg
from .utils import sanitize_name, unique_output_path


def process_one_link(
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
) -> tuple[LinkEntry, Optional[Path], Optional[str]]:
    try:
        tqdm.write(f"[extract] {entry.url}")
        with sync_playwright() as p:
            browser = p.chromium.launch(headless=headless)
            stream_info = extract_streams_from_page(
                browser=browser,
                storage_state_path=str(storage_state_path),
                url=entry.url,
                wait_ms=capture_wait_ms,
            )
            browser.close()

        video_height = parse_video_height_from_url(stream_info.video_m3u8)
        if video_height > 0:
            tqdm.write(f"[download] {stream_info.title} ({video_height}p)")
        else:
            tqdm.write(f"[download] {stream_info.title}")

        target_dir = output_root / sanitize_name(entry.course, "cours")

        if dry_run:
            final_output = unique_output_path(target_dir, stream_info.title)
            tqdm.write(f"[dry-run] Would download: {stream_info.title} -> {final_output}")
            tqdm.write(f"  audio: {stream_info.audio_m3u8}")
            tqdm.write(f"  video: {stream_info.video_m3u8}")
            return entry, final_output, None

        output_file = download_and_mux(
            stream_info=stream_info,
            output_dir=target_dir,
            ffmpeg_path=ffmpeg_path,
            mkvmerge_path=mkvmerge_path,
            download_threads=download_threads,
            ffmpeg_timeout_s=ffmpeg_timeout_s,
            keep_temp=keep_temp,
        )

        return entry, output_file, None
    except Exception as exc:
        return entry, None, str(exc)


def parse_args(argv: Optional[Iterable[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Download Moodle UbiCast replays and mux to MKV.")
    parser.add_argument("--url", action="append", default=[], help="Single Moodle replay URL (repeatable)")
    parser.add_argument("--input", type=Path, help="Input text file with course headings and URLs")
    parser.add_argument("--cookie-file", type=Path, default=Path("cookies.txt"), help="Cookie file")
    parser.add_argument("--output-dir", type=Path, default=Path(DEFAULT_OUTPUT_DIR), help="Output directory")
    parser.add_argument("--concurrency", type=int, default=1, help="Number of links processed in parallel")
    parser.add_argument("--download-threads", type=int, default=1, help="ffmpeg threads per stream")
    parser.add_argument("--capture-wait-ms", type=int, default=8000, help="Wait time after play click to capture m3u8")
    parser.add_argument("--ffmpeg-timeout", type=int, default=1800, help="Timeout in seconds per audio/video download")
    parser.add_argument("--show-browser", action="store_true", help="Show browser (debug authentication)")
    parser.add_argument("--keep-temp", action="store_true", help="Keep separate audio/video files")
    parser.add_argument("--dry-run", action="store_true", help="Show what would be downloaded without downloading")
    return parser.parse_args(argv)


def main(argv: Optional[Iterable[str]] = None) -> int:
    args = parse_args(argv)

    if args.concurrency < 1:
        raise CliError("--concurrency must be >= 1")
    if args.download_threads < 1:
        raise CliError("--download-threads must be >= 1")
    if args.ffmpeg_timeout < 60:
        raise CliError("--ffmpeg-timeout must be >= 60")

    project_root = Path.cwd()

    storage_state_path = project_root / "playwright-state.json"
    auth_mode = create_storage_state(
        storage_state_path=storage_state_path,
        cookie_file=args.cookie_file,
        headless=not args.show_browser,
    )

    entries = build_entries(args.url, args.input)

    # expand course pages into individual Ubicast activity links
    from .scraper import scan_course_for_ubicast_links
    from playwright.sync_api import sync_playwright

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
    print(
        f"{len(entries)} link(s) | auth={auth_mode} | concurrency={args.concurrency} | "
        f"ffmpeg_threads={args.download_threads}"
    )

    success_count = 0
    errors: list[str] = []
    lock = threading.Lock()

    with concurrent.futures.ThreadPoolExecutor(max_workers=args.concurrency) as executor:
        futures = [
            executor.submit(
                process_one_link,
                entry,
                storage_state_path,
                args.output_dir,
                ffmpeg_path,
                mkvmerge_path,
                not args.show_browser,
                args.capture_wait_ms,
                args.download_threads,
                args.ffmpeg_timeout,
                args.keep_temp,
                args.dry_run,
            )
            for entry in entries
        ]

        with tqdm(total=len(futures), desc="items", unit="video", dynamic_ncols=True) as main_bar:
            for future in concurrent.futures.as_completed(futures):
                entry, output_file, error = future.result()
                with lock:
                    if error:
                        errors.append(f"{entry.url} -> {error}")
                        tqdm.write(f"ERROR: {entry.url}")
                    else:
                        success_count += 1
                        tqdm.write(f"OK: {output_file}")
                    main_bar.update(1)

    storage_state_path.unlink(missing_ok=True)

    print(f"Done: {success_count}/{len(entries)} success")
    if errors:
        print("Errors:")
        for err in errors:
            print(f"- {err}")
        return 1

    return 0
