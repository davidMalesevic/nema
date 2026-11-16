//! Design-Tokens aus docs/UI Design/tokens.json — 1:1 übernommen, Hell und Dunkel.
//! Anwendung: theme::apply(ctx, mode) in jedem Frame (system Theme folgen).

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Response, Stroke, TextStyle, Theme, Vec2, Visuals};

pub fn hex(s: &str) -> Color32 {
    Color32::from_hex(s).expect("gültiger Hex-Farbwert")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Light,
    Dark,
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub sunken: Color32,
    pub hover: Color32,
    pub border: Color32,
    pub border_input: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub primary: Color32,
    pub on_primary: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub success_text: Color32,
    pub success_bg: Color32,
    pub danger: Color32,
    pub danger_text: Color32,
    pub danger_bg: Color32,
    pub warning_text: Color32,
    pub warning_bg: Color32,
    pub scrim: Color32,
}

pub fn palette(mode: Mode) -> Palette {
    match mode {
        Mode::Light => Palette {
            bg: hex("#fafbfc"),
            surface: hex("#ffffff"),
            sunken: hex("#f3f5f8"),
            hover: hex("#eaeff4"),
            border: hex("#e3e8ee"),
            border_input: hex("#7d8c9b"),
            text: hex("#12202e"),
            muted: hex("#5b6b7b"),
            primary: hex("#12202e"),
            on_primary: hex("#ffffff"),
            accent: hex("#f5a524"),
            on_accent: hex("#12202e"),
            success: hex("#2e9e5b"),
            success_text: hex("#1e7a44"),
            success_bg: hex("#e8f5ee"),
            danger: hex("#d64545"),
            danger_text: hex("#c23636"),
            danger_bg: hex("#fbeaea"),
            warning_text: hex("#8a5a00"),
            warning_bg: hex("#fef3dc"),
            scrim: Color32::from_rgba_unmultiplied(18, 32, 46, 97),
        },
        Mode::Dark => Palette {
            bg: hex("#0f1720"),
            surface: hex("#182430"),
            sunken: hex("#1e2c3a"),
            hover: hex("#243645"),
            border: hex("#2a3947"),
            border_input: hex("#6b7f92"),
            text: hex("#e8edf2"),
            muted: hex("#93a3b3"),
            primary: hex("#e8edf2"),
            on_primary: hex("#12202e"),
            accent: hex("#f5a524"),
            on_accent: hex("#12202e"),
            success: hex("#2e9e5b"),
            success_text: hex("#5cc98a"),
            success_bg: hex("#17332a"),
            danger: hex("#d64545"),
            danger_text: hex("#ff8585"),
            danger_bg: hex("#3a2226"),
            warning_text: hex("#f5a524"),
            warning_bg: hex("#3a2d14"),
            scrim: Color32::from_rgba_unmultiplied(5, 9, 14, 158),
        },
    }
}

/// System-Theme ermitteln (Default: Hell). Debug-Override: NEMA_THEME=dark|light.
pub fn system_mode(ctx: &egui::Context) -> Mode {
    if let Ok(v) = std::env::var("NEMA_THEME") {
        match v.to_ascii_lowercase().as_str() {
            "dark" => return Mode::Dark,
            "light" => return Mode::Light,
            _ => {}
        }
    }
    match ctx.system_theme() {
        Some(Theme::Dark) => Mode::Dark,
        _ => Mode::Light,
    }
}

/// Wendet Palette + Spacing + Text-Styles auf den Kontext an.
pub fn apply(ctx: &egui::Context, mode: Mode) {
    let p = palette(mode);
    let mut v = match mode {
        Mode::Light => Visuals::light(),
        Mode::Dark => Visuals::dark(),
    };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.faint_bg_color = p.sunken; // Grid/TableBuilder-Streifen
    v.extreme_bg_color = p.surface; // TextEdit-Fläche
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.text;
    v.selection.bg_fill = p.accent.gamma_multiply(0.35);
    v.selection.stroke = Stroke::new(1.0_f32, p.text);

    // Noninteractive (Label, Heading)
    let ni = &mut v.widgets.noninteractive;
    ni.fg_stroke = Stroke::new(1.0_f32, p.text);
    ni.weak_bg_fill = p.surface;
    ni.bg_stroke = Stroke::new(1.0_f32, p.border);
    ni.corner_radius = CornerRadius::same(4);

    // Inactive: Secondary-Buttons, Eingabefelder
    let wi = &mut v.widgets.inactive;
    wi.bg_fill = p.surface;
    wi.weak_bg_fill = p.surface;
    wi.bg_stroke = Stroke::new(1.0_f32, p.border_input);
    wi.fg_stroke = Stroke::new(1.0_f32, p.text);
    wi.corner_radius = CornerRadius::same(6);

    // Hovered
    let wh = &mut v.widgets.hovered;
    wh.bg_fill = p.hover;
    wh.weak_bg_fill = p.hover;
    wh.bg_stroke = Stroke::new(1.0_f32, p.border_input);
    wh.fg_stroke = Stroke::new(1.0_f32, p.text);
    wh.corner_radius = CornerRadius::same(6);

    // Active (gedrückt)
    let wa = &mut v.widgets.active;
    wa.bg_fill = p.sunken;
    wa.weak_bg_fill = p.sunken;
    wa.bg_stroke = Stroke::new(1.0_f32, p.text);
    wa.fg_stroke = Stroke::new(1.0_f32, p.text);
    wa.corner_radius = CornerRadius::same(6);

    ctx.set_visuals(v);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(8.0, 8.0);
    style.spacing.button_padding = Vec2::new(16.0, 8.0);
    style.spacing.window_margin = Margin::ZERO;
    style.spacing.indent = 16.0;
    style.text_styles = [
        (TextStyle::Heading, FontId::new(24.0, FontFamily::Name("Switzer SemiBold".into()))),
        (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(15.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.5, FontFamily::Monospace)),
        (TextStyle::Small, FontId::new(12.5, FontFamily::Proportional)),
    ]
    .into_iter()
    .collect();
    ctx.set_style(style);
}

/// Zeichnet den Fokus-Ring (2 px orange, 2 px Abstand) um ein Widget, wenn es fokussiert ist.
pub fn focus_ring(ui: &egui::Ui, resp: &Response, accent: Color32) {
    if ui.memory(|m| m.focused()) == Some(resp.id) {
        let r = resp.rect.expand(2.0);
        ui.painter().rect_stroke(r, CornerRadius::same(8), Stroke::new(2.0_f32, accent), egui::StrokeKind::Inside);
    }
}

/// Gekürzter Mono-Pfad (mittig gekürzt), wie in den Screens.
pub fn short_path(ui: &mut egui::Ui, path: &std::path::Path, p: Palette) {
    let s = path.display().to_string();
    let max = 52;
    let shown = if s.chars().count() > max {
        let head: String = s.chars().take(20).collect();
        let tail: String = s.chars().rev().take(max - 23).collect::<String>().chars().rev().collect();
        format!("{head}…{tail}")
    } else {
        s
    };
    ui.label(egui::RichText::new(shown).font(FontId::new(13.0, FontFamily::Monospace)).color(p.muted));
}

/// Karten-Rahmen (Surface + Rand 10 px), Inhalt via `.show(ui, …)`.
pub fn card(p: Palette, mode: Mode) -> egui::Frame {
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(10))
        .shadow(shadow_card(mode))
        .inner_margin(Margin::same(16))
}

/// Dialog-Schatten (stärker), z.B. für den Bestätigungsdialog.
pub fn shadow_dialog(mode: Mode) -> egui::Shadow {
    match mode {
        Mode::Light => egui::Shadow {
            offset: [0, 12],
            blur: 32,
            spread: 0,
            color: Color32::from_rgba_unmultiplied(18, 32, 46, 46),
        },
        Mode::Dark => egui::Shadow {
            offset: [0, 12],
            blur: 32,
            spread: 0,
            color: Color32::from_rgba_unmultiplied(0, 0, 0, 140),
        },
    }
}

fn shadow_card(mode: Mode) -> egui::Shadow {
    match mode {
        Mode::Light => egui::Shadow {
            offset: [0, 1],
            blur: 2,
            spread: 0,
            color: Color32::from_rgba_unmultiplied(18, 32, 46, 15),
        },
        Mode::Dark => egui::Shadow {
            offset: [0, 1],
            blur: 2,
            spread: 0,
            color: Color32::from_rgba_unmultiplied(0, 0, 0, 90),
        },
    }
}


