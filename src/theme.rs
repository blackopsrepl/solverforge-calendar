use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};

/* The resolved color palette used across all UI modules. */
#[derive(Debug, Clone)]
pub struct Theme {
    pub accent: Color,
    pub background: Color,
    pub foreground: Color,
    pub cursor: Color,
    pub selection_fg: Color,
    pub selection_bg: Color,
    pub color0: Color,
    pub color1: Color,
    pub color2: Color,
    pub color3: Color,
    pub color4: Color,
    pub color5: Color,
    pub color6: Color,
    pub color7: Color,
    pub color8: Color,
    pub color9: Color,
    pub color10: Color,
    pub color11: Color,
    pub color12: Color,
    pub color13: Color,
    pub color14: Color,
    pub color15: Color,
}

/* Calendar color palette: 8 distinct calendar colors cycling through the theme. */
/* Index 0-7 map to color1..color7 + accent. */
pub const CALENDAR_COLORS: usize = 8;

impl Theme {
    // ── Shared semantic styles (identical to solverforge-mail) ──────────

    pub fn header(&self) -> Style {
        Style::default().fg(self.background).bg(self.accent)
    }

    pub fn selected(&self) -> Style {
        Style::default()
            .fg(self.selection_fg)
            .bg(self.selection_bg)
            .add_modifier(Modifier::BOLD)
    }

    pub fn folder_active(&self) -> Style {
        Style::default()
            .fg(self.background)
            .bg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn folder_inactive(&self) -> Style {
        Style::default().fg(self.color7)
    }

    pub fn status_bar(&self) -> Style {
        Style::default().fg(self.foreground).bg(self.color0)
    }

    pub fn status_key(&self) -> Style {
        Style::default()
            .fg(self.background)
            .bg(self.color4)
            .add_modifier(Modifier::BOLD)
    }

    pub fn status_desc(&self) -> Style {
        Style::default().fg(self.color8)
    }

    pub fn border(&self) -> Style {
        Style::default().fg(self.color8)
    }

    pub fn border_focused(&self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn dimmed(&self) -> Style {
        Style::default().fg(self.color8)
    }

    pub fn normal(&self) -> Style {
        Style::default().fg(self.foreground)
    }

    pub fn error(&self) -> Style {
        Style::default()
            .fg(Color::Rgb(224, 108, 117))
            .add_modifier(Modifier::BOLD)
    }

    pub fn accent_style(&self) -> Style {
        Style::default().fg(self.accent)
    }

    pub fn header_label(&self) -> Style {
        Style::default()
            .fg(self.color4)
            .add_modifier(Modifier::BOLD)
    }

    pub fn header_value(&self) -> Style {
        Style::default().fg(self.foreground)
    }

    pub fn popup(&self) -> Style {
        Style::default().fg(self.foreground).bg(self.color0)
    }

    pub fn popup_title(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    pub fn search_input(&self) -> Style {
        Style::default().fg(self.foreground).bg(self.color0)
    }

    pub fn spinner(&self) -> Style {
        Style::default()
            .fg(self.color6)
            .add_modifier(Modifier::BOLD)
    }

    // ── Calendar-specific styles ─────────────────────────────────────

    // Color for a calendar by index (0-based). Cycles through the palette.
    // Returns the Color (not a Style) so callers can build fg/bg variants.
    pub fn calendar_color(&self, index: usize) -> Color {
        match index % CALENDAR_COLORS {
            0 => self.color1, // bright green
            1 => self.color3, // cyan-green
            2 => self.color6, // bright cyan
            3 => self.color7, // light cyan
            4 => self.color4, // muted blue
            5 => self.color5, // light blue
            6 => self.color9, // bright green variant
            _ => self.accent, // mint green
        }
    }

    // Style for a calendar bullet / dot with the given palette index.
    pub fn calendar_dot(&self, index: usize) -> Style {
        Style::default()
            .fg(self.calendar_color(index))
            .add_modifier(Modifier::BOLD)
    }

    // The pulsing "Now" beam line — accent color, bold.
    pub fn now_beam(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    // Dimmed style for past time slots in week/day view.
    pub fn past_dim(&self) -> Style {
        Style::default().fg(self.color8)
    }

    // The colored left-rail of an event ribbon (▌ character).
    pub fn event_rail(&self, cal_index: usize) -> Style {
        Style::default()
            .fg(self.calendar_color(cal_index))
            .add_modifier(Modifier::BOLD)
    }

    // Event title text in time-grid views.
    pub fn event_title(&self) -> Style {
        Style::default().fg(self.foreground)
    }

    // Event title when the event is selected.
    pub fn event_selected(&self) -> Style {
        Style::default()
            .fg(self.selection_fg)
            .bg(self.selection_bg)
            .add_modifier(Modifier::BOLD)
    }

    // Today's date number highlight in the month grid.
    pub fn today_cell(&self) -> Style {
        Style::default()
            .fg(self.background)
            .bg(self.accent)
            .add_modifier(Modifier::BOLD)
    }

    // Weekend day numbers (slightly dimmed).
    pub fn weekend(&self) -> Style {
        Style::default().fg(self.color8)
    }

    // Day from adjacent month (very dimmed).
    pub fn adjacent_month(&self) -> Style {
        Style::default().fg(Color::Rgb(60, 63, 90))
    }

    // Project badge text: `[P]` inline in status bar.
    pub fn project_badge(&self) -> Style {
        Style::default()
            .fg(self.background)
            .bg(self.color3)
            .add_modifier(Modifier::BOLD)
    }

    // Progress bar filled segment.
    pub fn progress_filled(&self) -> Style {
        Style::default().fg(self.accent)
    }

    // Progress bar empty segment.
    pub fn progress_empty(&self) -> Style {
        Style::default().fg(self.color8)
    }

    // Quick-add bar label badge.
    pub fn quick_add_label(&self) -> Style {
        Style::default()
            .fg(self.background)
            .bg(self.color3)
            .add_modifier(Modifier::BOLD)
    }

    // Form field label.
    pub fn form_label(&self) -> Style {
        Style::default()
            .fg(self.color4)
            .add_modifier(Modifier::BOLD)
    }

    // Form field value (editable).
    pub fn form_value(&self) -> Style {
        Style::default().fg(self.foreground)
    }

    // Form field when focused (accent border).
    pub fn form_focused(&self) -> Style {
        Style::default()
            .fg(self.foreground)
            .bg(Color::Rgb(20, 22, 40))
    }

    // Agenda date section header.
    pub fn agenda_date_header(&self) -> Style {
        Style::default()
            .fg(self.accent)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    }

    // "TODAY" label in agenda.
    pub fn agenda_today(&self) -> Style {
        Style::default()
            .fg(self.background)
            .bg(self.accent)
            .add_modifier(Modifier::BOLD)
    }
}

// ── Singleton ────────────────────────────────────────────────────────

static THEME: OnceLock<Theme> = OnceLock::new();

/* Returns the global theme, loading it once on first access. */
pub fn theme() -> &'static Theme {
    THEME.get_or_init(|| load_theme().unwrap_or_else(|_| fallback_theme()))
}

// ── Loading ──────────────────────────────────────────────────────────

fn colors_toml_path() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let path = home.join(".local/share/solverforge/default/theme/colors.toml");
    if path.exists() {
        Some(path)
    } else {
        None
    }
}

fn load_theme() -> anyhow::Result<Theme> {
    let path = colors_toml_path().ok_or_else(|| anyhow::anyhow!("colors.toml not found"))?;
    let content = std::fs::read_to_string(&path)?;
    parse_colors_toml(&content)
}

pub fn parse_colors_toml(content: &str) -> anyhow::Result<Theme> {
    let table: HashMap<String, String> = toml::from_str(content)?;
    let get = |key: &str| -> anyhow::Result<Color> {
        let hex = table
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("missing color: {key}"))?;
        parse_hex_color(hex)
    };
    Ok(Theme {
        accent: get("accent")?,
        background: get("background")?,
        foreground: get("foreground")?,
        cursor: get("cursor")?,
        selection_fg: get("selection_foreground")?,
        selection_bg: get("selection_background")?,
        color0: get("color0")?,
        color1: get("color1")?,
        color2: get("color2")?,
        color3: get("color3")?,
        color4: get("color4")?,
        color5: get("color5")?,
        color6: get("color6")?,
        color7: get("color7")?,
        color8: get("color8")?,
        color9: get("color9")?,
        color10: get("color10")?,
        color11: get("color11")?,
        color12: get("color12")?,
        color13: get("color13")?,
        color14: get("color14")?,
        color15: get("color15")?,
    })
}

pub fn parse_hex_color(hex: &str) -> anyhow::Result<Color> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 {
        anyhow::bail!("invalid hex color length: {hex}");
    }
    let r = u8::from_str_radix(&hex[0..2], 16)?;
    let g = u8::from_str_radix(&hex[2..4], 16)?;
    let b = u8::from_str_radix(&hex[4..6], 16)?;
    Ok(Color::Rgb(r, g, b))
}

pub fn fallback_theme() -> Theme {
    Theme {
        accent: Color::Rgb(130, 251, 156),
        background: Color::Rgb(11, 12, 22),
        foreground: Color::Rgb(221, 247, 255),
        cursor: Color::Rgb(221, 247, 255),
        selection_fg: Color::Rgb(11, 12, 22),
        selection_bg: Color::Rgb(221, 247, 255),
        color0: Color::Rgb(11, 12, 22),
        color1: Color::Rgb(80, 248, 114),
        color2: Color::Rgb(79, 232, 143),
        color3: Color::Rgb(80, 247, 212),
        color4: Color::Rgb(130, 157, 212),
        color5: Color::Rgb(134, 167, 223),
        color6: Color::Rgb(124, 248, 247),
        color7: Color::Rgb(133, 225, 251),
        color8: Color::Rgb(106, 110, 149),
        color9: Color::Rgb(133, 255, 157),
        color10: Color::Rgb(156, 247, 194),
        color11: Color::Rgb(164, 255, 236),
        color12: Color::Rgb(196, 210, 237),
        color13: Color::Rgb(205, 219, 244),
        color14: Color::Rgb(209, 255, 254),
        color15: Color::Rgb(221, 247, 255),
    }
}
