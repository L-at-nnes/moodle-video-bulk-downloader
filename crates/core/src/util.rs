use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static INVALID_CHARS: OnceLock<Regex> = OnceLock::new();
static WHITESPACE: OnceLock<Regex> = OnceLock::new();

pub fn sanitize_name(value: &str, fallback: &str) -> String {
    let invalid = INVALID_CHARS.get_or_init(|| Regex::new(r#"[\\/:*?"<>|]"#).unwrap());
    let whitespace = WHITESPACE.get_or_init(|| Regex::new(r"\s+").unwrap());

    let cleaned = invalid.replace_all(value, "_");
    let cleaned = whitespace.replace_all(cleaned.trim(), " ");
    let cleaned = cleaned.trim();

    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.chars().take(180).collect()
    }
}

pub fn unique_output_path(base_dir: &Path, filename: &str) -> PathBuf {
    let candidate = base_dir.join(format!("{filename}.mkv"));
    if !candidate.exists() {
        return candidate;
    }

    let mut index = 2;
    loop {
        let candidate = base_dir.join(format!("{filename} ({index}).mkv"));
        if !candidate.exists() {
            return candidate;
        }
        index += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_forbidden_characters() {
        assert_eq!(sanitize_name("a/b:c*d", "x"), "a_b_c_d");
    }

    #[test]
    fn falls_back_on_empty() {
        assert_eq!(sanitize_name("   ", "fallback"), "fallback");
    }
}
