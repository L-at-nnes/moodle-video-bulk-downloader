import concurrent.futures
import shutil
import subprocess
import tempfile
import threading
import time
from pathlib import Path
from typing import Callable, Optional

import requests

from .models import CliError, StreamInfo
from .utils import run_cmd, unique_output_path


# ── process registry for clean Ctrl+C ────────────────────────────────────────

_proc_lock = threading.Lock()
_active_procs: list[subprocess.Popen] = []


def _reg(p: subprocess.Popen) -> None:
    with _proc_lock:
        _active_procs.append(p)


def _unreg(p: subprocess.Popen) -> None:
    with _proc_lock:
        try:
            _active_procs.remove(p)
        except ValueError:
            pass


def kill_all_procs() -> None:
    with _proc_lock:
        for p in list(_active_procs):
            try:
                p.kill()
            except Exception:
                pass


# ── HLS helpers ───────────────────────────────────────────────────────────────

def _hls_duration(url: str) -> Optional[float]:
    try:
        r = requests.get(url, timeout=30)
        r.raise_for_status()
        total = sum(
            float(line.split(":", 1)[1].split(",", 1)[0])
            for line in r.text.splitlines()
            if line.startswith("#EXTINF:")
        )
        return total or None
    except Exception:
        return None


def _ffmpeg_dl(
    ffmpeg_path: str,
    url: str,
    output_file: Path,
    threads: int,
    timeout_s: int,
    on_progress: Optional[Callable[[float], None]],
    shutdown: Optional[threading.Event],
) -> None:
    duration = _hls_duration(url)

    cmd = [
        ffmpeg_path, "-y", "-nostdin",
        "-loglevel", "error",
        "-threads", str(threads),
        "-i", url,
        "-c", "copy",
        "-progress", "pipe:1", "-nostats",
        str(output_file),
    ]
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
    _reg(p)
    start = time.monotonic()
    last_s = 0.0

    try:
        while True:
            if shutdown and shutdown.is_set():
                p.kill()
                return
            if time.monotonic() - start > timeout_s:
                p.kill()
                raise CliError(f"Timeout ({timeout_s}s) pour {output_file.name}")

            line = p.stdout.readline() if p.stdout else ""
            if line:
                if line.startswith("out_time_ms=") and on_progress and duration:
                    try:
                        sec = int(line.split("=", 1)[1]) / 1_000_000.0
                        if sec > last_s:
                            on_progress(min(99.0, sec / duration * 100))
                            last_s = sec
                    except Exception:
                        pass
                continue

            if p.poll() is not None:
                break
            time.sleep(0.05)

        if p.returncode not in (0, None):
            stderr = (p.stderr.read() or "").strip()
            raise CliError(f"ffmpeg a échoué ({output_file.name}): {stderr[:200]}")

        if on_progress:
            on_progress(100.0)

    finally:
        _unreg(p)


# ── public API ────────────────────────────────────────────────────────────────

def download_and_mux(
    stream_info: StreamInfo,
    output_dir: Path,
    ffmpeg_path: str,
    mkvmerge_path: Path,
    download_threads: int,
    ffmpeg_timeout_s: int,
    keep_temp: bool,
    on_audio_progress: Optional[Callable[[float], None]] = None,
    on_video_progress: Optional[Callable[[float], None]] = None,
    on_mux_start: Optional[Callable[[], None]] = None,
    shutdown: Optional[threading.Event] = None,
) -> Path:
    output_dir.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="moodle-dl-") as tmp:
        tmp_dir = Path(tmp)
        audio_file = tmp_dir / "audio.m4a"
        video_file = tmp_dir / "video.mp4"

        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            af = pool.submit(
                _ffmpeg_dl, ffmpeg_path, stream_info.audio_m3u8, audio_file,
                download_threads, ffmpeg_timeout_s, on_audio_progress, shutdown,
            )
            vf = pool.submit(
                _ffmpeg_dl, ffmpeg_path, stream_info.video_m3u8, video_file,
                download_threads, ffmpeg_timeout_s, on_video_progress, shutdown,
            )
            for fut in (af, vf):
                fut.result()

        if shutdown and shutdown.is_set():
            raise CliError("Interrompu")

        if on_mux_start:
            on_mux_start()

        final_output = unique_output_path(output_dir, stream_info.title)
        run_cmd(
            [str(mkvmerge_path), "-o", str(final_output), str(video_file), str(audio_file)],
            timeout_s=max(300, ffmpeg_timeout_s),
        )

        if keep_temp:
            shutil.copy2(audio_file, final_output.with_suffix(".audio.m4a"))
            shutil.copy2(video_file, final_output.with_suffix(".video.mp4"))

        return final_output
