use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::k8s::diff::DiffLine;
use crate::ui::rounded_block;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let block = rounded_block(&app.theme, " Compare ");
    if app.compare.diff.is_empty() {
        frame.render_widget(
            Paragraph::new("No comparison loaded yet. On the Workloads list, press 'v' on one item, then 'v' on another of the same kind to diff them.").block(block),
            area,
        );
        return;
    }
    let lines: Vec<Line> = app
        .compare
        .diff
        .iter()
        .map(|d| match d {
            DiffLine::Added(s) => {
                Line::from(format!("+ {s}")).style(Style::default().fg(app.theme.ok))
            }
            DiffLine::Removed(s) => {
                Line::from(format!("- {s}")).style(Style::default().fg(app.theme.error))
            }
            DiffLine::Context(s) => {
                Line::from(format!("  {s}")).style(Style::default().fg(app.theme.dim))
            }
        })
        .collect();
    frame.render_widget(Paragraph::new(lines).block(block), area);
}
