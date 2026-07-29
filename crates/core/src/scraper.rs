use crate::error::{MvbdError, Result};
use crate::models::{LinkEntry, StreamInfo};
use chromiumoxide::Page;
use chromiumoxide::cdp::browser_protocol::network::{
    EventRequestWillBeSent, EventResponseReceived, GetResponseBodyParams,
};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use futures::StreamExt;
use regex::Regex;
use std::sync::{Arc, Mutex, OnceLock};
use tokio_util::sync::CancellationToken;

const COMMON_VIDEO_HEIGHTS: [u32; 9] = [4320, 2160, 1440, 1080, 720, 540, 480, 360, 240];
const MEDIA_API_PATTERNS: [&str; 3] = ["/api/v2/medias", "/api/media", "/mediaserver/api"];
const UBICAST_PATTERNS: [&str; 4] = [
    "/mod/ubicast/",
    "/mod/mediaserver/",
    "/mod/panopto/",
    "/mod/mediasource/",
];

fn video_variant_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)((?:media|video)_)(\d+)(_.*\.m3u8)").unwrap())
}

fn parse_date(value: &str) -> Option<DateTime<Utc>> {
    let s = value.trim();
    if let Ok(dt) = DateTime::parse_from_rfc3339(&s.replace('Z', "+00:00")) {
        return Some(dt.with_timezone(&Utc));
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Some(DateTime::from_naive_utc_and_offset(ndt, Utc));
    }
    if let Ok(nd) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(DateTime::from_naive_utc_and_offset(nd.and_hms_opt(0, 0, 0).unwrap(), Utc));
    }
    None
}

/// Best-effort click on a play button. chromiumoxide has no Playwright-style
/// per-frame locator API, and Moodle/UbiCast player iframes are usually
/// same-origin, but reaching into an iframe's document from the top page
/// still requires per-frame execution contexts that chromiumoxide doesn't
/// expose practically — so this only looks at the top-level page.
async fn try_play_video(page: &Page) {
    let selectors = [
        ".vjs-big-play-button",
        "button[aria-label*='Play']",
        "button[title*='Play']",
        "video",
    ];
    for selector in selectors {
        if let Ok(el) = page.find_element(selector).await {
            if el.click().await.is_ok() {
                break;
            }
        }
    }
}

async fn extract_activity_title(page: &Page) -> String {
    for selector in ["#region-main h2", "section#region-main h2", "div[role='main'] h2"] {
        if let Ok(el) = page.find_element(selector).await {
            if let Ok(Some(text)) = el.inner_text().await {
                let text = text.trim();
                if !text.is_empty() {
                    return text.to_string();
                }
            }
        }
    }

    if let Ok(el) = page.find_element("div[data-region='activity-information']").await {
        if let Ok(Some(name)) = el.attribute("data-activityname").await {
            let name = name.trim();
            if !name.is_empty() {
                return name.to_string();
            }
        }
    }

    for selector in ["#prev-activity-link", "#next-activity-link"] {
        if let Ok(el) = page.find_element(selector).await {
            if let Ok(Some(text)) = el.inner_text().await {
                let text = text.trim();
                if !text.is_empty() {
                    return text.to_string();
                }
            }
        }
    }

    if let Ok(el) = page.find_element("h1").await {
        if let Ok(Some(text)) = el.inner_text().await {
            let text = text.trim();
            if !text.is_empty() {
                return text.to_string();
            }
        }
    }

    page.get_title().await.ok().flatten().unwrap_or_default().trim().to_string()
}

/// Five fallback strategies, ported verbatim from `cli/streams.py::extract_activity_date`.
/// The fifth strategy (retrying the same lookups inside iframes) is dropped:
/// see the note on `try_play_video` above about chromiumoxide's lack of a
/// practical per-frame JS evaluation API.
async fn extract_activity_date(page: &Page) -> Option<DateTime<Utc>> {
    let strategies = [
        r#"() => {
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
        }"#,
        r#"() => {
            for (const t of document.querySelectorAll('time[datetime]')) {
                const dt = t.getAttribute('datetime');
                if (dt && dt.length >= 8) return dt;
            }
            return null;
        }"#,
        r#"() => {
            for (const p of ['article:published_time', 'og:published_time', 'DC.date']) {
                const m = document.querySelector(`meta[property="${p}"], meta[name="${p}"]`);
                if (m) return m.getAttribute('content');
            }
            return null;
        }"#,
        r#"() => {
            if (window.mediaData) {
                return window.mediaData.add_date || window.mediaData.creation_date || null;
            }
            if (window.EO && window.EO.media) {
                return window.EO.media.add_date || null;
            }
            return null;
        }"#,
    ];

    for js in strategies {
        if let Ok(result) = page.evaluate(js).await {
            if let Ok(Some(value)) = result.into_value::<Option<String>>() {
                if let Some(date) = parse_date(&value) {
                    return Some(date);
                }
            }
        }
    }

    None
}

fn parse_video_height_from_url(url: &str) -> u32 {
    video_variant_re()
        .captures(url)
        .and_then(|c| c.get(2))
        .and_then(|m| m.as_str().parse().ok())
        .unwrap_or(0)
}

async fn is_accessible_m3u8(url: &str) -> bool {
    match reqwest::get(url).await {
        Ok(resp) if resp.status().is_success() => match resp.text().await {
            Ok(body) => body.contains("#EXTM3U"),
            Err(_) => false,
        },
        _ => false,
    }
}

/// Picks the best captured video variant, then probes the other common
/// resolutions (by substituting the height in the URL) for an even better
/// one. Ports `cli/streams.py::resolve_best_video_stream`.
async fn resolve_best_video_stream(video_candidates: Vec<String>) -> String {
    if video_candidates.is_empty() {
        return String::new();
    }

    let mut unique = Vec::new();
    for c in video_candidates {
        if !unique.contains(&c) {
            unique.push(c);
        }
    }

    let best_captured = unique
        .iter()
        .max_by_key(|u| parse_video_height_from_url(u))
        .cloned()
        .unwrap();

    let Some(caps) = video_variant_re().captures(&best_captured) else {
        return best_captured;
    };
    let prefix = caps.get(1).unwrap().as_str().to_string();
    let current_height: u32 = caps.get(2).unwrap().as_str().parse().unwrap_or(0);
    let suffix = caps.get(3).unwrap().as_str().to_string();

    let mut discovered: std::collections::HashMap<u32, String> = std::collections::HashMap::new();
    for c in &unique {
        let height = parse_video_height_from_url(c);
        if height > 0 {
            discovered.insert(height, c.clone());
        }
    }

    for height in COMMON_VIDEO_HEIGHTS {
        let needle = format!("{prefix}{current_height}{suffix}");
        let replacement = format!("{prefix}{height}{suffix}");
        let candidate_url = best_captured.replace(&needle, &replacement);
        if candidate_url == best_captured {
            discovered.entry(height).or_insert(candidate_url);
            continue;
        }
        if discovered.contains_key(&height) {
            continue;
        }
        if is_accessible_m3u8(&candidate_url).await {
            discovered.insert(height, candidate_url);
        }
    }

    discovered
        .into_iter()
        .max_by_key(|(h, _)| *h)
        .map(|(_, url)| url)
        .unwrap_or(best_captured)
}

/// Watches network traffic on `page` for the audio/video `.m3u8` URLs, clicks
/// the play button to trigger playback if needed, then scrapes the activity
/// title and publication date. Mirrors `cli/streams.py::extract_streams_from_page`.
///
/// Unlike the stub's original signature, this now takes `url` and navigates
/// itself: the network listeners must be registered before navigation to
/// catch the initial burst of requests, so the goto has to happen inside
/// this function rather than by the caller beforehand.
pub async fn extract_streams_from_page(
    page: &Page,
    url: &str,
    wait_ms: u64,
    cancel: &CancellationToken,
) -> Result<StreamInfo> {
    let captured: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let captured_date: Arc<Mutex<Option<DateTime<Utc>>>> = Arc::new(Mutex::new(None));

    let mut request_events = page.event_listener::<EventRequestWillBeSent>().await?;
    let captured_requests = captured.clone();
    let request_task = tokio::spawn(async move {
        while let Some(event) = request_events.next().await {
            let u = &event.request.url;
            if u.contains(".m3u8") && (u.contains("audio_") || u.contains("video_") || u.contains("media_")) {
                captured_requests.lock().unwrap().push(u.clone());
            }
        }
    });

    let mut response_events = page.event_listener::<EventResponseReceived>().await?;
    let response_page = page.clone();
    let response_date = captured_date.clone();
    let response_task = tokio::spawn(async move {
        while let Some(event) = response_events.next().await {
            if response_date.lock().unwrap().is_some() {
                continue;
            }
            if !MEDIA_API_PATTERNS.iter().any(|p| event.response.url.contains(p)) {
                continue;
            }
            let Ok(body_resp) = response_page
                .execute(GetResponseBodyParams::new(event.request_id.clone()))
                .await
            else {
                continue;
            };
            if body_resp.result.base64_encoded {
                continue;
            }
            let Ok(data) = serde_json::from_str::<serde_json::Value>(&body_resp.result.body) else {
                continue;
            };
            let Some(obj) = data.as_object() else {
                continue;
            };
            for key in ["add_date", "creation_date", "record_date", "published_at"] {
                if let Some(val) = obj.get(key) {
                    let raw = match val {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    if let Some(date) = parse_date(&raw) {
                        *response_date.lock().unwrap() = Some(date);
                        break;
                    }
                }
            }
        }
    });

    page.goto(url).await?;
    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;

    if let Some(current_url) = page.url().await? {
        if current_url.contains("login/index.php") {
            request_task.abort();
            response_task.abort();
            return Err(MvbdError::new("Non authentifié (redirigé vers login)."));
        }
    }

    try_play_video(page).await;

    let chunk_ms = 300u64;
    let mut elapsed_ms = 0u64;
    while elapsed_ms < wait_ms {
        if cancel.is_cancelled() {
            request_task.abort();
            response_task.abort();
            return Err(MvbdError::new("Interrompu"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(chunk_ms)).await;
        elapsed_ms += chunk_ms;
        let snapshot = captured.lock().unwrap().clone();
        let has_audio = snapshot.iter().any(|u| u.contains("audio_"));
        let has_video = snapshot.iter().any(|u| u.contains("video_") || u.contains("media_"));
        if has_audio && has_video {
            break;
        }
    }

    let snapshot = captured.lock().unwrap().clone();
    let mut unique = Vec::new();
    for u in snapshot {
        if !unique.contains(&u) {
            unique.push(u);
        }
    }
    let audio = unique.iter().find(|u| u.contains("audio_")).cloned().unwrap_or_default();
    let video_candidates: Vec<String> = unique
        .into_iter()
        .filter(|u| u.contains("video_") || u.contains("media_"))
        .collect();
    let video = resolve_best_video_stream(video_candidates).await;

    let raw_title = extract_activity_title(page).await;

    let pub_date = {
        let d = *captured_date.lock().unwrap();
        if d.is_some() {
            d
        } else {
            extract_activity_date(page).await
        }
    };

    request_task.abort();
    response_task.abort();

    if audio.is_empty() || video.is_empty() {
        return Err(MvbdError::new("Impossible de trouver les URLs m3u8 audio/vidéo."));
    }

    let cleaned = raw_title
        .replace("| Moodle UniNE", "")
        .replace(['►', '◄'], "");
    let title = crate::util::sanitize_name(cleaned.trim(), "enregistrement");

    Ok(StreamInfo {
        title,
        audio_m3u8: audio,
        video_m3u8: video,
        pub_date,
    })
}

/// Scans a Moodle course page for UbiCast/mediaserver/panopto activity
/// links. Mirrors `cli/scraper.py::scan_course_for_ubicast_links`.
pub async fn scan_course_for_links(page: &Page, course_url: &str) -> Result<Vec<LinkEntry>> {
    page.goto(course_url).await?;
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;

    let title = match page.find_element("h1").await {
        Ok(el) => el
            .inner_text()
            .await
            .ok()
            .flatten()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Cours".to_string()),
        Err(_) => "Cours".to_string(),
    };

    // Scroll to trigger lazy loading, then back to the top.
    let _ = page.evaluate("() => window.scrollTo(0, document.body.scrollHeight)").await;
    tokio::time::sleep(std::time::Duration::from_millis(2000)).await;
    let _ = page.evaluate("() => window.scrollTo(0, 0)").await;
    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

    // chromiumoxide has no `eval_on_selector_all` equivalent, so collect
    // anchors and data-href attributes via self-contained scripts instead.
    let mut hrefs: Vec<String> = page
        .evaluate(
            r#"() => Array.from(document.querySelectorAll('a[href]'))
                .map(e => e.href)"#,
        )
        .await?
        .into_value::<Vec<String>>()
        .unwrap_or_default();

    let data_hrefs: Vec<String> = page
        .evaluate(
            r#"() => Array.from(document.querySelectorAll('[data-href], [href]'))
                .map(e => e.getAttribute('data-href') || e.getAttribute('href'))
                .filter(h => h)"#,
        )
        .await
        .ok()
        .and_then(|r| r.into_value::<Vec<String>>().ok())
        .unwrap_or_default();
    hrefs.extend(data_hrefs);

    let course_host = url::Url::parse(course_url).ok().and_then(|u| u.host_str().map(str::to_string));

    let mut unique = Vec::new();
    for href in hrefs {
        if href.is_empty() {
            continue;
        }
        if !UBICAST_PATTERNS.iter().any(|p| href.contains(p)) {
            continue;
        }
        let same_host = url::Url::parse(&href)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            == course_host;
        if !same_host {
            continue;
        }
        if !unique.contains(&href) {
            unique.push(href);
        }
    }

    Ok(unique
        .into_iter()
        .map(|url| LinkEntry {
            url,
            course: title.clone(),
        })
        .collect())
}
