import os
import shutil
from pathlib import Path
from typing import Optional

from .models import CliError
from .utils import run_cmd


def find_installed_mkvmerge() -> Optional[Path]:
    from_env = shutil.which("mkvmerge")
    if from_env:
        return Path(from_env)

    candidates = [
        Path(r"C:\Program Files\MKVToolNix\mkvmerge.exe"),
        Path(r"C:\Program Files (x86)\MKVToolNix\mkvmerge.exe"),
    ]

    local_app = os.getenv("LOCALAPPDATA")
    if local_app:
        local = Path(local_app)
        candidates.extend(local.glob("Programs/MKVToolNix/mkvmerge.exe"))
        candidates.extend(local.glob("Microsoft/WinGet/Packages/*MKVToolNix*/mkvmerge.exe"))

    for candidate in candidates:
        if candidate.exists():
            return candidate

    return None


def ensure_local_mkvtoolnix(project_root: Path) -> Path:
    tools_dir = project_root / "tools"
    local_mkvmerge = tools_dir / "mkvmerge.exe"

    if local_mkvmerge.exists():
        return local_mkvmerge

    installed = find_installed_mkvmerge()
    if not installed:
        winget = shutil.which("winget")
        if not winget:
            raise CliError("MKVToolNix is missing and winget is not available.")

        run_cmd(
            [
                winget,
                "install",
                "--id",
                "MoritzBunkus.MKVToolNix",
                "--source",
                "winget",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--silent",
            ]
        )
        installed = find_installed_mkvmerge()

    if not installed:
        raise CliError("Could not find MKVToolNix after installation.")

    tools_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(installed, local_mkvmerge)

    if not local_mkvmerge.exists():
        raise CliError("Failed to copy mkvmerge.exe into tools/")

    return local_mkvmerge


def find_ffmpeg(project_root: Path) -> str:
    local_ffmpeg = project_root / "tools" / "ffmpeg.exe"
    if local_ffmpeg.exists():
        return str(local_ffmpeg)

    ffmpeg = shutil.which("ffmpeg")
    if ffmpeg:
        tools_dir = project_root / "tools"
        tools_dir.mkdir(parents=True, exist_ok=True)
        target = tools_dir / "ffmpeg.exe"
        if not target.exists():
            try:
                shutil.copy2(ffmpeg, target)
                return str(target)
            except Exception:
                return ffmpeg
        return ffmpeg

    local_app = Path(os.getenv("LOCALAPPDATA", ""))
    candidates = sorted(local_app.glob("ms-playwright/ffmpeg-*/ffmpeg-win64/ffmpeg.exe"), reverse=True)
    if candidates:
        tools_dir = project_root / "tools"
        tools_dir.mkdir(parents=True, exist_ok=True)
        target = tools_dir / "ffmpeg.exe"
        try:
            shutil.copy2(candidates[0], target)
            return str(target)
        except Exception:
            return str(candidates[0])

    raise CliError("ffmpeg not found. Install ffmpeg or run: python -m playwright install chromium")
