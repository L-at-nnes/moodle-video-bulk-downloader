from playwright.sync_api import Browser, Page
import requests

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
        candidate_url = best_captured.replace(f"{prefix}{current_height}{suffix}", f"{prefix}{height}{suffix}")
        if candidate_url == best_captured:
            discovered.setdefault(height, candidate_url)
            continue
        if height in discovered:
            continue
        if is_accessible_m3u8(candidate_url):
            discovered[height] = candidate_url

    return discovered[max(discovered)] if discovered else best_captured


def extract_streams_from_page(browser: Browser, storage_state_path: str, url: str, wait_ms: int) -> StreamInfo:
    context = browser.new_context(storage_state=storage_state_path)
    page = context.new_page()

    captured: list[str] = []

    def on_request(request) -> None:
        request_url = request.url
        if ".m3u8" in request_url and ("audio_" in request_url or "video_" in request_url or "media_" in request_url):
            captured.append(request_url)

    context.on("request", on_request)

    page.goto(url, wait_until="domcontentloaded", timeout=120000)
    page.wait_for_timeout(1500)

    if "login/index.php" in page.url:
        context.close()
        raise CliError("Session is not authenticated (redirected to login).")

    try_play_video(page)
    page.wait_for_timeout(wait_ms)

    unique = list(dict.fromkeys(captured))
    audio = next((u for u in unique if "audio_" in u), "")
    video = resolve_best_video_stream([u for u in unique if "video_" in u or "media_" in u])

    raw_title = extract_activity_title(page)
    context.close()

    if not audio or not video:
        raise CliError("Could not find audio/video m3u8 URLs on this page.")

    title = sanitize_name(
        raw_title.replace("| Moodle UniNE", "").replace("►", "").replace("◄", "").strip(),
        "enregistrement",
    )
    return StreamInfo(title=title, audio_m3u8=audio, video_m3u8=video)
