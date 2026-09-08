use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::k8s::resources::warning_events;

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
            Span::raw(format!("{ready_nodes}/{total_nodes} ready")),
        ]),
        Line::from(vec![
            Span::styled("Pods: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{running}/{total_pods} running")),
        ]),
        Line::from(vec![
            Span::styled(
                "Namespaces: ",
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{namespaces}")),
        ]),
        Line::from(vec![
            Span::styled(
                "Warning events: ",
                Style::default()
                    .fg(app.theme.warn)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{warnings}")),
        ]),
    ];
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(app.theme.border))
        .title(" Cluster Overview ");
    frame.render_widget(
        Paragraph::new(stats)
            .style(Style::default().fg(app.theme.text))
            .block(block),
        chunks[0],
    );

    if !app.port_forwards.is_empty() {
        let lines: Vec<Line> = app
            .port_forwards
            .iter()
            .map(|pf| Line::from(format!("\u{2192} {pf}")))
            .collect();
        let pf_block = Block::default()
            .borders(Borders::ALL)
            .title(" Port Forwards ");
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
    let events_block = Block::default()
        .borders(Borders::ALL)
        .title(" Recent Warning Events ");
    if items.is_empty() {
        frame.render_widget(
            Paragraph::new("No warning events \u{1f389}").block(events_block),
            chunks[2],
        );
    } else {
        frame.render_widget(List::new(items).block(events_block), chunks[2]);
    }
}
