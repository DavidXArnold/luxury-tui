use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use ratatui::Frame;

use crate::app::{App, Screen, WORKLOAD_KINDS};
use crate::k8s::resources::{attention_count, filter_attention};
use crate::ui::rounded_block;

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
    // The namespace filter is shown in the top bar (right-aligned title) —
    // this panel is too narrow (Length(20)) to also fit a namespace name
    // without truncating it.
    let block = rounded_block(&app.theme, " Kinds ");
    frame.render_widget(List::new(items).block(block), area);
}

fn draw_namespace_summary(frame: &mut Frame, app: &App, area: Rect) {
    // A count-only pass, not filter_attention()'s full clone-and-collect —
    // this and draw_items() below both want "pods needing attention" every
    // render, and at a few thousand pods there's no reason to clone that
    // set twice just because two panels want to know about it.
    let count = attention_count(&app.pods);
    let block = rounded_block(&app.theme, " Attention ");
    let text = format!("{count} pod(s)\nneed attention");
    frame.render_widget(ratatui::widgets::Paragraph::new(text).block(block), area);
}

/// A `phase-word` + rest-of-line row, colored by what the phase word means
/// (Running=ok, Pending=warn, Failed=error, ...) instead of one flat color
/// for the whole line.
fn phase_row(
    theme: &crate::theme::Theme,
    phase: &str,
    phase_width: usize,
    rest: String,
) -> ListItem<'static> {
    ListItem::new(Line::from(vec![
        Span::styled(format!("{phase:<phase_width$} "), theme.phase_style(phase)),
        Span::raw(rest),
    ]))
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
                phase_row(
                    &app.theme,
                    &p.phase,
                    10,
                    format!(
                        "{}/{:<20} {:<3}/{:<3} restarts={}",
                        p.namespace, p.name, p.ready.0, p.ready.1, p.restarts
                    ),
                )
            })
            .collect()
    } else if app.workload_kind == crate::app::WorkloadKind::Nodes {
        app.nodes
            .iter()
            .map(|n| {
                let status = if n.ready { "Ready" } else { "NotReady" };
                let sched = if n.schedulable { "" } else { " (cordoned)" };
                let rest = format!("{:<20} {:<20} {}{}", n.name, n.roles, n.version, sched);
                phase_row(&app.theme, status, 10, rest)
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
                let rest = format!(
                    "{}/{:<20} {:<3}/{:<3} {:<28} {:<16} {}",
                    p.namespace,
                    p.name,
                    p.ready.0,
                    p.ready.1,
                    owner,
                    node,
                    format_age(p.age_seconds)
                );
                phase_row(&app.theme, &p.phase, 10, rest)
            })
            .collect()
    } else {
        app.active_workloads()
            .iter()
            .map(|w| {
                // `ready` here is a count like "2/3", not a phase word —
                // color it green when fully ready, yellow otherwise.
                let fully_ready = w
                    .ready
                    .split_once('/')
                    .is_some_and(|(a, b)| !a.is_empty() && a == b);
                let ready_style = if fully_ready {
                    Style::default().fg(app.theme.ok)
                } else {
                    Style::default().fg(app.theme.warn)
                };
                ListItem::new(Line::from(vec![
                    Span::raw(format!(
                        "{:<12} {:<20} {:<20} ",
                        w.kind, w.namespace, w.name
                    )),
                    Span::styled(w.ready.clone(), ready_style),
                ]))
            })
            .collect()
    };

    let block = rounded_block(&app.theme, title);
    if rows.is_empty() {
        app.selected = None;
        frame.render_widget(
            ratatui::widgets::Paragraph::new("(nothing here \u{2014} press r to refresh)")
                .block(block),
            area,
        );
        return;
    }

    // Resolve the selection by identity, not by trusting a stale index —
    // the API can (and does) return the same items in a different order
    // between refreshes, which would otherwise silently move the highlight
    // onto a different resource than the one the user actually picked.
    let refs = app.visible_item_refs(attention_only);
    let selected_idx = app.resolve_selection(&refs);
    let mut state = ListState::default();
    state.select(selected_idx);

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
