# Contributing

Thanks for your interest in improving this project.

## Development setup

1. Install Python 3.11+
2. Install dependencies:

```powershell
pip install -r requirements.txt
python -m playwright install chromium
```

## Coding guidelines

- Keep changes focused and minimal
- Prefer small, testable functions
- Preserve existing CLI behavior unless intentionally changed
- Use clear names and readable logs

## Pull requests

- Open a PR with a clear description of what changed and why
- Include reproduction steps for bug fixes
- If possible, include a short test command

## Issues

When opening an issue, please include:

- Command used
- Full error output
- Python version
- Whether cookies or login mode was used
