# Moodle Video Bulk Downloader

[![Python](https://img.shields.io/badge/Python-3.11%2B-blue)](https://www.python.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](CONTRIBUTING.md)

French version: [README_FR.md](README_FR.md)

## Disclaimer

1. This tool is for personal archiving only. Do not re-publish course recordings without your professor’s explicit permission.
2. It may or may not work on all Moodle instances. Feel free to fork and open a pull request. *Created for moodle.unine.ch*

## Features

- Download Moodle/UbiCast replay pages from one URL or a text list
- Parse grouped input files with course sections
- Authenticate with cookies only (`cookies.txt`)
- Detect `audio_*.m3u8` and best available video variant automatically (e.g. `1440p > 1080p`)
- Download audio/video with live progress bars
- Mux into MKV with `mkvmerge` (MKVToolNix CLI)
- Parallel processing support

## Requirements

- Python 3.11+

Install dependencies:

```powershell
pip install -r requirements.txt
python -m playwright install chromium
```

## Authentication

Cookies are the only supported authentication method.

### How to get cookies from Moodle

1. Log in to Moodle in your browser.
2. Open Developer Tools (`F12`) and go to Application/Storage -> Cookies.
3. Select the Moodle domain (for example `https://moodle.unine.ch`).
4. Copy at least:
	- `MoodleSession`
	- `_shibsession_...` (or equivalent SSO session cookie)
5. Paste them into `cookies.txt`, one per line in `NAME=VALUE` format.

Example:

```text
MoodleSession=...
_shibsession_...=...
```

## Input file format

Example `test.txt`:

```text
Course A
https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx
https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx
Course B
https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx
```

## Usage

Run from input file:

```powershell
python main.py --input test.txt --cookie-file cookies.txt
```

Run from direct URL(s):

```powershell
python main.py --url "https://moodle.unine.ch/mod/ubicast/view.php?id=xxxxxx"
```

## Useful options

| Option | Default | Description |
| --- | --- | --- |
| `--concurrency` | `1` | Number of links processed in parallel |
| `--download-threads` | `1` | ffmpeg threads per stream |
| `--capture-wait-ms` | `8000` | Wait after Play click to capture stream URLs |
| `--ffmpeg-timeout` | `1800` | Timeout (seconds) per audio/video download |
| `--output-dir` | `dl` | Output directory |
| `--show-browser` | `false` | Show browser for auth debugging |
| `--keep-temp` | `false` | Keep separate audio/video files |

## Build EXE (PyInstaller)

Build command:

```powershell
compile.cmd
```

Output:

- `dist/moodle-video-bulk-downloader.exe`

Important:

- Keep `tools/ffmpeg.exe` and `tools/mkvmerge.exe` available with your project/distribution.
- `icon.ico` is used during EXE compilation.

## Output

- `dl/<Course Name>/<Recording Title>.mkv`

Required binaries are stored directly in `tools/` (`mkvmerge.exe`, `ffmpeg.exe`).

## Contributing

Please read [CONTRIBUTING.md](CONTRIBUTING.md) for setup, style, and pull request guidance.

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE).
