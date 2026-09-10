mod compare;
mod logs;
mod objectmap;
mod overview;
pub mod palette;
mod workloads;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, Screen, MAIN_SCREENS};
use crate::logo::{LOGO_BADGE, LOGO_LARGE};
use crate::theme::Theme;

/// A bordered block in the app's consistent style — rounded corners, tinted
/// to the current cluster's theme. Shared so every screen looks like part
/// of the same app instead of each picking its own border.
pub(super) fn rounded_block(theme: &Theme, title: impl Into<String>) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(title.into())
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    if app.screen == Screen::ContextPicker {
        draw_context_picker(frame, app);
        return;
    }

    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);

    draw_tabs(frame, app, chunks[0]);

    match app.screen {
        Screen::Overview => overview::draw(frame, app, chunks[1]),
        Screen::Workloads | Screen::Attention => workloads::draw(frame, app, chunks[1]),
        Screen::Logs => logs::draw(frame, app, chunks[1]),
        Screen::ObjectMap => objectmap::draw(frame, app, chunks[1]),
        Screen::Compare => compare::draw(frame, app, chunks[1]),
        Screen::Help => draw_help(frame, app, chunks[1]),
        Screen::ContextPicker => unreachable!(),
    }

    draw_status_bar(frame, app, chunks[2]);

    if app.palette.active {
        palette::draw(frame, app, area);
    }
    if app.debug_prompt.is_some() {
        draw_debug_prompt(frame, app, area);
    }
}

fn draw_debug_prompt(frame: &mut Frame, app: &App, area: Rect) {
    let Some(prompt) = &app.debug_prompt else {
        return;
    };
    let width = area.width.saturating_sub(10).clamp(30, 70);
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + 2,
        width,
        height: 7,
    };
    frame.render_widget(ratatui::widgets::Clear, popup);

    let text = format!(
        "Debug container image for {}/{}:\n\n> {}\n\n(Enter to launch + attach, Esc to cancel)",
        prompt.namespace, prompt.pod, prompt.image
    );
    let block = rounded_block(&app.theme, " Launch Debug Container ")
        .border_style(Style::default().fg(app.theme.accent));
    frame.render_widget(Paragraph::new(text).block(block), popup);
}

fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = MAIN_SCREENS
        .iter()
        .map(|s| Line::from(screen_label(*s)))
        .collect();
    let selected = MAIN_SCREENS
        .iter()
        .position(|s| *s == app.screen)
        .unwrap_or(0);
    let context_name = app
        .current_context_name
        .clone()
        .unwrap_or_else(|| "(no context)".into());
    let ns_label = app.namespace_filter.as_deref().unwrap_or("all namespaces");
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(app.theme.accent))
        .title(Span::styled(
            format!(" Luxury TUI · {context_name} "),
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        ))
        .title_top(
            Line::from(Span::styled(
                format!(" ns: {ns_label} "),
                Style::default().fg(app.theme.dim),
            ))
            .right_aligned(),
        );
    let tabs = Tabs::new(titles)
        .block(block)
        .select(selected)
        .highlight_style(
            Style::default()
                .fg(app.theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .divider(" ");
    frame.render_widget(tabs, area);
}

fn screen_label(s: Screen) -> &'static str {
    match s {
        Screen::Overview => "Overview",
        Screen::Workloads => "Workloads",
        Screen::Attention => "Attention",
        Screen::Logs => "Logs",
        Screen::ObjectMap => "Object Map",
        Screen::Compare => "Compare",
        Screen::Help => "Help",
        Screen::ContextPicker => "Contexts",
    }
}

fn draw_status_bar(frame: &mut Frame, app: &App, area: Rect) {
    let text = app.status_message.clone().unwrap_or_else(|| {
        "?:help  Tab:next  Shift+Tab:prev  n:namespace  :  command palette  q:quit".to_string()
    });
    let style = Style::default().fg(app.theme.dim);
    frame.render_widget(Paragraph::new(text).style(style), area);
}

fn draw_help(frame: &mut Frame, app: &App, area: Rect) {
    let help = format!(
        "{LOGO_BADGE}\n\
Navigation
  Tab / Shift+Tab      cycle screens
  1..6                 jump to screen
  Up/Down, j/k         move selection
  Enter                drill in / view logs (Pods)
  n                    cycle namespace filter (all -> ns1 -> ns2 -> ...)
  t                    cycle this cluster's color theme
  r                    refresh
  f                    toggle log follow (Logs screen)
  Up/Down              scroll back through logs (Logs screen)
  /                    start a search; Enter/Esc to stop typing (Logs screen)
  T                    toggle log timestamps (Logs screen)
  j                    toggle JSON logfmt view (Logs screen)
  m                    map object relationships for selected item
  v                    mark item for compare; press again on a same-kind
                       item to diff (Workloads/Attention, any kind)
  c                    cordon/uncordon selected node (Workloads: Nodes)
  d                    drain selected node (Workloads: Nodes)
  D                    delete selected node (Workloads: Nodes)
  s                    open interactive shell in selected pod
  S                    launch a debug container in selected pod, then shell
  p                    start a port-forward to selected pod
  :                    open command palette
  ?                    this help screen
  q / Esc              quit / back
"
    );
    let block = rounded_block(&app.theme, " Help ");
    frame.render_widget(Paragraph::new(help).block(block), area);
}

fn draw_context_picker(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    // Sized from the actual logo text so a future redesign can't silently
    // overflow this chunk and overlap the list below it again.
    let logo_height = LOGO_LARGE.lines().count() as u16;
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(logo_height),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);

    let logo = Paragraph::new(LOGO_LARGE)
        .alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().fg(app.theme.accent));
    frame.render_widget(logo, chunks[0]);

    let items: Vec<Line> = if app.contexts.is_empty() {
        vec![Line::from(
            "No kubeconfig files found (checked $KUBECONFIG / ~/.kube). Press q to quit.",
        )]
    } else {
        app.contexts
            .iter()
            .enumerate()
            .map(|(i, ctx)| {
                let marker = if i == app.context_selected {
                    "> "
                } else {
                    "  "
                };
                let mut tags = String::new();
                if ctx.is_current {
                    tags.push_str(" (current)");
                }
                if ctx.is_default_file {
                    tags.push_str(" (default)");
                }
                let style = if ctx.invalid {
                    Style::default().fg(app.theme.error)
                } else if i == app.context_selected {
                    Style::default()
                        .fg(app.theme.accent)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let default_ns = ctx.namespace.as_deref().unwrap_or("default");
                let invalid_note = ctx
                    .invalid_reason
                    .as_deref()
                    .map(|r| format!("  \u{26a0} {r}"))
                    .unwrap_or_default();
                Line::from(Span::styled(
                    format!(
                        "{marker}[{}] {}  cluster={} user={} ns={}{tags}{invalid_note}",
                        ctx.source_display, ctx.name, ctx.cluster, ctx.user, default_ns
                    ),
                    style,
                ))
            })
            .collect()
    };
    let block = rounded_block(
        &app.theme,
        " Select a cluster context (\u{2191}/\u{2193}, Enter) ",
    );
    frame.render_widget(Paragraph::new(items).block(block), chunks[1]);

    let footer = Paragraph::new(
        "Luxury TUI \u{2014} a terminal companion inspired by Luxury Yacht by John Jeffers",
    )
    .alignment(ratatui::layout::Alignment::Center)
    .style(Style::default().fg(app.theme.dim));
    frame.render_widget(footer, chunks[2]);
}
