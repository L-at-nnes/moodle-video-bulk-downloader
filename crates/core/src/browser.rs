use crate::error::{MvbdError, Result};
use crate::models::RawCookie;
use chromiumoxide::cdp::browser_protocol::network::CookieParam;
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use std::path::Path;

/// A launched Chromium instance plus its background CDP event-pump task.
pub struct BrowserHandle {
    pub browser: Browser,
    handler_task: tokio::task::JoinHandle<()>,
}

impl BrowserHandle {
    /// Launches the bundled Chromium (see `crate::tools::ensure_chromium`)
    /// and spawns the task that drains its CDP event stream — required by
    /// chromiumoxide or the browser connection stalls.
    pub async fn launch(chromium_path: &Path, headless: bool) -> Result<Self> {
        let mut builder = BrowserConfig::builder().chrome_executable(chromium_path);
        if !headless {
            builder = builder.with_head();
        }
        let config = builder.build().map_err(MvbdError::new)?;
        let (browser, mut handler) = Browser::launch(config).await?;

        let handler_task = tokio::spawn(async move {
            while let Some(h) = handler.next().await {
                if h.is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            browser,
            handler_task,
        })
    }

    /// Opens a new page, injects the given cookies for `url`'s origin, then
    /// navigates to `url`. Mirrors the Playwright `storage_state` cookie
    /// injection, but derives the cookie domain from `url` instead of a
    /// hardcoded Moodle host.
    pub async fn new_authenticated_page(&self, cookies: &[RawCookie], url: &str) -> Result<Page> {
        let parsed = url::Url::parse(url).map_err(|e| MvbdError::new(format!("Invalid URL {url}: {e}")))?;
        let domain = parsed
            .host_str()
            .ok_or_else(|| MvbdError::new(format!("URL has no host: {url}")))?;

        let cookie_params = cookies
            .iter()
            .map(|c| {
                CookieParam::builder()
                    .name(&c.name)
                    .value(&c.value)
                    .domain(domain)
                    .url(url)
                    .path("/")
                    .build()
                    .map_err(MvbdError::new)
            })
            .collect::<Result<Vec<_>>>()?;

        // Cookies are set at the browser level (not the page level) since
        // that doesn't require the page to already be navigated to `url`.
        self.browser.set_cookies(cookie_params).await?;

        let page = self.browser.new_page(url).await?;
        Ok(page)
    }

    pub async fn shutdown(self) {
        drop(self.browser);
        self.handler_task.abort();
    }
}
