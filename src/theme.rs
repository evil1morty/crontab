use eframe::egui::{self, Color32, FontFamily, FontId, Rounding, Stroke, TextStyle};

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();

    // Typography
    style.text_styles.insert(TextStyle::Heading, FontId::new(20.0, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Body, FontId::new(13.5, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Button, FontId::new(13.0, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Small, FontId::new(11.5, FontFamily::Proportional));
    style.text_styles.insert(TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace));

    // Spacing
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(10.0, 5.0);
    style.spacing.indent = 14.0;
    style.spacing.window_margin = egui::Margin::same(14.0);
    style.spacing.menu_margin = egui::Margin::same(8.0);

    // Visuals — modern dark
    let mut v = egui::Visuals::dark();

    // Palette
    let bg = Color32::from_rgb(20, 22, 28);
    let panel = Color32::from_rgb(28, 31, 38);
    let card = Color32::from_rgb(34, 38, 46);
    let card_hover = Color32::from_rgb(42, 46, 56);
    let card_active = Color32::from_rgb(50, 56, 68);
    let border = Color32::from_rgb(48, 52, 62);
    let text = Color32::from_rgb(228, 230, 235);
    let muted = Color32::from_rgb(150, 155, 165);
    let accent = Color32::from_rgb(120, 170, 255);

    v.window_fill = bg;
    v.panel_fill = bg;
    v.faint_bg_color = panel;
    v.extreme_bg_color = Color32::from_rgb(14, 16, 20);
    v.code_bg_color = Color32::from_rgb(24, 27, 33);

    v.override_text_color = Some(text);
    v.hyperlink_color = accent;

    v.window_rounding = Rounding::same(10.0);
    v.menu_rounding = Rounding::same(8.0);

    let r = Rounding::same(8.0);

    v.widgets.noninteractive.bg_fill = panel;
    v.widgets.noninteractive.weak_bg_fill = panel;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, muted);
    v.widgets.noninteractive.rounding = r;

    v.widgets.inactive.bg_fill = card;
    v.widgets.inactive.weak_bg_fill = card;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
    v.widgets.inactive.rounding = r;

    v.widgets.hovered.bg_fill = card_hover;
    v.widgets.hovered.weak_bg_fill = card_hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, text);
    v.widgets.hovered.rounding = r;

    v.widgets.active.bg_fill = card_active;
    v.widgets.active.weak_bg_fill = card_active;
    v.widgets.active.bg_stroke = Stroke::new(1.0, accent);
    v.widgets.active.fg_stroke = Stroke::new(1.0, text);
    v.widgets.active.rounding = r;

    v.widgets.open.bg_fill = card_active;
    v.widgets.open.weak_bg_fill = card_active;
    v.widgets.open.bg_stroke = Stroke::new(1.0, border);
    v.widgets.open.fg_stroke = Stroke::new(1.0, text);
    v.widgets.open.rounding = r;

    v.selection.bg_fill = Color32::from_rgba_unmultiplied(120, 170, 255, 60);
    v.selection.stroke = Stroke::new(1.0, accent);

    style.visuals = v;
    ctx.set_style(style);
}

// Reusable colors for callers
pub const ACCENT: Color32 = Color32::from_rgb(120, 170, 255);
pub const MUTED: Color32 = Color32::from_rgb(150, 155, 165);
pub const SUCCESS: Color32 = Color32::from_rgb(110, 210, 140);
pub const DANGER: Color32 = Color32::from_rgb(230, 110, 110);
pub const CARD_BG: Color32 = Color32::from_rgb(34, 38, 46);
pub const BORDER: Color32 = Color32::from_rgb(48, 52, 62);
