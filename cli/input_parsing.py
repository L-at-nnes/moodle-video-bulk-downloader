from pathlib import Path
from typing import Optional

from .constants import URL_PATTERN
from .models import CliError, Credentials, LinkEntry


def read_credentials(login_file: Path) -> Credentials:
    lines = [line.strip() for line in login_file.read_text(encoding="utf-8").splitlines() if line.strip()]
    if len(lines) < 2:
        raise CliError("Login file must contain email on line 1 and password on line 2.")
    return Credentials(username=lines[0], password=lines[1])


def read_cookie_file(cookie_file: Path) -> list[dict]:
    if not cookie_file.exists():
        raise CliError(f"Cookie file not found: {cookie_file}")

    raw = cookie_file.read_text(encoding="utf-8").strip()
    if not raw:
        raise CliError("Cookie file is empty.")

    parsed: list[tuple[str, str]] = []
    for raw_line in raw.splitlines():
        line = raw_line.strip()
        if not line:
            continue

        for chunk in [part.strip() for part in line.split(";") if part.strip()]:
            if "=" not in chunk:
                continue
            name, value = chunk.split("=", 1)
            name = name.strip()
            value = value.strip()
            if name and value:
                parsed.append((name, value))

    if not parsed:
        raise CliError("No valid cookies found. Expected format: NAME=VALUE")

    return [
        {
            "name": name,
            "value": value,
            "url": "https://moodle.unine.ch",
            "secure": True,
            "httpOnly": False,
        }
        for name, value in parsed
    ]


def parse_input_file(input_file: Path) -> list[LinkEntry]:
    if not input_file.exists():
        raise CliError(f"Input file not found: {input_file}")

    current_course = "Sans catégorie"
    entries: list[LinkEntry] = []

    for raw_line in input_file.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if not line:
            continue

        if URL_PATTERN.match(line):
            entries.append(LinkEntry(url=line, course=current_course))
        else:
            current_course = line

    if not entries:
        raise CliError("No valid URL found in input file.")

    return entries


def build_entries(single_urls: list[str], input_file: Optional[Path]) -> list[LinkEntry]:
    entries: list[LinkEntry] = []

    for url in single_urls:
        if not URL_PATTERN.match(url):
            raise CliError(f"Invalid URL: {url}")
        entries.append(LinkEntry(url=url, course="Téléchargements"))

    if input_file:
        entries.extend(parse_input_file(input_file))

    if not entries:
        raise CliError("Provide at least --url or --input")

    return entries
