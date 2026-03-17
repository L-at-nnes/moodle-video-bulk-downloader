import shutil
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Optional

import requests
from tqdm import tqdm

from .models import CliError, StreamInfo
from .utils import run_cmd, unique_output_path


def estimate_hls_duration_seconds(m3u8_url: str) -> Optional[float]:
    try:
        response = requests.get(m3u8_url, timeout=30)
        response.raise_for_status()
        total = 0.0
        for line in response.text.splitlines():
            if line.startswith("#EXTINF:"):
                value = line.split(":", 1)[1].split(",", 1)[0].strip()
                total += float(value)
        return total if total > 0 else None
    except Exception:
        return None


def ffmpeg_download_with_progress(
    ffmpeg_path: str,
    source_url: str,
    output_file: Path,
    threads: int,
    timeout_s: int,
    label: str,
) -> None:
    duration = estimate_hls_duration_seconds(source_url)
    total = int(duration) if duration else 0

    command = [
        ffmpeg_path,
        "-y",
        "-nostdin",
        "-loglevel",
        "error",
        "-threads",
        str(threads),
        "-i",
        source_url,
        "-c",
        "copy",
        "-progress",
        "pipe:1",
        "-nostats",
        str(output_file),
    ]

    process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
    start_time = time.time()
    last_media_seconds = 0.0

    bar_total = total if total > 0 else 100
    unit = "s" if total > 0 else "%"
    with tqdm(total=bar_total, desc=label, unit=unit, leave=False, dynamic_ncols=True) as bar:
        while True:
            if time.time() - start_time > timeout_s:
                process.kill()
                raise CliError(f"Timeout after {timeout_s}s while downloading {label}")

            line = process.stdout.readline() if process.stdout else ""
            if line:
                stripped = line.strip()
                if stripped.startswith("out_time_ms="):
                    try:
                        out_time_ms = int(stripped.split("=", 1)[1])
                        media_seconds = out_time_ms / 1_000_000.0
                        if total > 0:
                            delta = int(media_seconds) - int(last_media_seconds)
                            if delta > 0:
                                bar.update(delta)
                                last_media_seconds = media_seconds
                        else:
                            now_pct = min(99, int(media_seconds) % 100)
                            bar.n = now_pct
                            bar.refresh()
                    except Exception:
                        pass
                continue

            if process.poll() is not None:
                break

            time.sleep(0.1)

        if process.returncode != 0:
            stderr = process.stderr.read().strip() if process.stderr else ""
            raise CliError(f"ffmpeg failed for {label}. stderr: {stderr}")

        if total > 0 and bar.n < bar.total:
            bar.update(bar.total - bar.n)
        elif total == 0:
            bar.n = 100
            bar.refresh()


def download_and_mux(
    stream_info: StreamInfo,
    output_dir: Path,
    ffmpeg_path: str,
    mkvmerge_path: Path,
    download_threads: int,
    ffmpeg_timeout_s: int,
    keep_temp: bool,
) -> Path:
    output_dir.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="moodle-dl-") as tmp:
        tmp_dir = Path(tmp)
        audio_file = tmp_dir / "audio.m4a"
        video_file = tmp_dir / "video.mp4"

        ffmpeg_download_with_progress(ffmpeg_path, stream_info.audio_m3u8, audio_file, download_threads, ffmpeg_timeout_s, "audio")
        ffmpeg_download_with_progress(ffmpeg_path, stream_info.video_m3u8, video_file, download_threads, ffmpeg_timeout_s, "video")

        final_output = unique_output_path(output_dir, stream_info.title)
        run_cmd([str(mkvmerge_path), "-o", str(final_output), str(video_file), str(audio_file)], timeout_s=max(300, ffmpeg_timeout_s))

        if keep_temp:
            shutil.copy2(audio_file, final_output.with_suffix(".audio.m4a"))
            shutil.copy2(video_file, final_output.with_suffix(".video.mp4"))

        return final_output
