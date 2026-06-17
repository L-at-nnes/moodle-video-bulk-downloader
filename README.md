# Moodle Video Bulk Downloader

[![Python](https://img.shields.io/badge/Python-3.11%2B-blue)](https://www.python.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

French version: [README_FR.md](README_FR.md)

## Disclaimer

1. This tool is for personal archiving only. Do not re-publish course recordings without your professor's explicit permission.
2. It may or may not work on all Moodle instances. Feel free to fork and open a pull request. *Created for moodle.unine.ch*

## Features

- Download Moodle/UbiCast replay pages from one URL or a text list
- Parse grouped input files with course sections
- Authenticate with cookies only (`cookies.txt` — default, no flag needed)
- Detect `audio_*.m3u8` and best available video variant automatically (e.g. `1440p > 1080p`)
- Auto-scan course pages: if you pass a course URL the tool will discover and queue all detected UbiCast activities
- **Parallel audio + video download** (both streams downloaded simultaneously)
- **Smart stream capture**: stops waiting as soon as both URLs are found instead of always waiting the full timeout
- **Adaptive live UI**: per-video status table auto-scrolls to active downloads and fits any terminal size; scroll indicator shows hidden rows (↑N / ↓N)
- **Automatic retry with exponential backoff**: 3 attempts with 5 s then 10 s delays; retry status shown in table
- **File date stamping**: sets the file modification date to the video's publication date (scraped from the page); enabled by default, disable with `--no-set-file-date`
- Mux into MKV with `mkvmerge` (MKVToolNix CLI)
- Parallel processing support (`--concurrency`)
- Ctrl+C kills everything immediately (all ffmpeg processes + exit)

## Requirements

- Python 3.11+

Install dependencies:

```powershell
pip install -r requirements.txt
python -m playwright install chromium
```

## Authentication

Cookies are the only supported authentication method. Place your cookies in `cookies.txt` (default — no `--cookie-file` flag needed).

### How to get cookies from Moodle

1. Log in to Moodle in your browser.
2. Open Developer Tools (`F12`) and go to Application/Storage -> Cookies.
3. Select the Moodle domain (for example `https://moodle.unine.ch`).
4. Copy at least `MoodleSession` and the SSO session cookie (for example `_shibsession_...`).
5. Paste them into `cookies.txt`. Both formats are supported:

One per line:

```text
MoodleSession=...
_shibsession_...=...
```

Or single-line (copied from the browser):

```text
MoodleSession=...; _shibsession_...=...;
```

## Input file format

Example `liens.txt`:

```text
Course A
https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx
https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx
Course B
https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx
```

## Usage

Run from input file (cookies loaded from `cookies.txt` automatically):

```powershell
python main.py --input liens.txt
```

Run from direct URL(s):

```powershell
python main.py --url "https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx"
```

Run with parallel downloads (2 videos at once):

```powershell
python main.py --input liens.txt --concurrency 2
```

## Useful options

| Option | Default | Description |
| --- | --- | --- |
| `--cookie-file` | `cookies.txt` | Cookie file (KEY=VALUE or Netscape format) |
| `--concurrency` | `1` | Number of links processed in parallel |
| `--download-threads` | `4` | ffmpeg threads per stream |
| `--capture-wait-ms` | `8000` | Max wait to capture stream URLs (exits early once found) |
| `--ffmpeg-timeout` | `1800` | Timeout (seconds) per audio/video download |
| `--output-dir` | `dl` | Output directory |
| `--set-file-date` / `--no-set-file-date` | enabled | Set file modification date to the video's publication date |
| `--show-browser` | `false` | Show browser for auth debugging |
| `--keep-temp` | `false` | Keep separate audio/video files |
| `--dry-run` | `false` | Show what would be downloaded without performing downloads |

## Build EXE (PyInstaller)

Build command:

```powershell
compile.cmd
```

Output:

- `dist/moodle-video-bulk-downloader/moodle-video-bulk-downloader.exe` (with bundled `tools/ffmpeg.exe` and `tools/mkvmerge.exe`)

The EXE folder includes all dependencies and tools — no additional setup needed.

## Output

- `dl/<Course Name>/<Recording Title>.mkv`

Required binaries are stored directly in `tools/` (`mkvmerge.exe`, `ffmpeg.exe`).

## Contributing

Please read [CONTRIBUTING.md](CONTRIBUTING.md) for setup, style, and pull request guidance.

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE).
