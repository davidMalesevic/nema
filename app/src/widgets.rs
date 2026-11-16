//! Wiederverwendbare Komponenten gemäss docs/UI Design/nema.css (egui-Übersetzung).
//! Alle Bedienelemente sind selbst gemalt, damit Höhen, Radien, Hover und
//! Fokus-Ring exakt den Tokens entsprechen.

use crate::theme::{self, mix, Mode, Palette, HOVER_SECONDS, RADIUS_LG, RADIUS_MD, RADIUS_SM};
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    pos2, vec2, Align, Color32, CornerRadius, CursorIcon, FontFamily, FontId, Layout, Margin, Painter, Pos2, Rect,
    Response, RichText, Sense, Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2, WidgetInfo, WidgetType,
};
use std::path::Path;

// ---------------------------------------------------------------- Fonts

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("Switzer SemiBold".into()))
}
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("Switzer Medium".into()))
}
pub fn body(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}

// ---------------------------------------------------------------- Icons

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Lock,
    Eye,
    EyeOff,
    Alert,
    AlertCircle,
    Info,
    Check,
    ArrowRight,
    ArrowLeft,
    Folder,
    File,
    Search,
    Plus,
    UserPlus,
}

/// Malt ein Line-Icon (16er-Raster) in `rect` mit Strichstärke `width`.
pub fn paint_icon(painter: &Painter, kind: Icon, rect: Rect, color: Color32, width: f32) {
    use std::f32::consts::PI;
    let s = rect.width() / 16.0;
    let o = rect.min;
    let pt = |x: f32, y: f32| pos2(o.x + x * s, o.y + y * s);
    let st = Stroke::new(width, color);
    let line = |pts: &[(f32, f32)]| {
        painter.add(Shape::line(pts.iter().map(|&(x, y)| pt(x, y)).collect(), st));
    };
    let closed = |pts: &[(f32, f32)]| {
        painter.add(Shape::closed_line(pts.iter().map(|&(x, y)| pt(x, y)).collect(), st));
    };
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| -> Vec<(f32, f32)> {
        let n = 16;
        (0..=n)
            .map(|i| {
                let a = from + (to - from) * (i as f32 / n as f32);
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };
    let dot = |x: f32, y: f32| {
        painter.circle_filled(pt(x, y), width * 0.75, color);
    };
    match kind {
        Icon::Lock => {
            painter.rect_stroke(Rect::from_min_max(pt(3.0, 7.2), pt(13.0, 14.0)), 1.6 * s, st, StrokeKind::Middle);
            let mut pts = vec![(5.3, 7.2), (5.3, 5.2)];
            pts.extend(arc(8.0, 5.2, 2.7, PI, 2.0 * PI));
            pts.push((10.7, 7.2));
            line(&pts);
        }
        Icon::Eye | Icon::EyeOff => {
            let n = 16;
            let mut pts: Vec<(f32, f32)> = (0..=n)
                .map(|i| {
                    let t = i as f32 / n as f32;
                    (1.5 + 13.0 * t, 8.0 - 4.4 * (PI * t).sin())
                })
                .collect();
            pts.extend((1..n).map(|i| {
                let t = i as f32 / n as f32;
                (14.5 - 13.0 * t, 8.0 + 4.4 * (PI * t).sin())
            }));
            closed(&pts);
            painter.circle_stroke(pt(8.0, 8.0), 2.0 * s, st);
            if kind == Icon::EyeOff {
                line(&[(2.8, 13.6), (13.2, 2.4)]);
            }
        }
        Icon::Alert => {
            closed(&[(8.0, 2.4), (14.4, 13.4), (1.6, 13.4)]);
            line(&[(8.0, 6.6), (8.0, 9.6)]);
            dot(8.0, 11.5);
        }
        Icon::AlertCircle => {
            painter.circle_stroke(pt(8.0, 8.0), 6.2 * s, st);
            line(&[(8.0, 4.9), (8.0, 8.6)]);
            dot(8.0, 11.0);
        }
        Icon::Info => {
            painter.circle_stroke(pt(8.0, 8.0), 6.2 * s, st);
            dot(8.0, 5.0);
            line(&[(8.0, 7.4), (8.0, 11.2)]);
        }
        Icon::Check => line(&[(3.0, 8.6), (6.4, 12.0), (13.0, 4.4)]),
        Icon::ArrowRight => {
            line(&[(2.8, 8.0), (13.0, 8.0)]);
            line(&[(9.2, 4.2), (13.0, 8.0), (9.2, 11.8)]);
        }
        Icon::ArrowLeft => {
            line(&[(13.2, 8.0), (3.0, 8.0)]);
            line(&[(6.8, 4.2), (3.0, 8.0), (6.8, 11.8)]);
        }
        Icon::Folder => closed(&[(1.8, 13.2), (1.8, 3.6), (6.2, 3.6), (7.8, 5.6), (14.2, 5.6), (14.2, 13.2)]),
        Icon::File => {
            closed(&[(3.6, 1.8), (9.6, 1.8), (12.6, 4.8), (12.6, 14.2), (3.6, 14.2)]);
            line(&[(9.6, 1.8), (9.6, 4.8), (12.6, 4.8)]);
        }
        Icon::Search => {
            painter.circle_stroke(pt(7.0, 7.0), 4.4 * s, st);
            line(&[(10.3, 10.3), (13.8, 13.8)]);
        }
        Icon::Plus => {
            line(&[(8.0, 3.2), (8.0, 12.8)]);
            line(&[(3.2, 8.0), (12.8, 8.0)]);
        }
        Icon::UserPlus => {
            painter.circle_stroke(pt(6.4, 5.0), 2.7 * s, st);
            line(&arc(6.4, 14.0, 4.9, PI, 2.0 * PI));
            line(&[(12.6, 4.0), (12.6, 8.4)]);
            line(&[(10.4, 6.2), (14.8, 6.2)]);
        }
    }
}

/// Icon als Widget (16×16, in einer 20 px hohen Zeile zentriert).
pub fn icon(ui: &mut Ui, kind: Icon, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(16.0, 20.0), Sense::hover());
    paint_icon(ui.painter(), kind, Rect::from_center_size(rect.center(), Vec2::splat(16.0)), color, 1.6);
    resp
}

// ---------------------------------------------------------------- Fokus

/// Fokus-Ring: 2 px orange mit 2 px Abstand (dazu wechselt der Rahmen auf `text`).
pub fn focus_ring(ui: &Ui, p: &Palette, rect: Rect, radius: u8) {
    ui.painter().rect_stroke(
        rect.expand(2.0),
        CornerRadius::same(radius.saturating_add(2)),
        Stroke::new(2.0_f32, p.accent),
        StrokeKind::Outside,
    );
}

// ---------------------------------------------------------------- Text-Helfer

pub fn h1(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(semibold(28.0)).color(p.text));
}
pub fn h2(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).font(semibold(22.0)).color(p.text)).truncate());
}
pub fn h3(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(semibold(17.0)).color(p.text));
}
/// Einleitungs-/Untertitel-Text (15 px, gedämpft, umbrechend).
pub fn lede(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).font(body(15.0)).color(p.muted)).wrap());
}
pub fn field_label(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).font(medium(13.0)).color(p.text)).truncate());
}
pub fn help(ui: &mut Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).font(body(12.0)).color(p.muted)).wrap());
}
pub fn muted(ui: &mut Ui, p: &Palette, text: impl Into<String>) {
    ui.add(egui::Label::new(RichText::new(text.into()).font(body(13.0)).color(p.muted)).wrap());
}

/// Fehlerzeile unter einem Feld: Icon + Text (13 px Medium, danger-text).
pub fn error_line(ui: &mut Ui, p: &Palette, text: &str) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        icon(ui, Icon::AlertCircle, p.danger_text);
        ui.add(egui::Label::new(RichText::new(text).font(medium(13.0)).color(p.danger_text)).wrap());
    });
}

/// Kürzt `text` in der Mitte mit «…», bis er in `max_w` passt.
pub fn elide_middle(painter: &Painter, text: &str, font: &FontId, max_w: f32) -> String {
    let width = |s: &str| painter.layout_no_wrap(s.to_owned(), font.clone(), Color32::PLACEHOLDER).size().x;
    let full = width(text);
    if full <= max_w {
        return text.to_owned();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut keep = ((chars.len() as f32) * max_w / full) as usize;
    loop {
        if keep < 4 {
            return "…".to_owned();
        }
        // mehr vom Ende zeigen: dort steht der Dateiname
        let head = keep * 2 / 5;
        let tail = keep - head;
        let s: String = chars[..head]
            .iter()
            .chain(std::iter::once(&'…'))
            .chain(chars[chars.len() - tail..].iter())
            .collect();
        if width(&s) <= max_w {
            return s;
        }
        keep -= 1;
    }
}

/// Pfad für die Anzeige: Home-Verzeichnis als «~».
pub fn display_path(path: &Path) -> String {
    let s = path.display().to_string();
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    if !home.is_empty() {
        if let Some(rest) = s.strip_prefix(&home) {
            if rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\') {
                return format!("~{rest}");
            }
        }
    }
    s
}

/// Zeile mit Icon + Mono-Pfad (13 px, gedämpft, mittig gekürzt).
pub fn path_line(ui: &mut Ui, p: &Palette, ic: Icon, text: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        icon(ui, ic, p.muted);
        let font = mono(13.0);
        let shown = elide_middle(ui.painter(), text, &font, ui.available_width());
        let resp = ui.label(RichText::new(&shown).font(font).color(p.muted));
        if shown != text {
            resp.on_hover_text(text);
        }
    });
}

const ARROW_SEP: &str = " → ";
const ARROW_W: f32 = 24.0;

fn arrow_parts(painter: &Painter, text: &str, font: &FontId, color: Color32) -> (Vec<std::sync::Arc<egui::Galley>>, Vec2) {
    let parts: Vec<_> = text.split(ARROW_SEP).map(|s| painter.layout_no_wrap(s.to_owned(), font.clone(), color)).collect();
    let h = parts.iter().map(|g| g.size().y).fold(0.0, f32::max);
    let w = parts.iter().map(|g| g.size().x).sum::<f32>() + (parts.len() as f32 - 1.0) * ARROW_W;
    (parts, vec2(w, h))
}

fn paint_arrow_parts(painter: &Painter, left_center: Pos2, parts: Vec<std::sync::Arc<egui::Galley>>, color: Color32) {
    let mut x = left_center.x;
    for (i, g) in parts.into_iter().enumerate() {
        if i > 0 {
            let r = Rect::from_center_size(pos2(x + ARROW_W / 2.0, left_center.y + 0.5), Vec2::splat(12.0));
            paint_icon(painter, Icon::ArrowRight, r, color, 1.4);
            x += ARROW_W;
        }
        let w = g.size().x;
        painter.galley(pos2(x, left_center.y - g.size().y / 2.0), g, color);
        x += w;
    }
}

/// Einzeiliger Text, in dem « → » als Icon gezeichnet wird
/// (die Pfeil-Glyphe der Fallback-Schrift sitzt zu eng und zu klein).
pub fn arrow_label(ui: &mut Ui, text: &str, font: FontId, color: Color32) {
    let (parts, size) = arrow_parts(ui.painter(), text, &font, color);
    let (rect, _) = ui.allocate_exact_size(vec2(size.x.min(ui.available_width()), size.y), Sense::hover());
    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
    paint_arrow_parts(&painter, rect.left_center(), parts, color);
}

// ---------------------------------------------------------------- Buttons

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    Primary,
    Secondary,
    Danger,
}

/// Button gemäss `.nm-btn`: 36 px (klein: 28 px), optional mit Icon, volle Breite oder deaktiviert.
pub struct Button<'a> {
    kind: Btn,
    label: &'a str,
    icon: Option<Icon>,
    small: bool,
    block: bool,
    enabled: bool,
}

impl<'a> Button<'a> {
    pub fn new(kind: Btn, label: &'a str) -> Self {
        Self { kind, label, icon: None, small: false, block: false, enabled: true }
    }
    pub fn primary(label: &'a str) -> Self {
        Self::new(Btn::Primary, label)
    }
    pub fn secondary(label: &'a str) -> Self {
        Self::new(Btn::Secondary, label)
    }
    pub fn danger(label: &'a str) -> Self {
        Self::new(Btn::Danger, label)
    }
    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }
    pub fn small(mut self) -> Self {
        self.small = true;
        self
    }
    pub fn block(mut self) -> Self {
        self.block = true;
        self
    }
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn show(self, ui: &mut Ui, p: &Palette) -> Response {
        let Self { kind, label, icon, small, block, enabled } = self;
        let (h, pad, font, radius) = if small {
            (28.0, 12.0, medium(14.0), RADIUS_SM)
        } else {
            (36.0, 16.0, semibold(15.0), RADIUS_MD)
        };
        let (fill, fg, border, hover_fill) = match (kind, enabled) {
            (_, false) => (p.sunken, p.muted, p.border, p.sunken),
            (Btn::Primary, true) => (p.primary, p.on_primary, p.primary, mix(p.primary, p.surface, 0.12)),
            (Btn::Secondary, true) => (p.surface, p.text, p.border_input, p.hover),
            (Btn::Danger, true) => (p.surface, p.danger_text, p.danger, p.danger_bg),
        };
        let galley = ui.painter().layout_no_wrap(label.to_owned(), font, fg);
        let icon_w = if icon.is_some() { 16.0 + 8.0 } else { 0.0 };
        let content_w = icon_w + galley.size().x;
        let w = if block { ui.available_width() } else { content_w + 2.0 * pad };
        let sense = if enabled { Sense::click() } else { Sense::hover() };
        let (rect, resp) = ui.allocate_exact_size(vec2(w, h), sense);
        resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, label));

        if ui.is_rect_visible(rect) {
            let t = ui.ctx().animate_bool_with_time(resp.id.with("hover"), enabled && resp.hovered(), HOVER_SECONDS);
            let focused = resp.has_focus();
            let stroke = Stroke::new(1.0_f32, if focused { p.text } else { border });
            ui.painter().rect(rect, CornerRadius::same(radius), mix(fill, hover_fill, t), stroke, StrokeKind::Inside);
            let mut x = (rect.center().x - content_w / 2.0).max(rect.left() + 4.0);
            if let Some(ic) = icon {
                let r = Rect::from_min_size(pos2(x, rect.center().y - 8.0), Vec2::splat(16.0));
                paint_icon(ui.painter(), ic, r, fg, 1.6);
                x += 24.0;
            }
            let text_pos = pos2(x, rect.center().y - galley.size().y / 2.0);
            ui.painter().with_clip_rect(rect.intersect(ui.clip_rect())).galley(text_pos, galley, fg);
            if focused {
                focus_ring(ui, p, rect, radius);
            }
        }
        if enabled {
            resp.on_hover_cursor(CursorIcon::PointingHand)
        } else {
            resp
        }
    }
}

/// Link-artiger Danger-Button in Tabellen («löschen»), 20 px hoch.
pub fn link_button(ui: &mut Ui, p: &Palette, label: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_owned(), medium(14.0), p.danger_text);
    // Höhe = Textzeile, damit die Grundlinie zu den Nachbarzellen passt.
    let (rect, resp) = ui.allocate_exact_size(galley.size(), Sense::click());
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    let t = ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), HOVER_SECONDS);
    let bg = rect.expand2(vec2(6.0, 3.0));
    if t > 0.0 {
        ui.painter().rect_filled(bg, CornerRadius::same(RADIUS_SM), p.danger_bg.gamma_multiply(t));
    }
    ui.painter().galley(pos2(rect.left(), rect.center().y - galley.size().y / 2.0), galley, p.danger_text);
    if resp.has_focus() {
        focus_ring(ui, p, bg, RADIUS_SM);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

// ---------------------------------------------------------------- Felder

fn field_impl(ui: &mut Ui, p: &Palette, value: &mut String, hint: &str, error: bool, visible: Option<&mut bool>) -> Response {
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
    let bg_idx = ui.painter().add(Shape::Noop);
    let eye_w = if visible.is_some() { 32.0 } else { 0.0 };
    let masked = visible.as_deref().map(|v| !*v).unwrap_or(false);

    let edit_rect = Rect::from_min_max(rect.min, pos2(rect.right() - eye_w, rect.bottom()));
    let mut child = ui.new_child(UiBuilder::new().max_rect(edit_rect).layout(Layout::left_to_right(Align::Center)));
    let resp = child.add(
        egui::TextEdit::singleline(value)
            .frame(false)
            .margin(Margin::symmetric(12, 0))
            .min_size(edit_rect.size())
            .desired_width(f32::INFINITY)
            .vertical_align(Align::Center)
            .font(body(15.0))
            .text_color(p.text)
            .password(masked)
            .hint_text(RichText::new(hint).font(body(15.0)).color(p.muted)),
    );

    let focused = resp.has_focus();
    let stroke = if error {
        Stroke::new(2.0_f32, p.danger)
    } else if focused {
        Stroke::new(1.0_f32, p.text)
    } else {
        Stroke::new(1.0_f32, p.border_input)
    };
    ui.painter().set(
        bg_idx,
        egui::epaint::RectShape::new(rect, CornerRadius::same(RADIUS_MD), p.surface, stroke, StrokeKind::Inside),
    );
    if focused {
        focus_ring(ui, p, rect, RADIUS_MD);
    }

    if let Some(visible) = visible {
        let eye_rect = Rect::from_min_size(pos2(rect.right() - 32.0, rect.top() + 4.0), Vec2::splat(28.0));
        let eye = ui.interact(eye_rect, resp.id.with("eye"), Sense::click());
        let label = if *visible { "Passwort verbergen" } else { "Passwort anzeigen" };
        eye.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
        if eye.hovered() {
            ui.painter().rect_filled(eye_rect, CornerRadius::same(RADIUS_SM), p.hover);
        }
        let ic = if *visible { Icon::EyeOff } else { Icon::Eye };
        paint_icon(ui.painter(), ic, Rect::from_center_size(eye_rect.center(), Vec2::splat(16.0)), p.muted, 1.5);
        if eye.has_focus() {
            focus_ring(ui, p, eye_rect, RADIUS_SM);
        }
        if eye.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(label).clicked() {
            *visible = !*visible;
        }
    }
    resp
}

/// Textfeld 36 px, volle Breite. `error` malt einen 2-px-danger-Rahmen.
pub fn text_field(ui: &mut Ui, p: &Palette, value: &mut String, hint: &str, error: bool) -> Response {
    field_impl(ui, p, value, hint, error, None)
}

/// Passwortfeld mit Sichtbarkeits-Toggle (Auge im Feld rechts).
pub fn password_field(ui: &mut Ui, p: &Palette, value: &mut String, hint: &str, error: bool, visible: &mut bool) -> Response {
    field_impl(ui, p, value, hint, error, Some(visible))
}

/// Checkbox 16 px (in 20-px-Zeile), `label` nur für Screenreader.
pub fn checkbox(ui: &mut Ui, p: &Palette, checked: &mut bool, label: &str) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
    if resp.clicked() {
        *checked = !*checked;
        resp.mark_changed();
    }
    let on = *checked;
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, on, label));
    let b = Rect::from_center_size(rect.center(), Vec2::splat(16.0));
    let r = CornerRadius::same(RADIUS_SM);
    if on {
        ui.painter().rect_filled(b, r, p.primary);
        paint_icon(ui.painter(), Icon::Check, b.shrink(1.5), p.on_primary, 1.8);
    } else {
        let fill = if resp.hovered() { p.hover } else { p.surface };
        ui.painter().rect(b, r, fill, Stroke::new(1.0_f32, p.border_input), StrokeKind::Inside);
    }
    if resp.has_focus() {
        focus_ring(ui, p, b, RADIUS_SM);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

// ---------------------------------------------------------------- Tabs, Badge, Pill

/// Tab (44 px hoch): aktiv SemiBold mit 3-px-Unterstreichung in Orange, optional Badge.
pub fn tab(ui: &mut Ui, p: &Palette, selected: bool, label: &str, badge: Option<usize>) -> Response {
    // Breite immer mit SemiBold messen, damit die Leiste beim Wechsel nicht springt.
    let wide = ui.painter().layout_no_wrap(label.to_owned(), semibold(15.0), p.text);
    let badge_galley = badge.map(|n| ui.painter().layout_no_wrap(n.to_string(), semibold(12.0), p.on_accent));
    let badge_w = badge_galley.as_ref().map(|g| (g.size().x + 12.0).max(20.0));
    let w = wide.size().x + badge_w.map_or(0.0, |b| 8.0 + b);
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 44.0), Sense::click());
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, selected, label));

    let t = ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), HOVER_SECONDS);
    let galley = if selected {
        wide
    } else {
        ui.painter().layout_no_wrap(label.to_owned(), medium(15.0), mix(p.muted, p.text, t))
    };
    let color = if selected { p.text } else { mix(p.muted, p.text, t) };
    ui.painter().galley(pos2(rect.left(), rect.center().y - galley.size().y / 2.0), galley, color);

    if let (Some(g), Some(bw)) = (badge_galley, badge_w) {
        let b = Rect::from_min_size(pos2(rect.right() - bw, rect.center().y - 10.0), vec2(bw, 20.0));
        ui.painter().rect_filled(b, CornerRadius::same(10), p.accent);
        ui.painter().galley(b.center() - g.size() / 2.0, g, p.on_accent);
    }
    if selected {
        let bar = Rect::from_min_max(pos2(rect.left() - 4.0, rect.bottom() - 3.0), pos2(rect.right() + 4.0, rect.bottom()));
        ui.painter().rect_filled(bar, CornerRadius::ZERO, p.accent);
    }
    if resp.has_focus() {
        focus_ring(ui, p, rect.shrink2(vec2(-6.0, 8.0)), RADIUS_SM);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Tresor-Status-Pill: Schloss + «Lokal verschlüsselt · Modus».
pub fn vault_pill(ui: &mut Ui, p: &Palette, mode_label: &str) {
    let text = format!("Lokal verschlüsselt · {mode_label}");
    let galley = ui.painter().layout_no_wrap(text.clone(), medium(13.0), p.text);
    let w = 12.0 + 16.0 + 6.0 + galley.size().x + 12.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(14), p.sunken, Stroke::new(1.0_f32, p.border), StrokeKind::Inside);
    let ic = Rect::from_min_size(pos2(rect.left() + 12.0, rect.center().y - 8.0), Vec2::splat(16.0));
    paint_icon(ui.painter(), Icon::Lock, ic, p.success_text, 1.6);
    ui.painter().galley(pos2(ic.right() + 6.0, rect.center().y - galley.size().y / 2.0), galley, p.text);
    resp.on_hover_text("Alle Namen bleiben lokal und verschlüsselt auf diesem Gerät.");
}

/// Kleines Tag («Empfohlen») bzw. Richtungs-Pill im Protokoll.
pub fn tag(ui: &mut Ui, p: &Palette, ic: Option<Icon>, text: &str) {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), medium(12.0), p.text);
    let icon_w = if ic.is_some() { 12.0 + 4.0 } else { 0.0 };
    let (rect, _) = ui.allocate_exact_size(vec2(8.0 + icon_w + galley.size().x + 8.0, 20.0), Sense::hover());
    ui.painter().rect(rect, CornerRadius::same(10), p.sunken, Stroke::new(1.0_f32, p.border), StrokeKind::Inside);
    let mut x = rect.left() + 8.0;
    if let Some(ic) = ic {
        paint_icon(ui.painter(), ic, Rect::from_min_size(pos2(x, rect.center().y - 6.0), Vec2::splat(12.0)), p.text, 1.4);
        x += 16.0;
    }
    ui.painter().galley(pos2(x, rect.center().y - galley.size().y / 2.0), galley, p.text);
}

// ---------------------------------------------------------------- Karten

/// Nummerierte Sektions-Karte. `min_inner_h` gleicht die Höhe an die Nachbarkarte an;
/// Rückgabe ist die natürliche Inhaltshöhe (für die nächste Runde).
#[allow(clippy::too_many_arguments)]
pub fn section_card(
    ui: &mut Ui,
    p: &Palette,
    mode: Mode,
    num: u8,
    title: &str,
    sub: &str,
    min_inner_h: f32,
    add_contents: impl FnOnce(&mut Ui),
) -> f32 {
    theme::card(p, mode)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let top = ui.cursor().top();
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                let (rect, _) = ui.allocate_exact_size(vec2(24.0, 42.0), Sense::hover());
                let c = pos2(rect.center().x, rect.top() + 14.0);
                ui.painter().circle_filled(c, 12.0, p.primary);
                let g = ui.painter().layout_no_wrap(num.to_string(), semibold(13.0), p.on_primary);
                ui.painter().galley(c - g.size() / 2.0, g, p.on_primary);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add(egui::Label::new(RichText::new(title).font(semibold(17.0)).color(p.text)).truncate());
                    arrow_label(ui, sub, body(13.0), p.muted);
                });
            });
            ui.add_space(4.0);
            add_contents(ui);
            let natural = ui.min_rect().bottom() - top;
            if min_inner_h > natural {
                let left = ui.min_rect().left();
                ui.expand_to_include_rect(Rect::from_min_max(pos2(left, top), pos2(left, top + min_inner_h)));
            }
            natural
        })
        .inner
}

// ---------------------------------------------------------------- Tabelle

#[derive(Clone, Copy)]
enum ColW {
    Fixed(f32),
    Flex(f32),
}

/// Spalte der Tabelle: feste Breite oder Anteil am Rest.
#[derive(Clone, Copy)]
pub struct Col<'a> {
    header: &'a str,
    w: ColW,
    right: bool,
}

impl<'a> Col<'a> {
    pub fn fixed(header: &'a str, width: f32) -> Self {
        Self { header, w: ColW::Fixed(width), right: false }
    }
    pub fn flex(header: &'a str, weight: f32) -> Self {
        Self { header, w: ColW::Flex(weight), right: false }
    }
    pub fn right(mut self) -> Self {
        self.right = true;
        self
    }
}

/// Gestreifte Tabelle gemäss `.nm-table`: Kopf auf `surface-sunken`, Zeilen-Trenner,
/// Hover-Highlight, Zeilenhöhe folgt dem Inhalt. `cell(ui, zeile, spalte)` füllt die Zellen.
pub fn table(ui: &mut Ui, p: &Palette, cols: &[Col<'_>], n_rows: usize, mut cell: impl FnMut(&mut Ui, usize, usize)) {
    const PAD: Vec2 = vec2(12.0, 10.0);
    const HEADER_H: f32 = 38.0;
    const LINE_H: f32 = 20.0;

    let total_w = ui.available_width();
    let origin = ui.cursor().min;
    let fixed: f32 = cols.iter().map(|c| if let ColW::Fixed(w) = c.w { w } else { 0.0 }).sum();
    let flex: f32 = cols.iter().map(|c| if let ColW::Flex(w) = c.w { w } else { 0.0 }).sum();
    let rest = (total_w - fixed).max(0.0);
    let widths: Vec<f32> = cols
        .iter()
        .map(|c| match c.w {
            ColW::Fixed(w) => w,
            ColW::Flex(w) => rest * w / flex.max(0.001),
        })
        .collect();

    let painter = ui.painter().clone();
    let lg = RADIUS_LG;
    let line = Stroke::new(1.0_f32, p.border);

    // Kopf
    let header = Rect::from_min_size(origin, vec2(total_w, HEADER_H));
    painter.rect_filled(header, CornerRadius { nw: lg, ne: lg, sw: 0, se: 0 }, p.sunken);
    let mut x = origin.x;
    for (c, col) in cols.iter().enumerate() {
        let col_rect = Rect::from_min_size(pos2(x, origin.y), vec2(widths[c], HEADER_H));
        if col.header.contains(ARROW_SEP) {
            let (parts, _) = arrow_parts(&painter, col.header, &semibold(14.0), p.text);
            let clipped = painter.with_clip_rect(col_rect.shrink2(vec2(PAD.x, 0.0)).intersect(ui.clip_rect()));
            paint_arrow_parts(&clipped, pos2(col_rect.left() + PAD.x, col_rect.center().y), parts, p.text);
        } else {
            let mut job = LayoutJob::simple_singleline(col.header.to_owned(), semibold(14.0), p.text);
            job.wrap = TextWrapping::truncate_at_width((widths[c] - 2.0 * PAD.x).max(0.0));
            let g = painter.layout_job(job);
            let gx = if col.right { col_rect.right() - PAD.x - g.size().x } else { col_rect.left() + PAD.x };
            painter.galley(pos2(gx, col_rect.center().y - g.size().y / 2.0), g, p.text);
        }
        x += widths[c];
    }

    // Zeilen
    let mut y = header.bottom();
    for r in 0..n_rows {
        let bg_idx = painter.add(Shape::Noop);
        let mut bottom = y + LINE_H + 2.0 * PAD.y;
        let mut x = origin.x;
        for (c, col) in cols.iter().enumerate() {
            let content = Rect::from_min_max(pos2(x + PAD.x, y + PAD.y), pos2(x + widths[c] - PAD.x, y + PAD.y + 4000.0));
            let layout = if col.right { Layout::top_down(Align::Max) } else { Layout::top_down(Align::Min) };
            let mut child = ui.new_child(UiBuilder::new().id_salt(("cell", r, c)).max_rect(content).layout(layout));
            let clip = Rect::from_min_max(pos2(x, y), pos2(x + widths[c], f32::INFINITY));
            child.set_clip_rect(clip.intersect(ui.clip_rect()));
            child.spacing_mut().item_spacing.y = 2.0;
            cell(&mut child, r, c);
            bottom = bottom.max(child.min_rect().bottom() + PAD.y);
            x += widths[c];
        }
        let row = Rect::from_min_max(pos2(origin.x, y), pos2(origin.x + total_w, bottom));
        let fill = if ui.rect_contains_pointer(row) {
            p.hover
        } else if r % 2 == 1 {
            p.sunken
        } else {
            p.surface
        };
        let cr = if r + 1 == n_rows { CornerRadius { nw: 0, ne: 0, sw: lg, se: lg } } else { CornerRadius::ZERO };
        painter.set(bg_idx, egui::epaint::RectShape::filled(row, cr, fill));
        painter.hline(row.x_range(), y, line);
        y = bottom;
    }

    let full = Rect::from_min_max(origin, pos2(origin.x + total_w, y));
    painter.rect_stroke(full, CornerRadius::same(lg), line, StrokeKind::Inside);
    ui.allocate_rect(full, Sense::hover());
}

/// Zellentext (14 px), einzeilig gekürzt.
pub fn cell(ui: &mut Ui, p: &Palette, text: impl Into<String>) {
    ui.add(egui::Label::new(RichText::new(text.into()).font(body(14.0)).color(p.text)).truncate());
}
/// Zellentext (14 px), umbrechend.
pub fn cell_wrap(ui: &mut Ui, p: &Palette, text: impl Into<String>) {
    ui.add(egui::Label::new(RichText::new(text.into()).font(body(14.0)).color(p.text)).wrap());
}
/// Zellentext in Mono (13 px), umbrechend. 1 px tiefer: gleiche Grundlinie wie der 14-px-Text.
pub fn cell_mono(ui: &mut Ui, p: &Palette, text: impl Into<String>) {
    ui.add_space(1.0);
    ui.add(egui::Label::new(RichText::new(text.into()).font(mono(13.0)).color(p.text)).wrap());
}
/// Zellentext in Mono (13 px), einzeilig gekürzt.
pub fn cell_mono_line(ui: &mut Ui, p: &Palette, text: impl Into<String>) {
    ui.add_space(1.0);
    ui.add(egui::Label::new(RichText::new(text.into()).font(mono(13.0)).color(p.text)).truncate());
}

// ---------------------------------------------------------------- Status & Meldungen

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Neutral,
    Success,
    Warning,
    Error,
}

/// Statusleiste unten (32 px, immer Icon + Wort). Vor Seitenleiste und Inhalt aufrufen.
pub fn status_bar(ctx: &egui::Context, p: &Palette, kind: Status, text: &str) {
    let (bg, fg, ic) = match kind {
        Status::Neutral => (p.sunken, p.text, Icon::Info),
        Status::Success => (p.success_bg, p.success_text, Icon::Check),
        Status::Warning => (p.warning_bg, p.warning_text, Icon::Alert),
        Status::Error => (p.danger_bg, p.danger_text, Icon::AlertCircle),
    };
    egui::TopBottomPanel::bottom("nema-status")
        .exact_height(32.0)
        .frame(egui::Frame::new().fill(bg).inner_margin(Margin::symmetric(16, 0)))
        .show(ctx, |ui| {
            ui.horizontal_centered(|ui| {
                icon(ui, ic, fg);
                ui.add(egui::Label::new(RichText::new(text).font(body(13.0)).color(fg)).truncate());
            });
        });
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Msg {
    Warning,
    Error,
    Success,
}

/// Meldungs-Box (Icon + Inhalt, farbiger Hintergrund + Rahmen). Der Inhalt bekommt die Textfarbe.
pub fn message_with(ui: &mut Ui, p: &Palette, kind: Msg, add_contents: impl FnOnce(&mut Ui, Color32)) {
    let (bg, fg, ic, edge) = match kind {
        Msg::Warning => (p.warning_bg, p.warning_text, Icon::Alert, p.warning),
        Msg::Error => (p.danger_bg, p.danger_text, Icon::AlertCircle, p.danger),
        Msg::Success => (p.success_bg, p.success_text, Icon::Check, p.success),
    };
    egui::Frame::new()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, edge))
        .corner_radius(CornerRadius::same(RADIUS_LG))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_top(|ui| {
                icon(ui, ic, fg);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    add_contents(ui, fg);
                });
            });
        });
}

pub fn message(ui: &mut Ui, p: &Palette, kind: Msg, text: &str) {
    message_with(ui, p, kind, |ui, fg| {
        ui.add(egui::Label::new(RichText::new(text).font(body(14.0)).color(fg)).wrap());
    });
}

/// Leer-Zustand: gestrichelter Rahmen, Icon, zentrierter Text.
pub fn empty_state(ui: &mut Ui, p: &Palette, ic: Icon, text: &str) {
    let w = ui.available_width();
    let mut job = LayoutJob::simple(text.to_owned(), body(15.0), p.muted, (w - 48.0).clamp(80.0, 360.0));
    job.halign = Align::Center;
    let galley = ui.painter().layout_job(job);
    let h = 48.0 + 32.0 + 12.0 + galley.size().y + 48.0;
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(RADIUS_LG), p.surface);
    let mut pts = rounded_rect_points(rect.shrink(0.5), RADIUS_LG as f32);
    pts.push(pts[0]);
    painter.extend(Shape::dashed_line(&pts, Stroke::new(1.0_f32, p.border_input), 4.0, 4.0));
    let ic_rect = Rect::from_min_size(pos2(rect.center().x - 16.0, rect.top() + 48.0), Vec2::splat(32.0));
    paint_icon(painter, ic, ic_rect, p.muted, 1.75);
    painter.galley(pos2(rect.center().x, ic_rect.bottom() + 12.0), galley, p.muted);
}

fn rounded_rect_points(rect: Rect, r: f32) -> Vec<Pos2> {
    use std::f32::consts::FRAC_PI_2;
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -FRAC_PI_2),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), FRAC_PI_2),
        (pos2(rect.left() + r, rect.top() + r), 2.0 * FRAC_PI_2),
    ];
    let mut pts = Vec::with_capacity(4 * 7);
    for (c, start) in corners {
        for i in 0..=6 {
            let a = start + FRAC_PI_2 * (i as f32 / 6.0);
            pts.push(pos2(c.x + r * a.cos(), c.y + r * a.sin()));
        }
    }
    pts
}

/// Trenner mit Text in der Mitte («oder Backup-Passwort»).
pub fn divider(ui: &mut Ui, p: &Palette, text: &str) {
    let galley = ui.painter().layout_no_wrap(text.to_owned(), body(13.0), p.muted);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    let half = galley.size().x / 2.0 + 12.0;
    let line = Stroke::new(1.0_f32, p.border);
    let y = rect.center().y;
    ui.painter().hline(rect.left()..=(rect.center().x - half), y, line);
    ui.painter().hline((rect.center().x + half)..=rect.right(), y, line);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, p.muted);
}

// ---------------------------------------------------------------- Seitenleiste

/// Projekt-Eintrag (aktiv: `surface-sunken`, SemiBold, oranger Balken links).
pub fn project_item(ui: &mut Ui, p: &Palette, name: &str, selected: bool) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 38.0), Sense::click());
    resp.widget_info(|| WidgetInfo::selected(WidgetType::Button, true, selected, name));
    let r = CornerRadius::same(RADIUS_MD);
    if selected {
        ui.painter().rect_filled(rect, r, p.sunken);
        let bar = Rect::from_min_size(rect.min, vec2(3.0, rect.height()));
        ui.painter().rect_filled(bar, CornerRadius { nw: 2, sw: 2, ne: 0, se: 0 }, p.accent);
    } else {
        let t = ui.ctx().animate_bool_with_time(resp.id.with("hover"), resp.hovered(), HOVER_SECONDS);
        if t > 0.0 {
            ui.painter().rect_filled(rect, r, p.hover.gamma_multiply(t));
        }
    }
    let font = if selected { semibold(15.0) } else { body(15.0) };
    let mut job = LayoutJob::simple_singleline(name.to_owned(), font, p.text);
    job.wrap = TextWrapping::truncate_at_width(w - 24.0);
    let g = ui.painter().layout_job(job);
    ui.painter().galley(pos2(rect.left() + 12.0, rect.center().y - g.size().y / 2.0), g, p.text);
    if resp.has_focus() {
        focus_ring(ui, p, rect.shrink(2.0), RADIUS_MD);
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

// ---------------------------------------------------------------- Dialog

/// Bestätigungs-Dialog über Scrim (420 px). `focus_cancel`: beim Öffnen «Abbrechen» fokussieren.
/// Rückgabe: Some(true) = bestätigt, Some(false) = abgebrochen/Esc, None = noch offen.
pub fn confirm_dialog(
    ctx: &egui::Context,
    p: &Palette,
    mode: Mode,
    title: &str,
    text: &str,
    confirm_label: &str,
    focus_cancel: bool,
) -> Option<bool> {
    let frame = egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(RADIUS_LG))
        .shadow(theme::shadow_dialog(mode))
        .inner_margin(Margin::same(24));
    let modal = egui::Modal::new(egui::Id::new("nema-confirm"))
        .backdrop_color(p.scrim)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(420.0 - 2.0 * 25.0);
            let mut result = None;
            ui.label(RichText::new(title).font(semibold(22.0)).color(p.text));
            ui.add(egui::Label::new(RichText::new(text).font(body(15.0)).color(p.muted)).wrap());
            ui.add_space(16.0);
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 36.0), Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if Button::danger(confirm_label).show(ui, p).clicked() {
                    result = Some(true);
                }
                let cancel = Button::secondary("Abbrechen").show(ui, p);
                if focus_cancel {
                    cancel.request_focus();
                }
                if cancel.clicked() {
                    result = Some(false);
                }
            });
            result
        });
    if modal.inner.is_some() {
        modal.inner
    } else if modal.should_close() {
        Some(false)
    } else {
        None
    }
}
