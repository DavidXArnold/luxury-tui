use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::ui::rounded_block;

pub const ACTIONS: &[&str] = &[
    "goto overview",
    "goto workloads",
    "goto attention",
    "goto logs",
    "goto object-map",
    "goto compare",
    "namespace all",
    "refresh",
    "quit",
];

pub fn matches(query: &str) -> Vec<&'static str> {
    use fuzzy_matcher::skim::SkimMatcherV2;
    use fuzzy_matcher::FuzzyMatcher;
    if query.is_empty() {
        return ACTIONS.to_vec();
    }
    let matcher = SkimMatcherV2::default();
    let mut scored: Vec<(i64, &'static str)> = ACTIONS
        .iter()
        .filter_map(|a| matcher.fuzzy_match(a, query).map(|score| (score, *a)))
        .collect();
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().map(|(_, a)| a).collect()
}

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let width = area.width.saturating_sub(10).clamp(30, 70);
    let height = 12u16.min(area.height.saturating_sub(4));
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);

    let chunks = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(3),
            ratatui::layout::Constraint::Min(3),
        ])
        .split(popup);

    let input = Paragraph::new(format!("> {}", app.palette.query))
        .block(
            rounded_block(&app.theme, " Command Palette ")
                .border_style(Style::default().fg(app.theme.accent)),
        )
        .alignment(Alignment::Left);
    frame.render_widget(input, chunks[0]);

    let results = matches(&app.palette.query);
    let items: Vec<ListItem> = results
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let style = if i == app.palette.selected {
                Style::default()
                    .fg(app.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(Line::from(*a)).style(style)
        })
        .collect();
    frame.render_widget(
        List::new(items).block(rounded_block(&app.theme, "")),
        chunks[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_every_action_in_declared_order() {
        assert_eq!(matches(""), ACTIONS.to_vec());
    }

    #[test]
    fn query_narrows_and_ranks_results() {
        let results = matches("goto logs");
        assert!(!results.is_empty());
        assert_eq!(results[0], "goto logs");
    }

    #[test]
    fn nonsense_query_returns_nothing() {
        assert!(matches("zzzzznotarealaction").is_empty());
    }
}
