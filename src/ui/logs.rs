use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::ui::rounded_block;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(3)])
        .split(area);

    let target = app
        .logs
        .target
        .as_ref()
        .map(|(ns, pod, c)| {
            format!(
                "{ns}/{pod}{}",
                c.as_deref().map(|c| format!(" [{c}]")).unwrap_or_default()
            )
        })
        .unwrap_or_else(|| {
            "(no pod selected \u{2014} pick one on Workloads and press Enter)".to_string()
        });
    let follow = if app.logs.follow {
        "following".to_string()
    } else {
        format!("paused, scrolled back {} lines", app.logs.scroll)
    };
    let search_hint = if app.logs.editing_search {
        format!("search: {}_ (Enter/Esc to stop)", app.logs.search)
    } else {
        format!("search: '{}'", app.logs.search)
    };
    let flags = format!(
        "{}{}",
        if app.logs.show_timestamps {
            ""
        } else {
            "no-ts "
        },
        if app.logs.json_pretty { "json " } else { "" }
    );
    let header = format!(" {target}  — {follow}  {flags}{search_hint} ");
    frame.render_widget(
        Paragraph::new(header).style(Style::default().fg(app.theme.dim)),
        chunks[0],
    );

    let window = chunks[1].height.saturating_sub(2) as usize; // minus the block's borders
    let total = app.logs.lines.len();
    let end = total.saturating_sub(app.logs.scroll.min(total));
    let start = end.saturating_sub(window.max(1));
    let visible: Vec<Line> = app.logs.lines[start..end]
        .iter()
        .map(|raw| highlight_line(display_text(app, raw), &app.logs.search))
        .collect();

    let block = rounded_block(&app.theme, " Logs ");
    frame.render_widget(
        Paragraph::new(visible)
            .block(block)
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

/// Applies the timestamp and JSON-logfmt display toggles to one raw log
/// line. Every stream is requested *with* timestamps (see
/// `k8s::logs::LogRequest`), so this only ever strips, never adds.
fn display_text(app: &App, raw: &str) -> String {
    let after_ts = if app.logs.show_timestamps {
        raw
    } else {
        strip_timestamp_prefix(raw)
    };
    if app.logs.json_pretty {
        if let Some(flattened) = format_json_logfmt(after_ts) {
            return flattened;
        }
    }
    after_ts.to_string()
}

/// Kubernetes timestamps a line as `<rfc3339> <message>`; strip that prefix
/// back off if it parses, otherwise leave the line untouched.
fn strip_timestamp_prefix(line: &str) -> &str {
    let Some((first, rest)) = line.split_once(' ') else {
        return line;
    };
    if chrono::DateTime::parse_from_rfc3339(first).is_ok() {
        rest
    } else {
        line
    }
}

/// Renders a JSON object log line as `key=value key2=value2 ...` — easier
/// to scan than raw JSON at a glance. Returns `None` for non-JSON lines.
fn format_json_logfmt(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let obj = value.as_object()?;
    let parts: Vec<String> = obj
        .iter()
        .map(|(k, v)| {
            let v = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            format!("{k}={v}")
        })
        .collect();
    Some(parts.join(" "))
}

fn highlight_line(line: String, search: &str) -> Line<'static> {
    if search.is_empty() {
        return Line::from(line);
    }
    let mut spans = vec![];
    let mut rest = line.as_str();
    let needle = search.to_lowercase();
    while let Some(idx) = rest.to_lowercase().find(&needle) {
        let (before, matched_and_after) = rest.split_at(idx);
        let (matched, after) = matched_and_after.split_at(search.len());
        if !before.is_empty() {
            spans.push(Span::raw(before.to_string()));
        }
        spans.push(Span::styled(
            matched.to_string(),
            Style::default().add_modifier(Modifier::REVERSED),
        ));
        rest = after;
        if rest.is_empty() {
            break;
        }
    }
    if !rest.is_empty() {
        spans.push(Span::raw(rest.to_string()));
    }
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_a_valid_rfc3339_timestamp_prefix() {
        let line = "2024-01-01T00:00:00.000000000Z listening on :8080";
        assert_eq!(strip_timestamp_prefix(line), "listening on :8080");
    }

    #[test]
    fn leaves_lines_without_a_timestamp_alone() {
        let line = "listening on :8080";
        assert_eq!(strip_timestamp_prefix(line), line);
    }

    #[test]
    fn formats_a_json_object_as_logfmt() {
        let line = r#"{"level":"info","msg":"started"}"#;
        let formatted = format_json_logfmt(line).unwrap();
        assert!(formatted.contains("level=info"));
        assert!(formatted.contains("msg=started"));
    }

    #[test]
    fn non_json_lines_are_not_formatted() {
        assert_eq!(format_json_logfmt("plain text line"), None);
    }
}
