from pathlib import Path

from playwright.sync_api import sync_playwright

from .input_parsing import read_cookie_file
from .models import CliError


def create_storage_state(
    storage_state_path: Path,
    cookie_file: Path,
    headless: bool,
) -> str:
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=headless)
        context = browser.new_context()
        page = context.new_page()

        if not cookie_file.exists():
            browser.close()
            raise CliError("Cookie file not found. Provide a valid --cookie-file.")

        context.add_cookies(read_cookie_file(cookie_file))
        auth_mode = "cookies"

        page.goto("https://moodle.unine.ch/", wait_until="networkidle", timeout=120000)
        if "login/index.php" in page.url:
            browser.close()
            raise CliError("Authentication failed (invalid/expired cookies or login flow not completed).")

        context.storage_state(path=str(storage_state_path))
        browser.close()
        return auth_mode
