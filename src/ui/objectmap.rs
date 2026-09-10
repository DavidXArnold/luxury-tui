use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;
use crate::ui::rounded_block;

pub fn draw(frame: &mut Frame, app: &App, area: Rect) {
    let block = rounded_block(&app.theme, " Object Map ");
    let text = if app.object_map.rendered.is_empty() {
        "Select a pod on the Workloads screen and press 'm' to map its relationships.".to_string()
    } else {
        app.object_map.rendered.clone()
    };
    frame.render_widget(Paragraph::new(text).block(block), area);
}
