use crate::error::{MvbdError, Result};
use crate::models::LinkEntry;
use std::path::Path;
use std::sync::OnceLock;

static URL_RE: OnceLock<regex::Regex> = OnceLock::new();

fn is_url(line: &str) -> bool {
    URL_RE
        .get_or_init(|| regex::Regex::new(r"(?i)^https?://").unwrap())
        .is_match(line)
}

/// Parses a grouped input file: plain lines are course headers, URL lines
/// are attached to the most recently seen header.
pub fn parse_input_text(text: &str) -> Result<Vec<LinkEntry>> {
    let mut current_course = "Sans catégorie".to_string();
    let mut entries = Vec::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        if is_url(line) {
            entries.push(LinkEntry {
                url: line.to_string(),
                course: current_course.clone(),
            });
        } else {
            current_course = line.to_string();
        }
    }

    if entries.is_empty() {
        return Err(MvbdError::new("No valid URL found in input file."));
    }

    Ok(entries)
}

pub fn parse_input_file(path: &Path) -> Result<Vec<LinkEntry>> {
    if !path.exists() {
        return Err(MvbdError::new(format!(
            "Input file not found: {}",
            path.display()
        )));
    }
    let text = std::fs::read_to_string(path)?;
    parse_input_text(&text)
}

pub fn build_entries(single_urls: &[String], input_file: Option<&Path>) -> Result<Vec<LinkEntry>> {
    let mut entries = Vec::new();

    for url in single_urls {
        if !is_url(url) {
            return Err(MvbdError::new(format!("Invalid URL: {url}")));
        }
        entries.push(LinkEntry {
            url: url.clone(),
            course: "Téléchargements".to_string(),
        });
    }

    if let Some(path) = input_file {
        entries.extend(parse_input_file(path)?);
    }

    if entries.is_empty() {
        return Err(MvbdError::new("Provide at least one URL or an input file"));
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_urls_under_headers() {
        let text = "Course A\nhttps://x/a\nhttps://x/b\nCourse B\nhttps://x/c";
        let entries = parse_input_text(text).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].course, "Course A");
        assert_eq!(entries[2].course, "Course B");
    }

    #[test]
    fn rejects_no_urls() {
        assert!(parse_input_text("just a course name").is_err());
    }
}
