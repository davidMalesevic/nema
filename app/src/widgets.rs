//! Wiederverwendbare Komponenten gemäss docs/UI Design/nema.css (egui-Übersetzung).

use crate::theme::{self, Mode, Palette};
use egui::{Align, Align2, Color32, CornerRadius, FontFamily, FontId, Key, Layout, Margin, Order, Pos2, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::path::PathBuf;

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
#[allow(dead_code)]
pub enum Icon {
    Lock,
    Eye,
    EyeOff,
    Alert,
    Info,
    Check,
    ArrowRight,
    Trash,
    Folder,
    File,
    Search,
    Plus,
    X,
}

/// Malt ein Stroke-Icon (16er-Box, stroke 1.6) in `rect`.
pub fn paint_icon(painter: &egui::Painter, kind: Icon, rect: egui::Rect, color: Color32) {
    let s = rect.width() / 16.0;
    let ox = rect.left();
    let oy = rect.top();
    let pt = |x: f32, y: f32| Pos2::new(ox + x * s, oy + y * s);
    let st = Stroke::new(1.6 * s, color);
    let line = |a: (f32, f32), b: (f32, f32)| painter.line_segment([pt(a.0, a.1), pt(b.0, b.1)], st);
    let poly = |pts: &[(f32, f32)]| {
        let v: Vec<Pos2> = pts.iter().map(|p| pt(p.0, p.1)).collect();
        painter.add(egui::Shape::line(v, st));
    };
    let circle = |cx: f32, cy: f32, r: f32, filled: bool| {
        let c = pt(cx, cy);
        if filled {
            painter.circle_filled(c, r * s, color);
        } else {
            painter.circle_stroke(c, r * s, st);
        }
    };
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| {
        let n = 24;
        let v: Vec<Pos2> = (0..=n)
            .map(|i| {
                let a = from + (to - from) * (i as f32 / n as f32);
                pt(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect();
        painter.add(egui::Shape::line(v, st));
    };
    match kind {
        Icon::Lock => {
            painter.rect_stroke(egui::Rect::from_min_size(pt(3.5, 7.0), Vec2::splat(9.0 * s)), 1.5 * s, st, StrokeKind::Inside);
            arc(8.0, 7.0, 2.8, std::f32::consts::PI, 2.0 * std::f32::consts::PI);
            circle(8.0, 10.0, 1.1, true);
        }
        Icon::Eye => {
            let n = 32;
            let v: Vec<Pos2> = (0..=n)
                .map(|i| {
                    let a = 2.0 * std::f32::consts::PI * (i as f32 / n as f32);
                    pt(8.0 + 6.0 * a.cos(), 8.0 + 3.6 * a.sin())
                })
                .collect();
            painter.add(egui::Shape::line(v, st));
            circle(8.0, 8.0, 1.7, true);
        }
        Icon::EyeOff => {
            paint_icon(painter, Icon::Eye, rect, color);
            line((3.0, 13.5), (13.0, 2.5));
        }
        Icon::Alert => {
            poly(&[(8.0, 2.8), (14.0, 13.0), (2.0, 13.0), (8.0, 2.8)]);
            line((8.0, 6.8), (8.0, 10.2));
            circle(8.0, 11.6, 0.9, true);
        }
        Icon::Info => {
            circle(8.0, 8.0, 6.0, false);
            line((8.0, 7.2), (8.0, 11.2));
            circle(8.0, 4.6, 0.9, true);
        }
        Icon::Check => poly(&[(3.2, 8.6), (6.6, 12.0), (12.8, 4.2)]),
        Icon::ArrowRight => {
            line((3.0, 8.0), (13.0, 8.0));
            poly(&[(9.0, 4.0), (13.0, 8.0), (9.0, 12.0)]);
        }
        Icon::Trash => {
            line((3.0, 4.5), (13.0, 4.5));
            line((6.5, 2.8), (9.5, 2.8));
            poly(&[(5.0, 4.5), (6.0, 13.2), (10.0, 13.2), (11.0, 4.5)]);
            line((7.0, 7.5), (7.0, 10.8));
            line((9.0, 7.5), (9.0, 10.8));
        }
        Icon::Folder => poly(&[(2.0, 13.5), (2.0, 4.5), (6.5, 4.5), (8.2, 6.5), (14.0, 6.5), (14.0, 13.5), (2.0, 13.5)]),
        Icon::File => {
            poly(&[(4.0, 2.0), (10.0, 2.0), (13.0, 5.0), (13.0, 14.0), (4.0, 14.0), (4.0, 2.0)]);
            poly(&[(10.0, 2.0), (10.0, 5.0), (13.0, 5.0)]);
        }
        Icon::Search => {
            circle(7.0, 7.0, 4.2, false);
            line((10.2, 10.2), (13.5, 13.5));
        }
        Icon::Plus => {
            line((8.0, 3.5), (8.0, 12.5));
            line((3.5, 8.0), (12.5, 8.0));
        }
        Icon::X => {
            line((4.5, 4.5), (11.5, 11.5));
            line((11.5, 4.5), (4.5, 11.5));
        }
    }
}

/// Icon als Widget (16×16).
pub fn icon(ui: &mut Ui, kind: Icon, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
    paint_icon(ui.painter(), kind, rect, color);
    resp
}

/// Klickbarer Icon-Button (28×28, Hover-Kreis).
pub fn icon_button(ui: &mut Ui, p: &Palette, kind: Icon, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    if resp.hovered() {
        ui.painter().circle_filled(rect.center(), 14.0, p.hover);
    }
    paint_icon(ui.painter(), kind, egui::Rect::from_center_size(rect.center(), Vec2::splat(16.0)), color);
    theme::focus_ring(ui, &resp, p.accent);
    resp
}

// ---------------------------------------------------------------- Typo-Helfer

pub fn h1(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(semibold(28.0)).color(p.text));
}
pub fn h2(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(semibold(22.0)).color(p.text));
}
pub fn lede(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(body(15.0)).color(p.muted));
}
pub fn field_label(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(medium(13.0)).color(p.text));
}
pub fn help(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(RichText::new(text).font(body(12.0)).color(p.muted));
}
pub fn muted(ui: &mut Ui, p: &Palette, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).font(body(13.0)).color(p.muted));
}

/// Fehlerzeile mit Icon (13 px, danger-text).
pub fn error_line(ui: &mut Ui, p: &Palette, text: &str) {
    ui.horizontal(|ui| {
        icon(ui, Icon::Alert, p.danger_text);
        ui.label(RichText::new(text).font(medium(13.0)).color(p.danger_text));
    });
}

// ---------------------------------------------------------------- Buttons

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Btn {
    Primary,
    Secondary,
    Danger,
}

fn btn_style(kind: Btn, p: &Palette) -> (Color32, Color32, Stroke) {
    match kind {
        Btn::Primary => (p.primary, p.on_primary, Stroke::NONE),
        Btn::Secondary => (p.surface, p.text, Stroke::new(1.0_f32, p.border_input)),
        Btn::Danger => (p.surface, p.danger_text, Stroke::new(1.0_f32, p.danger)),
    }
}

fn button_sized(ui: &mut Ui, p: &Palette, kind: Btn, label: &str, min_size: Vec2, size: f32) -> Response {
    let (mut fill, mut txt, stroke) = btn_style(kind, p);
    let radius: u8 = if size <= 14.0 { 4 } else { 6 };
    if !ui.is_enabled() {
        fill = p.sunken;
        txt = p.muted;
    }
    let font = if size <= 14.0 { medium(size) } else { semibold(size) };
    let btn = egui::Button::new(RichText::new(label).font(font).color(txt))
        .fill(fill)
        .stroke(stroke)
        .corner_radius(CornerRadius::same(radius))
        .min_size(min_size);
    let resp = ui.add(btn);
    theme::focus_ring(ui, &resp, p.accent);
    resp
}

/// Button 36 px hoch, 15 px SemiBold.
pub fn button(ui: &mut Ui, p: &Palette, kind: Btn, label: &str) -> Response {
    button_sized(ui, p, kind, label, Vec2::new(0.0, 36.0), 15.0)
}

/// Button vollbreit (Block).
pub fn block_button(ui: &mut Ui, p: &Palette, kind: Btn, label: &str) -> Response {
    let w = ui.available_width();
    button_sized(ui, p, kind, label, Vec2::new(w, 36.0), 15.0)
}

/// Kleiner Button 28 px hoch, 14 px Medium.
pub fn small_button(ui: &mut Ui, p: &Palette, kind: Btn, label: &str) -> Response {
    button_sized(ui, p, kind, label, Vec2::new(0.0, 28.0), 14.0)
}

/// Link-artiger Danger-Button (löschen), ohne Rahmen.
pub fn link_button(ui: &mut Ui, p: &Palette, label: &str) -> Response {
    let old = ui.visuals().widgets.hovered.bg_fill;
    ui.visuals_mut().widgets.hovered.bg_fill = p.danger_bg;
    let btn = egui::Button::new(RichText::new(label).font(medium(14.0)).color(p.danger_text))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(4))
        .min_size(Vec2::new(0.0, 28.0));
    let resp = ui.add(btn);
    ui.visuals_mut().widgets.hovered.bg_fill = old;
    theme::focus_ring(ui, &resp, p.accent);
    resp
}

// ---------------------------------------------------------------- Felder

/// Textfeld 36 px, volle Breite. `error` malt einen 2-px-danger-Rahmen.
pub fn text_field(ui: &mut Ui, p: &Palette, value: &mut String, hint: &str, error: bool) -> Response {
    let w = ui.available_width();
    let resp = ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(hint)
            .desired_width(w)
            .font(body(15.0)),
    );
    if error {
        ui.painter().rect_stroke(resp.rect, CornerRadius::same(6), Stroke::new(2.0_f32, p.danger), StrokeKind::Inside);
    }
    theme::focus_ring(ui, &resp, p.accent);
    resp
}

/// Passwortfeld mit Sichtbarkeits-Toggle (Auge rechts daneben).
pub fn password_field(ui: &mut Ui, p: &Palette, value: &mut String, hint: &str, error: bool, visible: &mut bool) -> Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let w = (ui.available_width() - 32.0).max(40.0);
        let resp = ui.add(
            egui::TextEdit::singleline(value)
                .password(!*visible)
                .hint_text(hint)
                .desired_width(w)
                .font(body(15.0)),
        );
        if error {
            ui.painter().rect_stroke(resp.rect, CornerRadius::same(6), Stroke::new(2.0_f32, p.danger), StrokeKind::Inside);
        }
        theme::focus_ring(ui, &resp, p.accent);
        let eye = icon_button(ui, p, if *visible { Icon::EyeOff } else { Icon::Eye }, p.muted);
        if eye.clicked() {
            *visible = !*visible;
        }
        resp
    })
    .inner
}

// ---------------------------------------------------------------- Picker

/// Zeilen-Picker: Secondary-Button + gekürzter Mono-Pfad bzw. Hinweis.
/// Rückgabe: true, wenn der Button geklickt wurde (Dialog öffnen).
pub fn path_picker(ui: &mut Ui, p: &Palette, btn_label: &str, path: &Option<PathBuf>) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        if button(ui, p, Btn::Secondary, btn_label).clicked() {
            clicked = true;
        }
        match path {
            Some(d) => theme::short_path(ui, d, *p),
            None => muted(ui, p, "Noch kein Ordner gewählt"),
        }
    });
    clicked
}

/// Datei-Picker (mehrere Dateien) mit Liste der gewählten Dateien.
pub fn file_picker(ui: &mut Ui, p: &Palette, btn_label: &str, files: &mut Vec<PathBuf>, exts: &[&str], empty_note: &str) {
    ui.horizontal(|ui| {
        if button(ui, p, Btn::Secondary, btn_label).clicked() {
            let picked = rfd::FileDialog::new().add_filter("Dokumente", exts).pick_files();
            if let Some(picked) = picked {
                for f in picked {
                    if !files.contains(&f) {
                        files.push(f);
                    }
                }
            }
        }
        if small_button(ui, p, Btn::Secondary, "Liste leeren").clicked() {
            files.clear();
        }
    });
    if files.is_empty() {
        muted(ui, p, empty_note);
    } else {
        let shown = files.len().min(2);
        for f in &files[..shown] {
            let name = f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| f.display().to_string());
            ui.horizontal(|ui| {
                icon(ui, Icon::File, p.muted);
                ui.label(RichText::new(name).font(mono(13.0)).color(p.text));
            });
        }
        if files.len() > shown {
            muted(ui, p, format!("und {} weitere Dateien", files.len() - shown));
        }
    }
}

// ---------------------------------------------------------------- Badge, Pill, Tabs

/// Orange Pill-Badge (z.B. Review-Zähler).
pub fn badge(ui: &mut Ui, p: &Palette, n: usize) {
    let btn = egui::Button::new(RichText::new(n.to_string()).font(semibold(12.0)).color(p.on_accent))
        .fill(p.accent)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::same(100))
        .min_size(Vec2::new(20.0, 20.0));
    ui.add(btn);
}

/// Tresor-Pill oben rechts: Schloss-Icon + Text.
pub fn vault_pill(ui: &mut Ui, p: &Palette, mode_label: &str) {
    egui::Frame::new()
        .fill(p.sunken)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(100))
        .inner_margin(Margin::symmetric(12, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                icon(ui, Icon::Lock, p.success_text);
                ui.label(RichText::new(format!("Lokal verschlüsselt · {mode_label}")).font(medium(13.0)).color(p.text));
            });
        });
}

/// Tab in der Tab-Leiste (44 px hoch, aktive 3-px-orange-Unterstreichung).
pub fn tab(ui: &mut Ui, p: &Palette, selected: bool, label: &str) -> Response {
    let hovered_color = p.text;
    let color = if selected { p.text } else { p.muted };
    let font = if selected { semibold(15.0) } else { medium(15.0) };
    let btn = egui::Button::new(RichText::new(label).font(font.clone()).color(color))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::ZERO)
        .min_size(Vec2::new(0.0, 44.0));
    let resp = ui.add(btn);
    if resp.hovered() && !selected {
        ui.painter().rect_filled(resp.rect, CornerRadius::ZERO, p.hover);
        // Text neu malen (über dem Hover-Rechteck)
        ui.painter().text(
            resp.rect.center(),
            Align2::CENTER_CENTER,
            label,
            font,
            hovered_color,
        );
    }
    if selected {
        let r = resp.rect;
        ui.painter().line_segment(
            [Pos2::new(r.left(), r.bottom() - 1.5), Pos2::new(r.right(), r.bottom() - 1.5)],
            Stroke::new(3.0_f32, p.accent),
        );
    }
    theme::focus_ring(ui, &resp, p.accent);
    resp
}

// ---------------------------------------------------------------- Karten & Layout

/// Nummerierte Section-Karte im Surface-Look.
pub fn section_card<R>(
    ui: &mut Ui,
    p: &Palette,
    mode: Mode,
    num: u8,
    title: &str,
    sub: &str,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> egui::InnerResponse<R> {
    theme::card(*p, mode).show(ui, |ui| {
        ui.horizontal(|ui| {
            let (rect, _r) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
            ui.painter().circle_filled(rect.center(), 12.0, p.primary);
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, num.to_string(), semibold(13.0), p.on_primary);
            ui.add_space(4.0);
            ui.vertical(|ui| {
                ui.label(RichText::new(title).font(semibold(17.0)).color(p.text));
                if !sub.is_empty() {
                    ui.label(RichText::new(sub).font(body(13.0)).color(p.muted));
                }
            });
        });
        ui.add_space(8.0);
        add_contents(ui)
    })
}

/// Gestreifte Tabelle im Karten-Rahmen. `body` malt die Zellen pro Zeile.
#[allow(clippy::too_many_arguments)]
pub fn striped_table(
    ui: &mut Ui,
    p: &Palette,
    id_salt: &str,
    cols: &[egui_extras::Column],
    headers: &[&str],
    row_height: f32,
    n_rows: usize,
    mut body: impl FnMut(&mut egui_extras::TableRow<'_, '_>),
) {
    egui::Frame::new()
        .fill(p.surface)
        .stroke(Stroke::new(1.0_f32, p.border))
        .corner_radius(CornerRadius::same(10))
        .show(ui, |ui| {
            let mut tb = egui_extras::TableBuilder::new(ui).id_salt(id_salt).striped(true);
            for c in cols {
                tb = tb.column(*c);
            }
            tb.header(26.0, |mut header| {
                for h in headers {
                    header.col(|ui| {
                        ui.painter().rect_filled(ui.max_rect(), CornerRadius::ZERO, p.sunken);
                        ui.add(egui::Label::new(RichText::new(*h).font(semibold(14.0)).color(p.text)).truncate());
                    });
                }
            })
            .body(|b| {
                b.rows(row_height, n_rows, |mut row| {
                    body(&mut row);
                });
            });
        });
}

/// Leere-Zellen-Text der Tabelle (14 px).
pub fn cell(ui: &mut Ui, p: &Palette, text: impl Into<String>) {
    ui.add(egui::Label::new(RichText::new(text.into()).font(body(14.0)).color(p.text)).truncate());
}

// ---------------------------------------------------------------- Status & Meldungen

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Neutral,
    Success,
    Warning,
    Error,
}

/// Statusleiste unten (immer mit Icon + Wort).
pub fn status_bar(ctx: &egui::Context, p: &Palette, kind: Status, text: &str) {
    let (bg, fg, ic) = match kind {
        Status::Neutral => (p.sunken, p.text, Icon::Info),
        Status::Success => (p.success_bg, p.success_text, Icon::Check),
        Status::Warning => (p.warning_bg, p.warning_text, Icon::Alert),
        Status::Error => (p.danger_bg, p.danger_text, Icon::Alert),
    };
    egui::TopBottomPanel::bottom("nema-status")
        .frame(egui::Frame::new().fill(bg).inner_margin(Margin::symmetric(16, 7)))
        .show(ctx, |ui| {
            let r = ui.max_rect();
            ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(1.0_f32, p.border));
            ui.horizontal(|ui| {
                icon(ui, ic, fg);
                ui.label(RichText::new(text).font(body(13.0)).color(fg));
            });
        });
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Msg {
    Warning,
    Error,
    Success,
    Info,
}

/// Message-Box (Icon + Text, farbiger Hintergrund + Rahmen).
pub fn message(ui: &mut Ui, p: &Palette, kind: Msg, text: &str) {
    let (bg, fg, ic, edge) = match kind {
        Msg::Warning => (p.warning_bg, p.warning_text, Icon::Alert, p.accent),
        Msg::Error => (p.danger_bg, p.danger_text, Icon::Alert, p.danger),
        Msg::Success => (p.success_bg, p.success_text, Icon::Check, p.success),
        Msg::Info => (p.sunken, p.text, Icon::Info, p.border),
    };
    let max_w = ui.available_width();
    egui::Frame::new()
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, edge))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_max_width(max_w);
            ui.horizontal(|ui| {
                icon(ui, ic, fg);
                ui.add(egui::Label::new(RichText::new(text).font(body(14.0)).color(fg)).wrap());
            });
        });
}

/// Leer-Zustand mit gestricheltem Rahmen.
pub fn empty_state(ui: &mut Ui, p: &Palette, ic: Icon, text: &str) {
    let w = ui.available_width();
    let (rect, _resp) = ui.allocate_exact_size(Vec2::new(w, 110.0), Sense::hover());
    dashed_rect(ui.painter(), rect, p.border_input);
    paint_icon(ui.painter(), ic, egui::Rect::from_center_size(Pos2::new(rect.center().x, rect.center().y - 14.0), Vec2::splat(32.0)), p.muted);
    ui.painter().text(
        Pos2::new(rect.center().x, rect.center().y + 12.0),
        Align2::CENTER_CENTER,
        text,
        body(13.0),
        p.muted,
    );
}

fn dashed_rect(painter: &egui::Painter, rect: egui::Rect, color: Color32) {
    let st = Stroke::new(1.0_f32, color);
    let dash = 6.0;
    let gap = 4.0;
    let seg = |a: Pos2, b: Pos2| {
        let dir = b - a;
        let len = dir.length();
        if len < 0.1 {
            return;
        }
        let dir = dir / len;
        let mut t = 0.0;
        while t < len {
            let e = (t + dash).min(len);
            painter.line_segment([a + dir * t, a + dir * e], st);
            t = e + gap;
        }
    };
    seg(rect.left_top(), rect.right_top());
    seg(rect.right_top(), rect.right_bottom());
    seg(rect.right_bottom(), rect.left_bottom());
    seg(rect.left_bottom(), rect.left_top());
}

/// Projekt-Eintrag in der Seitenleiste (aktiv = sunken + oranger inset Balken).
pub fn project_item(ui: &mut Ui, p: &Palette, name: &str, selected: bool) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 34.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, CornerRadius::same(6), p.sunken);
        ui.painter().rect_filled(
            egui::Rect::from_min_size(rect.left_top(), Vec2::new(3.0, rect.height())),
            CornerRadius::ZERO,
            p.accent,
        );
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::same(6), p.hover);
    }
    let font = if selected { semibold(15.0) } else { body(15.0) };
    ui.painter().text(
        Pos2::new(rect.left() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        name,
        font,
        p.text,
    );
    theme::focus_ring(ui, &resp, p.accent);
    resp
}

// ---------------------------------------------------------------- Dialog

/// Bestätigungs-Dialog über Scrim. Rückgabe: None = geschlossen,
/// Some(true) = bestätigt, Some(false) = abgebrochen. Esc schliesst (gibt None).
pub fn confirm_dialog(ctx: &egui::Context, p: &Palette, mode: Mode, open: &mut bool, title: &str, text: &str, confirm_label: &str) -> Option<bool> {
    if !*open {
        return None;
    }
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        *open = false;
        return None;
    }
    let screen = ctx.content_rect();
    let mut result = Some(false);
    egui::Area::new(egui::Id::new("nema-confirm"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            ui.painter().rect_filled(screen, CornerRadius::ZERO, p.scrim);
            ui.set_width(420.0);
            egui::Frame::new()
                .fill(p.surface)
                .stroke(Stroke::new(1.0_f32, p.border))
                .corner_radius(CornerRadius::same(10))
                .shadow(theme::shadow_dialog(mode))
                .inner_margin(Margin::same(24))
                .show(ui, |ui| {
                    ui.label(RichText::new(title).font(semibold(22.0)).color(p.text));
                    ui.add_space(8.0);
                    ui.label(RichText::new(text).font(body(15.0)).color(p.muted));
                    ui.add_space(20.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if button(ui, p, Btn::Danger, confirm_label).clicked() {
                            result = Some(true);
                            *open = false;
                        }
                        if button(ui, p, Btn::Secondary, "Abbrechen").clicked() {
                            result = Some(false);
                            *open = false;
                        }
                    });
                });
        });
    result
}
