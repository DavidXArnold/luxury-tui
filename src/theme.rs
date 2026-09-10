//! Per-cluster color theming, mirroring Luxury Yacht's "assign a color to each
//! cluster so you never `kubectl delete` in the wrong one" feature.

use ratatui::style::{Color, Style};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Palette {
    Ocean,
    Sunset,
    Forest,
    Volcano,
    Grape,
    Slate,
}

impl Palette {
    pub const ALL: [Palette; 6] = [
        Palette::Ocean,
        Palette::Sunset,
        Palette::Forest,
        Palette::Volcano,
        Palette::Grape,
        Palette::Slate,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Palette::Ocean => "ocean",
            Palette::Sunset => "sunset",
            Palette::Forest => "forest",
            Palette::Volcano => "volcano",
            Palette::Grape => "grape",
            Palette::Slate => "slate",
        }
    }

    pub fn from_name(name: &str) -> Option<Palette> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }

    /// Deterministically pick a palette for a cluster/context name so a given
    /// cluster tends to keep "its" color even before the user customizes it.
    pub fn for_context(name: &str) -> Palette {
        let hash: u32 = name
            .bytes()
            .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u32));
        Self::ALL[(hash as usize) % Self::ALL.len()]
    }

    pub fn accent(&self) -> Color {
        match self {
            Palette::Ocean => Color::Rgb(0x35, 0xa8, 0xe0),
            Palette::Sunset => Color::Rgb(0xf2, 0x8c, 0x3a),
            Palette::Forest => Color::Rgb(0x4c, 0xaf, 0x6a),
            Palette::Volcano => Color::Rgb(0xe0, 0x4b, 0x4b),
            Palette::Grape => Color::Rgb(0xa0, 0x6c, 0xd5),
            Palette::Slate => Color::Rgb(0x7f, 0x8c, 0x9e),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub accent: Color,
    pub border: Color,
    pub text: Color,
    pub dim: Color,
    pub warn: Color,
    pub error: Color,
    pub ok: Color,
}

impl Theme {
    pub fn from_palette(p: Palette) -> Self {
        Theme {
            accent: p.accent(),
            border: Color::DarkGray,
            text: Color::White,
            dim: Color::Gray,
            warn: Color::Yellow,
            error: Color::Red,
            ok: Color::Green,
        }
    }

    /// Color for a Kubernetes phase/status word (`Running`, `Pending`,
    /// `NotReady`, ...) — used to make workload/node lists scannable at a
    /// glance instead of a wall of same-colored text.
    pub fn phase_style(&self, phase: &str) -> Style {
        let color = match phase {
            "Running" | "Ready" | "Bound" | "Active" => self.ok,
            "Pending" | "ContainerCreating" | "NotReady" | "Terminating" | "SchedulingDisabled" => {
                self.warn
            }
            "Failed" | "Error" | "CrashLoopBackOff" | "ImagePullBackOff" | "Unknown"
            | "Evicted" => self.error,
            "Succeeded" | "Completed" => self.dim,
            _ => self.text,
        };
        Style::default().fg(color)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Theme::from_palette(Palette::Ocean)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_assignment_is_deterministic() {
        // Same context name must always map to the same color, run after
        // run — that's the whole point of "assign a color per cluster".
        let a = Palette::for_context("prod-us-east");
        let b = Palette::for_context("prod-us-east");
        assert_eq!(a, b);
    }

    #[test]
    fn palette_round_trips_through_its_name() {
        for p in Palette::ALL {
            assert_eq!(Palette::from_name(p.name()), Some(p));
        }
    }

    #[test]
    fn unknown_palette_name_is_rejected() {
        assert_eq!(Palette::from_name("not-a-real-palette"), None);
    }

    #[test]
    fn phase_style_maps_known_phases_to_the_right_semantic_color() {
        let theme = Theme::default();
        assert_eq!(theme.phase_style("Running").fg, Some(theme.ok));
        assert_eq!(theme.phase_style("Pending").fg, Some(theme.warn));
        assert_eq!(theme.phase_style("Failed").fg, Some(theme.error));
        assert_eq!(theme.phase_style("Succeeded").fg, Some(theme.dim));
    }

    #[test]
    fn phase_style_falls_back_to_plain_text_for_unknown_phases() {
        let theme = Theme::default();
        assert_eq!(theme.phase_style("SomeUnknownPhase").fg, Some(theme.text));
    }
}
