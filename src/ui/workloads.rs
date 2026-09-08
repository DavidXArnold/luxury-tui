use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::{App, Screen, WORKLOAD_KINDS};
use crate::k8s::resources::filter_attention;

pub fn draw(frame: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(20)])
        .split(area);

    if app.screen == Screen::Workloads {
        draw_kind_list(frame, app, chunks[0]);
        draw_items(frame, app, chunks[1], false);
    } else {
        draw_namespace_summary(frame, app, chunks[0]);
        draw_items(frame, app, chunks[1], true);
    }
}

fn draw_kind_list(frame: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = WORKLOAD_KINDS
        .iter()
        .map(|k| {
            let style = if *k == app.workload_kind {
                Style::default()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(k.label()).style(style)
        })
        .collect();
    let ns = app
        .namespace_filter
        .clone()
        .unwrap_or_else(|| "all".to_string());
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Kinds (ns: {ns}) "));
    frame.render_widget(List::new(items).block(block), area);
}

fn draw_namespace_summary(frame: &mut Frame, app: &App, area: Rect) {
    let attention_count = filter_attention(&app.pods).len();
    let block = Block::default().borders(Borders::ALL).title(" Attention ");
    let text = format!("{attention_count} pod(s)\nneed attention");
    frame.render_widget(ratatui::widgets::Paragraph::new(text).block(block), area);
}

fn draw_items(frame: &mut Frame, app: &mut App, area: Rect, attention_only: bool) {
    let title = if attention_only {
        " Pods needing attention ".to_string()
    } else {
        format!(" {} ", app.workload_kind.label())
    };

    let rows: Vec<ListItem> = if attention_only {
        filter_attention(&app.pods)
            .iter()
            .map(|p| {
                ListItem::new(format!(
                    "{:<10} {}/{:<20} {:<3}/{:<3} restarts={}",
                    p.phase, p.namespace, p.name, p.ready.0, p.ready.1, p.restarts
                ))
                .style(Style::default().fg(app.theme.error))
            })
            .collect()
    } else if app.workload_kind == crate::app::WorkloadKind::Nodes {
        app.nodes
            .iter()
            .map(|n| {
                let status = if n.ready { "Ready" } else { "NotReady" };
                let sched = if n.schedulable { "" } else { " (cordoned)" };
                ListItem::new(format!(
                    "{:<20} {:<10} {:<20} {}{}",
                    n.name, status, n.roles, n.version, sched
                ))
            })
            .collect()
    } else if app.workload_kind == crate::app::WorkloadKind::Pods {
        app.pods
            .iter()
            .map(|p| {
                let owner = match (&p.owner_kind, &p.owner_name) {
                    (Some(k), Some(n)) => format!("{k}/{n}"),
                    _ => "-".to_string(),
                };
                let node = p.node.as_deref().unwrap_or("-");
                ListItem::new(format!(
                    "{:<10} {}/{:<20} {:<3}/{:<3} {:<28} {:<16} {}",
                    p.phase,
                    p.namespace,
                    p.name,
                    p.ready.0,
                    p.ready.1,
                    owner,
                    node,
                    format_age(p.age_seconds)
                ))
            })
            .collect()
    } else {
        app.active_workloads()
            .iter()
            .map(|w| {
                ListItem::new(format!(
                    "{:<12} {:<20} {:<20} {}",
                    w.kind, w.namespace, w.name, w.ready
                ))
            })
            .collect()
    };

    let block = Block::default().borders(Borders::ALL).title(title);
    if rows.is_empty() {
        frame.render_widget(
            ratatui::widgets::Paragraph::new("(nothing here \u{2014} press r to refresh)")
                .block(block),
            area,
        );
        return;
    }

    let mut state = app.workload_list_state.clone();
    if state.selected().is_none() {
        state.select(Some(0));
    }
    let list = List::new(rows).block(block).highlight_style(
        Style::default()
            .fg(app.theme.accent)
            .add_modifier(Modifier::REVERSED),
    );
    frame.render_stateful_widget(list, area, &mut state);
    app.workload_list_state = state;
}

fn format_age(seconds: i64) -> String {
    let seconds = seconds.max(0);
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86400 {
        format!("{}h", seconds / 3600)
    } else {
        format!("{}d", seconds / 86400)
    }
}
