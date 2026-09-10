use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::k8s::resources::warning_events;
use crate::ui::rounded_block;

fn count_style(theme: &crate::theme::Theme, healthy: usize, total: usize) -> Style {
    let color = if total == 0 || healthy == total {
        theme.ok
    } else {
        theme.warn
    };
    Style::default().fg(color)
}

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let pf_height = if app.port_forwards.is_empty() {
        0
    } else {
        app.port_forwards.len().min(4) as u16 + 2
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Length(pf_height),
            Constraint::Min(3),
        ])
        .split(area);

    let running = app.pods.iter().filter(|p| p.phase == "Running").count();
    let total_pods = app.pods.len();
    let ready_nodes = app.nodes.iter().filter(|n| n.ready).count();
    let total_nodes = app.nodes.len();
    let namespaces = app.namespaces.len();
    let warnings = warning_events(&app.events).len();

    let stats = vec![
        Line::from(vec![
            Span::styled("Nodes: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{ready_nodes}/{total_nodes} ready"),
                count_style(&app.theme, ready_nodes, total_nodes),
            ),
        ]),
        Line::from(vec![
            Span::styled("Pods: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{running}/{total_pods} running"),
                count_style(&app.theme, running, total_pods),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                "Namespaces: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{namespaces}"), Style::default().fg(app.theme.text)),
        ]),
        Line::from(vec![
            Span::styled(
                "Warning events: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{warnings}"),
                if warnings == 0 {
                    Style::default().fg(app.theme.ok)
                } else {
                    Style::default().fg(app.theme.warn)
                },
            ),
        ]),
    ];
    let block = rounded_block(&app.theme, " Cluster Overview ");
    frame.render_widget(Paragraph::new(stats).block(block), chunks[0]);

    if !app.port_forwards.is_empty() {
        let lines: Vec<Line> = app
            .port_forwards
            .iter()
            .map(|pf| Line::from(format!("\u{2192} {pf}")))
            .collect();
        let pf_block = rounded_block(&app.theme, " Port Forwards ");
        frame.render_widget(
            Paragraph::new(lines)
                .style(Style::default().fg(app.theme.dim))
                .block(pf_block),
            chunks[1],
        );
    }

    let mut events = warning_events(&app.events);
    events.truncate(200);
    let items: Vec<ListItem> = events
        .iter()
        .map(|e| {
            ListItem::new(format!(
                "{:<10} x{:<3} {}/{}/{}  {}: {}",
                e.type_, e.count, e.namespace, e.kind, e.name, e.reason, e.message
            ))
            .style(Style::default().fg(app.theme.warn))
        })
        .collect();
    let events_block = rounded_block(&app.theme, " Recent Warning Events ");
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new("No warning events \u{1f389}").block(events_block),
            chunks[2],
        );
    } else {
        frame.render_widget(List::new(items).block(events_block), chunks[2]);
    }
}
