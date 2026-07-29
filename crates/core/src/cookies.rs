use crate::error::{MvbdError, Result};
use crate::models::RawCookie;
use std::path::Path;

/// Parses a cookie file. Accepts both one-per-line `NAME=VALUE` and the
/// single-line `NAME=VALUE; NAME2=VALUE2;` format copied straight out of a
/// browser's dev tools.
pub fn parse_cookie_text(raw: &str) -> Result<Vec<RawCookie>> {
    let mut parsed = Vec::new();

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        for chunk in line.split(';').map(str::trim).filter(|c| !c.is_empty()) {
            let Some((name, value)) = chunk.split_once('=') else {
                continue;
            };
            let name = name.trim();
            let value = value.trim();
            if !name.is_empty() && !value.is_empty() {
                parsed.push(RawCookie {
                    name: name.to_string(),
                    value: value.to_string(),
                });
            }
        }
    }

    if parsed.is_empty() {
        return Err(MvbdError::new(
            "No valid cookies found. Expected format: NAME=VALUE",
        ));
    }

    Ok(parsed)
}

pub fn read_cookie_file(path: &Path) -> Result<Vec<RawCookie>> {
    if !path.exists() {
        return Err(MvbdError::new(format!(
            "Cookie file not found: {}",
            path.display()
        )));
    }

    let raw = std::fs::read_to_string(path)?;
    if raw.trim().is_empty() {
        return Err(MvbdError::new("Cookie file is empty."));
    }

    parse_cookie_text(&raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_per_line() {
        let cookies = parse_cookie_text("MoodleSession=abc\n_shibsession_x=def").unwrap();
        assert_eq!(cookies.len(), 2);
        assert_eq!(cookies[0].name, "MoodleSession");
        assert_eq!(cookies[0].value, "abc");
    }

    #[test]
    fn parses_single_line() {
        let cookies = parse_cookie_text("MoodleSession=abc; _shibsession_x=def;").unwrap();
        assert_eq!(cookies.len(), 2);
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_cookie_text("").is_err());
    }
}
