import threading
from datetime import datetime
from typing import Optional

import requests
from playwright.sync_api import Browser, Page

from .constants import COMMON_VIDEO_HEIGHTS, VIDEO_VARIANT_RE
from .models import CliError, StreamInfo
from .utils import sanitize_name


def try_play_video(page: Page) -> None:
    selectors = [
        ".vjs-big-play-button",
        "button[aria-label*='Play']",
        "button[title*='Play']",
        "button:has-text('Play')",
        "text=Play",
        "video",
    ]
    for frame in page.frames:
        for selector in selectors:
            locator = frame.locator(selector)
            if locator.count() == 0:
                continue
            try:
                locator.first.click(timeout=2500)
                break
            except Exception:
                try:
                    locator.first.click(timeout=2500, force=True)
                    break
                except Exception:
                    continue


def extract_activity_title(page: Page) -> str:
    for selector in ["#region-main h2", "section#region-main h2", "div[role='main'] h2"]:
        locator = page.locator(selector)
        if locator.count() > 0:
            value = (locator.first.inner_text() or "").strip()
            if value:
                return value

    info = page.locator("div[data-region='activity-information']")
    if info.count() > 0:
        data_name = (info.first.get_attribute("data-activityname") or "").strip()
        if data_name:
            return data_name

    for selector in ["#prev-activity-link", "#next-activity-link"]:
        locator = page.locator(selector)
        if locator.count() > 0:
            text = (locator.first.inner_text() or "").strip()
            if text:
                return text

    if page.locator("h1").count() > 0:
        h1_text = (page.locator("h1").first.inner_text() or "").strip()
        if h1_text:
            return h1_text

    return (page.title() or "").strip()


def parse_video_height_from_url(url: str) -> int:
    match = VIDEO_VARIANT_RE.search(url)
    if not match:
        return 0
    try:
        return int(match.group(2))
    except Exception:
        return 0


def is_accessible_m3u8(url: str) -> bool:
    try:
        response = requests.get(url, timeout=10)
        return response.status_code == 200 and "#EXTM3U" in response.text
    except Exception:
        return False


def resolve_best_video_stream(video_candidates: list[str]) -> str:
    if not video_candidates:
        return ""

    unique_candidates = list(dict.fromkeys(video_candidates))
    best_captured = max(unique_candidates, key=parse_video_height_from_url)

    match = VIDEO_VARIANT_RE.search(best_captured)
    if not match:
        return best_captured

    prefix, current_height_str, suffix = match.group(1), match.group(2), match.group(3)
    current_height = int(current_height_str)
    discovered: dict[int, str] = {}

    for candidate in unique_candidates:
        height = parse_video_height_from_url(candidate)
        if height > 0:
            discovered[height] = candidate

    for height in COMMON_VIDEO_HEIGHTS:
        candidate_url = best_captured.replace(
            f"{prefix}{current_height}{suffix}", f"{prefix}{height}{suffix}"
        )
        if candidate_url == best_captured:
            discovered.setdefault(height, candidate_url)
            continue
        if height in discovered:
            continue
        if is_accessible_m3u8(candidate_url):
            discovered[height] = candidate_url

    return discovered[max(discovered)] if discovered else best_captured


def _parse_date(value: str) -> Optional[datetime]:
    s = value.strip()
    # Replace Z suffix and try fromisoformat (handles most ISO 8601 variants in Python 3.11+)
    try:
        return datetime.fromisoformat(s.replace("Z", "+00:00"))
    except Exception:
        pass
    # Fallback: try common non-ISO formats
    for fmt in ("%Y-%m-%d %H:%M:%S", "%Y-%m-%d"):
        try:
            return datetime.strptime(s[:len(fmt)], fmt)
        except Exception:
            pass
    return None


def extract_activity_date(page: Page) -> Optional[datetime]:
    # Strategy 1: JSON-LD structured data
    try:
        result = page.evaluate("""() => {
            const scripts = document.querySelectorAll('script[type="application/ld+json"]');
            for (const s of scripts) {
                try {
                    const d = JSON.parse(s.textContent);
                    for (const k of ['datePublished', 'dateCreated', 'uploadDate', 'date']) {
                        if (d[k]) return d[k];
                    }
                } catch {}
            }
            return null;
        }""")
        if result:
            d = _parse_date(str(result))
            if d:
                return d
    except Exception:
        pass

    # Strategy 2: <time datetime="..."> elements
    try:
        result = page.evaluate("""() => {
            for (const t of document.querySelectorAll('time[datetime]')) {
                const dt = t.getAttribute('datetime');
                if (dt && dt.length >= 8) return dt;
            }
            return null;
        }""")
        if result:
            d = _parse_date(str(result))
            if d:
                return d
    except Exception:
        pass

    # Strategy 3: meta tags
    try:
        result = page.evaluate("""() => {
            for (const p of ['article:published_time', 'og:published_time', 'DC.date']) {
                const m = document.querySelector(`meta[property="${p}"], meta[name="${p}"]`);
                if (m) return m.getAttribute('content');
            }
            return null;
        }""")
        if result:
            d = _parse_date(str(result))
            if d:
                return d
    except Exception:
        pass

    # Strategy 4: UbiCast/Nudgis window variables
    try:
        result = page.evaluate("""() => {
            if (window.mediaData) {
                return window.mediaData.add_date || window.mediaData.creation_date || null;
            }
            if (window.EO && window.EO.media) {
                return window.EO.media.add_date || null;
            }
            return null;
        }""")
        if result:
            d = _parse_date(str(result))
            if d:
                return d
    except Exception:
        pass

    # Strategy 5: try same selectors inside iframes
    try:
        for frame in page.frames:
            if frame.url == page.url:
                continue
            result = frame.evaluate("""() => {
                if (window.mediaData)
                    return window.mediaData.add_date || window.mediaData.creation_date || null;
                if (window.EO && window.EO.media)
                    return window.EO.media.add_date || null;
                const t = document.querySelector('time[datetime]');
                return t ? t.getAttribute('datetime') : null;
            }""")
            if result:
                d = _parse_date(str(result))
                if d:
                    return d
    except Exception:
        pass

    return None


def extract_streams_from_page(
    browser: Browser,
    storage_state_path: str,
    url: str,
    wait_ms: int,
    shutdown: Optional[threading.Event] = None,
) -> StreamInfo:
    context = browser.new_context(storage_state=storage_state_path)
    page = context.new_page()
    captured: list[str] = []
    captured_date: list = [None]

    def on_request(request) -> None:
        u = request.url
        if ".m3u8" in u and ("audio_" in u or "video_" in u or "media_" in u):
            captured.append(u)

    def on_response(response) -> None:
        if captured_date[0] is not None:
            return
        try:
            rurl = response.url
            if any(p in rurl for p in ["/api/v2/medias", "/api/media", "/mediaserver/api"]):
                data = response.json()
                if isinstance(data, dict):
                    for key in ["add_date", "creation_date", "record_date", "published_at"]:
                        val = data.get(key)
                        if val:
                            d = _parse_date(str(val))
                            if d:
                                captured_date[0] = d
                                return
        except Exception:
            pass

    context.on("request", on_request)
    context.on("response", on_response)

    page.goto(url, wait_until="domcontentloaded", timeout=120000)
    page.wait_for_timeout(1500)

    if "login/index.php" in page.url:
        context.close()
        raise CliError("Non authentifié (redirigé vers login).")

    try_play_video(page)

    # Incremental wait: exit early once both audio and video m3u8 URLs are captured
    chunk_ms = 300
    elapsed_ms = 0
    while elapsed_ms < wait_ms:
        if shutdown and shutdown.is_set():
            context.close()
            raise CliError("Interrompu")
        page.wait_for_timeout(chunk_ms)
        elapsed_ms += chunk_ms
        has_audio = any("audio_" in u for u in captured)
        has_video = any("video_" in u or "media_" in u for u in captured)
        if has_audio and has_video:
            break

    unique = list(dict.fromkeys(captured))
    audio = next((u for u in unique if "audio_" in u), "")
    video = resolve_best_video_stream([u for u in unique if "video_" in u or "media_" in u])

    raw_title = extract_activity_title(page)

    pub_date = captured_date[0]
    if pub_date is None:
        pub_date = extract_activity_date(page)

    context.close()

    if not audio or not video:
        raise CliError("Impossible de trouver les URLs m3u8 audio/vidéo.")

    title = sanitize_name(
        raw_title.replace("| Moodle UniNE", "").replace("►", "").replace("◄", "").strip(),
        "enregistrement",
    )
    return StreamInfo(title=title, audio_m3u8=audio, video_m3u8=video, pub_date=pub_date)
