//! Design-Tokens aus docs/UI Design/tokens.json — 1:1 übernommen, Hell und Dunkel.
//! `theme::install_fonts(ctx)` einmalig, `theme::apply(ctx, mode)` bei jedem Theme-Wechsel.

use egui::{Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle, Theme, Vec2, Visuals};

// Layout-Tokens
pub const SIDEBAR_WIDTH: f32 = 220.0;
pub const RADIUS_SM: u8 = 4;
pub const RADIUS_MD: u8 = 6;
pub const RADIUS_LG: u8 = 10;
pub const HOVER_SECONDS: f32 = 0.15;

pub fn hex(s: &str) -> Color32 {
    Color32::from_hex(s).expect("gültiger Hex-Farbwert")
}

/// Lineare Mischung zweier Farben (t = 0 → a, t = 1 → b).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    a.lerp_to_gamma(b, t.clamp(0.0, 1.0))
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
    pub warning: Color32,
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
            warning: hex("#f5a524"),
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
            warning: hex("#f5a524"),
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

/// Registriert Switzer (Fontshare, free license) als App-Font.
/// Die Gewichte sind eigene Familien: egui kennt kein font-weight.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let regular: &'static [u8] = include_bytes!("../fonts/Switzer-400.ttf");
    let medium: &'static [u8] = include_bytes!("../fonts/Switzer-500.ttf");
    let semibold: &'static [u8] = include_bytes!("../fonts/Switzer-600.ttf");
    let bold: &'static [u8] = include_bytes!("../fonts/Switzer-700.ttf");
    fonts.font_data.insert("Switzer".into(), egui::FontData::from_static(regular).into());
    fonts.font_data.insert("Switzer Medium".into(), egui::FontData::from_static(medium).into());
    fonts.font_data.insert("Switzer SemiBold".into(), egui::FontData::from_static(semibold).into());
    fonts.font_data.insert("Switzer Bold".into(), egui::FontData::from_static(bold).into());

    // Fallbacks für Zeichen, die Switzer fehlen (Pfeile, Symbole): zuerst Hack
    // (gut lesbare Pfeile), danach die übrigen egui-Standardfonts.
    let mut fallbacks: Vec<String> = vec!["Hack".to_string()];
    for f in fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default() {
        if !fallbacks.contains(&f) {
            fallbacks.push(f);
        }
    }

    let mut proportional = vec!["Switzer".to_string()];
    proportional.extend(fallbacks.iter().cloned());
    fonts.families.insert(FontFamily::Proportional, proportional);

    for name in ["Switzer Medium", "Switzer SemiBold", "Switzer Bold"] {
        let mut list = vec![name.to_string()];
        list.extend(fallbacks.iter().cloned());
        fonts.families.insert(FontFamily::Name(name.into()), list);
    }
    ctx.set_fonts(fonts);
}

/// Wendet Palette, Abstände und Text-Styles an (für beide egui-Themes identisch,
/// damit der Modus allein von `mode` abhängt).
pub fn apply(ctx: &egui::Context, mode: Mode) {
    let p = palette(mode);
    let mut v = match mode {
        Mode::Light => Visuals::light(),
        Mode::Dark => Visuals::dark(),
    };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.window_stroke = Stroke::new(1.0_f32, p.border);
    v.window_corner_radius = CornerRadius::same(RADIUS_LG);
    v.menu_corner_radius = CornerRadius::same(RADIUS_MD);
    v.popup_shadow = shadow_popup(mode);
    v.faint_bg_color = p.sunken;
    v.extreme_bg_color = p.surface;
    v.code_bg_color = p.sunken;
    v.override_text_color = None;
    v.hyperlink_color = p.text;
    v.selection.bg_fill = mix(p.surface, p.accent, 0.38);
    v.selection.stroke = Stroke::new(1.0_f32, p.text);
    v.text_cursor.stroke = Stroke::new(1.5_f32, p.text);

    let radius = CornerRadius::same(RADIUS_MD);

    let ni = &mut v.widgets.noninteractive;
    ni.bg_fill = p.surface;
    ni.weak_bg_fill = p.surface;
    ni.bg_stroke = Stroke::new(1.0_f32, p.border); // Trennlinien, Panel-Kanten
    ni.fg_stroke = Stroke::new(1.0_f32, p.text);
    ni.corner_radius = radius;

    let wi = &mut v.widgets.inactive;
    wi.bg_fill = p.surface;
    wi.weak_bg_fill = p.surface;
    wi.bg_stroke = Stroke::new(1.0_f32, p.border_input);
    wi.fg_stroke = Stroke::new(1.0_f32, p.text);
    wi.corner_radius = radius;
    wi.expansion = 0.0;

    let wh = &mut v.widgets.hovered;
    wh.bg_fill = p.hover;
    wh.weak_bg_fill = p.hover;
    wh.bg_stroke = Stroke::new(1.0_f32, p.border_input);
    wh.fg_stroke = Stroke::new(1.0_f32, p.text);
    wh.corner_radius = radius;
    wh.expansion = 0.0;

    let wa = &mut v.widgets.active;
    wa.bg_fill = p.sunken;
    wa.weak_bg_fill = p.sunken;
    wa.bg_stroke = Stroke::new(1.0_f32, p.text);
    wa.fg_stroke = Stroke::new(1.0_f32, p.text);
    wa.corner_radius = radius;
    wa.expansion = 0.0;

    let wo = &mut v.widgets.open;
    wo.bg_fill = p.surface;
    wo.weak_bg_fill = p.surface;
    wo.bg_stroke = Stroke::new(1.0_f32, p.text);
    wo.fg_stroke = Stroke::new(1.0_f32, p.text);
    wo.corner_radius = radius;
    wo.expansion = 0.0;

    let text_styles: std::collections::BTreeMap<TextStyle, FontId> = [
        (TextStyle::Heading, FontId::new(22.0, FontFamily::Name("Switzer SemiBold".into()))),
        (TextStyle::Body, FontId::new(15.0, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
    ]
    .into_iter()
    .collect();

    ctx.all_styles_mut(|style| {
        style.visuals = v.clone();
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(12.0, 5.0);
        style.spacing.interact_size = Vec2::new(40.0, 28.0);
        style.spacing.window_margin = Margin::same(8);
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.indent = 16.0;
        style.interaction.selectable_labels = false;
        style.text_styles = text_styles.clone();
    });
}

/// Karten-Rahmen (Surface, Rand, Radius 10, dezenter Schatten, Padding 16).
pub fn card(p: &Palette, mode: Mode) -> egui::Frame {
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(RADIUS_LG))
        .shadow(shadow_card(mode))
        .inner_margin(Margin::same(16))
}

pub fn shadow_dialog(mode: Mode) -> egui::Shadow {
    egui::Shadow {
        offset: [0, 12],
        blur: 32,
        spread: 0,
        color: match mode {
            Mode::Light => Color32::from_rgba_unmultiplied(18, 32, 46, 46),
            Mode::Dark => Color32::from_rgba_unmultiplied(0, 0, 0, 140),
        },
    }
}

fn shadow_popup(mode: Mode) -> egui::Shadow {
    egui::Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: match mode {
            Mode::Light => Color32::from_rgba_unmultiplied(18, 32, 46, 30),
            Mode::Dark => Color32::from_rgba_unmultiplied(0, 0, 0, 110),
        },
    }
}

fn shadow_card(mode: Mode) -> egui::Shadow {
    egui::Shadow {
        offset: [0, 1],
        blur: 2,
        spread: 0,
        color: match mode {
            Mode::Light => Color32::from_rgba_unmultiplied(18, 32, 46, 15),
            Mode::Dark => Color32::from_rgba_unmultiplied(0, 0, 0, 90),
        },
    }
}
