import subprocess
from pathlib import Path
from typing import Optional

from .models import CliError


def sanitize_name(value: str, fallback: str) -> str:
    import re

    cleaned = re.sub(r"[\\/:*?\"<>|]", "_", value).strip()
    cleaned = re.sub(r"\s+", " ", cleaned)
    return cleaned[:180] if cleaned else fallback


def run_cmd(command: list[str], timeout_s: Optional[int] = None) -> None:
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=timeout_s)
    except subprocess.TimeoutExpired as exc:
        raise CliError(f"Timeout after {timeout_s}s for command: {' '.join(command)}") from exc

    if result.returncode != 0:
        raise CliError(
            f"Command failed: {' '.join(command)}\\n"
            f"stdout: {(result.stdout or '').strip()}\\n"
            f"stderr: {(result.stderr or '').strip()}"
        )


def unique_output_path(base_dir: Path, filename: str) -> Path:
    candidate = base_dir / f"{filename}.mkv"
    if not candidate.exists():
        return candidate

    index = 2
    while True:
        candidate = base_dir / f"{filename} ({index}).mkv"
        if not candidate.exists():
            return candidate
        index += 1
