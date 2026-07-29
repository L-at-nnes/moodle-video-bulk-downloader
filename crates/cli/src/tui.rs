use mvbd_core::{LinkEntry, ProgressEvent, Status};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Row, Table};
use ratatui::Terminal;
use std::io::Stdout;
use std::sync::Mutex;
use std::time::SystemTime;

struct RowState {
    course: String,
    url: String,
    title: Option<String>,
    status: Status,
    audio_pct: f64,
    video_pct: f64,
    retry: u32,
}

const LOG_MAX: usize = 20;
const LOG_DISPLAY: usize = 5;

pub struct Tui {
    inner: Mutex<Inner>,
}

struct Inner {
    terminal: Terminal<CrosstermBackend<Stdout>>,
    rows: Vec<RowState>,
    logs: Vec<String>,
    header: String,
    done: usize,
}

impl Tui {
    pub fn new(
        terminal: Terminal<CrosstermBackend<Stdout>>,
        entries: &[LinkEntry],
        concurrency: usize,
        download_threads: usize,
    ) -> Self {
        let n = entries.len();
        let rows = entries
            .iter()
            .map(|e| RowState {
                course: e.course.clone(),
                url: e.url.clone(),
                title: None,
                status: Status::Queued,
                audio_pct: 0.0,
                video_pct: 0.0,
                retry: 0,
            })
            .collect();

        let header = format!(
            "Moodle Video Downloader  |  {n} vidéo{s}  |  {concurrency} worker{ws}  |  {download_threads} thread{ts} ffmpeg",
            s = if n > 1 { "s" } else { "" },
            ws = if concurrency > 1 { "s" } else { "" },
            ts = if download_threads > 1 { "s" } else { "" },
        );

        Self {
            inner: Mutex::new(Inner {
                terminal,
                rows,
                logs: Vec::new(),
                header,
                done: 0,
            }),
        }
    }

    pub fn handle(&self, event: ProgressEvent) {
        let mut inner = self.inner.lock().unwrap();

        match event {
            ProgressEvent::Status { idx, status } => {
                if let Some(row) = inner.rows.get_mut(idx) {
                    row.status = status;
                }
            }
            ProgressEvent::Title { idx, title } => {
                if let Some(row) = inner.rows.get_mut(idx) {
                    row.title = Some(title);
                }
            }
            ProgressEvent::Progress { idx, audio, video } => {
                if let Some(row) = inner.rows.get_mut(idx) {
                    if let Some(a) = audio {
                        row.audio_pct = a;
                    }
                    if let Some(v) = video {
                        row.video_pct = v;
                    }
                }
            }
            ProgressEvent::Retry { idx, attempt } => {
                if let Some(row) = inner.rows.get_mut(idx) {
                    row.status = Status::Retrying;
                    row.retry = attempt;
                }
            }
            ProgressEvent::Done { idx, .. } => {
                if let Some(row) = inner.rows.get_mut(idx) {
                    row.status = Status::Done;
                    row.audio_pct = 100.0;
                    row.video_pct = 100.0;
                    let label = row.title.clone().unwrap_or_else(|| row.url.clone());
                    inner.done += 1;
                    push_log(&mut inner.logs, format!("{}  \u{2713}  {label}", ts()));
                }
            }
            ProgressEvent::Error { idx, message } => {
                if let Some(row) = inner.rows.get_mut(idx) {
                    row.status = Status::Error;
                    let label = row.title.clone().unwrap_or_else(|| row.url.clone());
                    inner.done += 1;
                    push_log(
                        &mut inner.logs,
                        format!("{}  \u{2717}  {label}: {}", ts(), truncate(&message, 70)),
                    );
                }
            }
        }

        inner.render();
    }

    pub fn teardown(&self) {
        let mut inner = self.inner.lock().unwrap();
        let _ = inner.terminal.show_cursor();
    }
}

fn push_log(logs: &mut Vec<String>, line: String) {
    logs.push(line);
    if logs.len() > LOG_MAX {
        let excess = logs.len() - LOG_MAX;
        logs.drain(0..excess);
    }
}

fn ts() -> String {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (now / 3600) % 24;
    let m = (now / 60) % 60;
    let s = now % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect::<String>() + "…"
    }
}

fn status_icon(status: Status) -> (&'static str, Color) {
    match status {
        Status::Queued => ("\u{00B7}", Color::DarkGray),
        Status::Extracting => ("\u{27F3}", Color::Yellow),
        Status::Downloading => ("\u{2193}", Color::Cyan),
        Status::Muxing => ("\u{2699}", Color::Blue),
        Status::Retrying => ("\u{21BB}", Color::Yellow),
        Status::Done => ("\u{2713}", Color::Green),
        Status::Error => ("\u{2717}", Color::Red),
    }
}

fn status_label(status: Status, retry: u32) -> String {
    match status {
        Status::Queued => "En attente".to_string(),
        Status::Extracting => "Extraction".to_string(),
        Status::Downloading => "Téléchargement".to_string(),
        Status::Muxing => "Muxage".to_string(),
        Status::Retrying => format!("Retry {retry}/3"),
        Status::Done => "Terminé".to_string(),
        Status::Error => "Erreur".to_string(),
    }
}

fn bar(pct: f64, width: usize) -> String {
    let filled = ((pct / 100.0) * width as f64).round().clamp(0.0, width as f64) as usize;
    "\u{2588}".repeat(filled) + &"\u{2591}".repeat(width - filled)
}

impl Inner {
    fn render(&mut self) {
        let header = self.header.clone();
        let total = self.rows.len();
        let done = self.done;
        let logs = self.logs.clone();

        // Snapshot row data so the closure below doesn't borrow `self.rows`
        // while `self.terminal` is also borrowed mutably by `draw`.
        let snapshot: Vec<(String, Option<String>, String, Status, f64, f64, u32)> = self
            .rows
            .iter()
            .map(|r| {
                (
                    r.course.clone(),
                    r.title.clone(),
                    r.url.clone(),
                    r.status,
                    r.audio_pct,
                    r.video_pct,
                    r.retry,
                )
            })
            .collect();

        let _ = self.terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Length(3),
                    Constraint::Min(5),
                    Constraint::Length(LOG_DISPLAY as u16 + 2),
                ])
                .split(area);

            f.render_widget(
                Paragraph::new(header).block(Block::default().borders(Borders::ALL)),
                chunks[0],
            );

            let ratio = if total > 0 { done as f64 / total as f64 } else { 0.0 };
            f.render_widget(
                Gauge::default()
                    .block(Block::default().borders(Borders::ALL).title("Progression globale"))
                    .gauge_style(Style::default().fg(Color::Blue))
                    .ratio(ratio.clamp(0.0, 1.0))
                    .label(format!("{done}/{total}")),
                chunks[1],
            );

            render_table(f, chunks[2], &snapshot);

            let display_logs = if logs.len() > LOG_DISPLAY {
                &logs[logs.len() - LOG_DISPLAY..]
            } else {
                &logs[..]
            };
            let log_text = if display_logs.is_empty() {
                "En attente…".to_string()
            } else {
                display_logs.join("\n")
            };
            f.render_widget(
                Paragraph::new(log_text).block(Block::default().borders(Borders::ALL).title("Journal")),
                chunks[3],
            );
        });
    }
}

#[allow(clippy::type_complexity)]
fn render_table(
    f: &mut ratatui::Frame,
    area: Rect,
    snapshot: &[(String, Option<String>, String, Status, f64, f64, u32)],
) {
    let total = snapshot.len();
    let max_rows = area.height.saturating_sub(3).max(1) as usize;

    let (start, visible_title) = if total <= max_rows {
        (0, "Vidéos".to_string())
    } else {
        let focus = snapshot
            .iter()
            .position(|(_, _, _, status, _, _, _)| {
                matches!(
                    status,
                    Status::Extracting | Status::Downloading | Status::Muxing | Status::Retrying
                )
            })
            .unwrap_or(total - 1);
        let start = focus.saturating_sub(max_rows / 3).min(total - max_rows);
        let end = start + max_rows;
        let mut title = format!("Vidéos ({}–{} / {total})", start + 1, end);
        if start > 0 {
            title.push_str(&format!("  ↑{start}"));
        }
        if total - end > 0 {
            title.push_str(&format!("  ↓{}", total - end));
        }
        (start, title)
    };
    let end = (start + max_rows).min(total);

    let rows: Vec<Row> = snapshot[start..end]
        .iter()
        .enumerate()
        .map(|(i, (course, title, url, status, apct, vpct, retry))| {
            let (icon, color) = status_icon(*status);
            let label = title.clone().unwrap_or_else(|| truncate(url, 50));
            let state_text = format!("{icon} {}", status_label(*status, *retry));
            let av_text = if *status == Status::Downloading {
                format!("{}\u{00B7}{}", bar(*apct, 5), bar(*vpct, 5))
            } else {
                String::new()
            };

            Row::new(vec![
                (start + i + 1).to_string(),
                truncate(course, 22),
                label,
                state_text,
                av_text,
            ])
            .style(Style::default().fg(color))
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(4),
            Constraint::Length(22),
            Constraint::Percentage(45),
            Constraint::Length(20),
            Constraint::Length(13),
        ],
    )
    .header(
        Row::new(vec!["#", "Cours", "Titre", "État", "A / V"])
            .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().borders(Borders::ALL).title(visible_title));

    f.render_widget(table, area);
}
