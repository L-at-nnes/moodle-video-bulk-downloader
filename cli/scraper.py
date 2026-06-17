from typing import List
from pathlib import Path

from playwright.sync_api import Browser

from .models import LinkEntry


def scan_course_for_ubicast_links(browser: Browser, storage_state_path: Path, course_url: str) -> List[LinkEntry]:
    context = browser.new_context(storage_state=str(storage_state_path))
    page = context.new_page()
    page.goto(course_url, wait_until="networkidle", timeout=120000)
    page.wait_for_timeout(2000)

    try:
        title = (page.locator("h1").first.inner_text() or "").strip()
    except Exception:
        title = "Cours"

    # scroll to trigger lazy loading
    page.evaluate("() => window.scrollTo(0, document.body.scrollHeight)")
    page.wait_for_timeout(2000)
    page.evaluate("() => window.scrollTo(0, 0)")
    page.wait_for_timeout(1000)

    # collect anchors from the course page
    try:
        hrefs = page.eval_on_selector_all("a[href]", "els => els.map(e => e.href)") or []
    except Exception:
        hrefs = []

    # also search for links in data attributes, hrefs in activity containers, etc.
    try:
        data_hrefs = page.eval_on_selector_all("[data-href], [href]", "els => els.map(e => e.getAttribute('data-href') || e.getAttribute('href')).filter(h => h)") or []
        hrefs.extend(data_hrefs)
    except Exception:
        pass

    # filter for known UbiCast/media activity types
    ubicast_patterns = ["/mod/ubicast/", "/mod/mediaserver/", "/mod/panopto/", "/mod/mediasource/"]
    candidates = []
    for h in hrefs:
        if not h:
            continue
        if "moodle.unine.ch" not in h:
            continue
        # only pick links that are known media activity types
        if any(pattern in h for pattern in ubicast_patterns):
            candidates.append(h)

    unique = list(dict.fromkeys(candidates))
    results: List[LinkEntry] = [LinkEntry(url=href, course=title or "Cours") for href in unique]

    context.close()
    return results

