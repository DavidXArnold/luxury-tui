use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;

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
    let header = format!(" {target}  — {follow}  search: '{}' ", app.logs.search);
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
        .map(|l| highlight_line(l, &app.logs.search))
        .collect();

    let block = Block::default().borders(Borders::ALL).title(" Logs ");
    frame.render_widget(
        Paragraph::new(visible)
            .block(block)
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn highlight_line<'a>(line: &'a str, search: &str) -> Line<'a> {
    if search.is_empty() {
        return Line::from(line);
    }
    let mut spans = vec![];
    let mut rest = line;
    while let Some(idx) = rest.to_lowercase().find(&search.to_lowercase()) {
        let (before, matched_and_after) = rest.split_at(idx);
        let (matched, after) = matched_and_after.split_at(search.len());
        if !before.is_empty() {
            spans.push(Span::raw(before));
        }
        spans.push(Span::styled(
            matched,
            Style::default().add_modifier(Modifier::REVERSED),
        ));
        rest = after;
    }
    spans.push(Span::raw(rest));
    Line::from(spans)
}
