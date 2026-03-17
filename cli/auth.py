from pathlib import Path
from typing import Optional

from playwright.sync_api import Page, sync_playwright

from .constants import SHIBBOLETH_LOGIN_URL
from .input_parsing import read_cookie_file
from .models import CliError, Credentials


def login_shibboleth(page: Page, credentials: Credentials) -> None:
    page.goto(SHIBBOLETH_LOGIN_URL, wait_until="domcontentloaded", timeout=120000)
    page.wait_for_timeout(1200)

    if page.locator("input#username, input[name='j_username'], input[type='email']").count() == 0:
        page.goto("https://moodle.unine.ch/login/index.php", wait_until="domcontentloaded", timeout=120000)
        page.wait_for_timeout(1200)
        for selector in [
            "a[href*='Shibboleth.sso/Login']",
            "a:has-text('Switch edu-ID')",
            "a:has-text('edu-ID')",
        ]:
            if page.locator(selector).count() > 0:
                page.locator(selector).first.click()
                page.wait_for_timeout(1800)
                break

    email_filled = False
    for selector in ["input#username", "input[name='j_username']", "input[type='email']", "input[autocomplete='username']"]:
        if page.locator(selector).count() > 0:
            page.fill(selector, credentials.username)
            email_filled = True
            break
    if not email_filled:
        raise CliError("Could not find SSO email field.")

    for selector in ["button:has-text('Continue')", "button:has-text('Continuer')", "button[type='submit']", "input[type='submit']"]:
        if page.locator(selector).count() > 0:
            page.locator(selector).first.click()
            break

    page.wait_for_timeout(1500)

    for selector in [
        "button:has-text('Use password')",
        "a:has-text('Use password')",
        "text=Use password",
        "button:has-text('Utiliser un mot de passe')",
        "a:has-text('Utiliser un mot de passe')",
        "text=Utiliser un mot de passe",
    ]:
        if page.locator(selector).count() > 0:
            try:
                page.locator(selector).first.click(timeout=3000)
            except Exception:
                pass
            page.wait_for_timeout(1200)
            break

    for _ in range(8):
        if page.locator("input[type='password'], input#password, input[name='password']").count() > 0:
            break
        page.wait_for_timeout(500)

    password_filled = False
    for selector in ["input[type='password']", "input#password", "input[name='password']"]:
        if page.locator(selector).count() > 0:
            page.fill(selector, credentials.password)
            password_filled = True
            break
    if not password_filled:
        raise CliError("Could not find SSO password field. MFA/manual step may be required.")

    for selector in ["button:has-text('Login')", "button:has-text('Se connecter')", "button:has-text('Sign in')", "button[type='submit']", "input[type='submit']"]:
        if page.locator(selector).count() > 0:
            page.locator(selector).first.click()
            break

    page.wait_for_load_state("networkidle", timeout=120000)


def create_storage_state(
    storage_state_path: Path,
    cookie_file: Optional[Path],
    credentials: Optional[Credentials],
    headless: bool,
) -> str:
    with sync_playwright() as p:
        browser = p.chromium.launch(headless=headless)
        context = browser.new_context()
        page = context.new_page()

        if cookie_file and cookie_file.exists():
            context.add_cookies(read_cookie_file(cookie_file))
            auth_mode = "cookies"
        elif credentials:
            login_shibboleth(page, credentials)
            auth_mode = "login"
        else:
            browser.close()
            raise CliError("No auth method available. Provide --cookie-file or --login-file.")

        page.goto("https://moodle.unine.ch/", wait_until="networkidle", timeout=120000)
        if "login/index.php" in page.url:
            browser.close()
            raise CliError("Authentication failed (invalid/expired cookies or login flow not completed).")

        context.storage_state(path=str(storage_state_path))
        browser.close()
        return auth_mode
