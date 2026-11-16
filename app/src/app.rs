//! egui-Oberfläche gemäss docs/UI Design (Screens Hell/Dunkel).
//! Dünne Schicht über nema-core — die gesamte Logik liegt in nema-core und ist dort getestet.

use crate::theme::{self, Mode, Palette};
use crate::widgets::{self, Button, Col, Icon, Msg, Status};
use eframe::egui;
use egui::{pos2, vec2, Align, CornerRadius, Layout, Margin, Rect, RichText, Sense, Stroke, StrokeKind, Ui};
use nema_core::crypto;
use nema_core::csv_import;
use nema_core::docx;
use nema_core::fuzzy::FuzzyHit;
use nema_core::mapping;
use nema_core::pdf;
use nema_core::person::Person;
use nema_core::project::{ProjectMeta, ProjectVault};
use nema_core::roster::{self, Roster};
use nema_core::runs;
use nema_core::store::{KeyMode, VaultStore};
use nema_core::uuid::Uuid;
use std::path::PathBuf;

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Setup,
    Unlock,
    Main,
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Classes,
    Persons,
    Anonymize,
    Review,
    Log,
}

struct ReviewItem {
    input: PathBuf,
    anonymize: bool,
    hit: FuzzyHit,
    selected: bool,
}

/// Offene Rückfrage (Bestätigungs-Dialog).
#[derive(PartialEq, Clone, Copy)]
enum Confirm {
    None,
    DeleteProject,
    DeleteClass(Uuid),
}

/// Eintrag der Ergebnisliste im Tab «Anonymisieren» (neueste oben).
struct ResultItem {
    kind: Msg,
    head: String,
    detail: String,
}

pub struct App {
    dir: PathBuf,
    screen: Screen,
    store: Option<VaultStore>,
    /// OS-Schlüsselbund nutzbar? Einmal beim Start geprüft (D-Bus-Roundtrip).
    keychain_ok: bool,

    // Theme
    mode: Mode,
    mode_override: Option<Mode>,
    applied_mode: Option<Mode>,
    fonts_ready: bool,
    logo_light: Option<egui::TextureHandle>,
    logo_dark: Option<egui::TextureHandle>,

    // Setup
    setup_keychain: bool,
    setup_pw1: String,
    setup_pw2: String,
    setup_msg: String,
    setup_tried: bool,
    show_pw1: bool,
    show_pw2: bool,

    // Unlock
    unlock_pw: String,
    /// Hinweis, wenn der Schlüsselbund nicht klappt (Warnung).
    unlock_msg: String,
    /// Fehler am Passwortfeld (falsches Passwort).
    unlock_pw_err: String,
    show_pw_unlock: bool,
    focus_unlock_pw: bool,

    // Main
    projects: Vec<ProjectMeta>,
    selected: Option<Uuid>,
    vault: Option<ProjectVault>,
    new_project: String,
    tab: Tab,
    status: String,
    status_kind: Status,
    confirm: Confirm,
    dialog_focus: bool,

    // Klassen (Lernenden-Verwaltung, projektübergreifend)
    roster: Roster,
    sel_class: Option<Uuid>,
    new_class: String,
    cf_first: String,
    cf_last: String,
    cf_email: String,
    cf_matr: String,
    /// Name für «Einzelpersonen als Klasse speichern» im Projekt.
    promote_name: String,
    new_term: String,

    // Personen
    pf_first: String,
    pf_last: String,
    pf_email: String,
    pf_matr: String,
    delim: char,

    // Anonymisieren
    anon_files: Vec<PathBuf>,
    deanon_files: Vec<PathBuf>,
    out_dir_anon: Option<PathBuf>,
    out_dir_deanon: Option<PathBuf>,
    results: Vec<ResultItem>,
    review: Vec<ReviewItem>,

    // Läufe
    run_label: String,
    run_files: Vec<PathBuf>,
    run_root: Option<PathBuf>,
    deanon_folder: Option<PathBuf>,
}

/// Dekodiert ein PNG (include_bytes!) als Textur.
fn png_texture(ctx: &egui::Context, name: &str, bytes: &'static [u8]) -> egui::TextureHandle {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().expect("gültiges PNG");
    let mut buf = vec![0u8; reader.output_buffer_size().expect("PNG-Grösse")];
    let info = reader.next_frame(&mut buf).expect("PNG-Frame");
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            let mut v = Vec::with_capacity(w * h * 4);
            for px in buf[..info.buffer_size()].as_chunks::<3>().0 {
                v.extend_from_slice(px);
                v.push(255);
            }
            v
        }
        other => panic!("ununterstütztes PNG-Format: {other:?}"),
    };
    let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
    let options = egui::TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..egui::TextureOptions::LINEAR
    };
    ctx.load_texture(name, egui::ImageData::Color(std::sync::Arc::new(img)), options)
}

fn file_name(f: &std::path::Path) -> String {
    f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| f.display().to_string())
}

impl App {
    pub fn new() -> Self {
        let mut app = Self::blank(VaultStore::default_dir(), crypto::keychain_backend_available());
        if VaultStore::is_initialized(&app.dir) {
            match VaultStore::unlock(&app.dir) {
                Ok(store) => app.enter_main(store),
                Err(_) => app.screen = Screen::Unlock,
            }
        }
        app
    }

    /// Leerer Zustand ohne Zugriff auf Tresor oder Schlüsselbund.
    fn blank(dir: PathBuf, keychain_ok: bool) -> Self {
        Self {
            dir,
            screen: Screen::Setup,
            store: None,
            keychain_ok,
            mode: Mode::Light,
            mode_override: None,
            applied_mode: None,
            fonts_ready: false,
            logo_light: None,
            logo_dark: None,
            setup_keychain: keychain_ok,
            setup_pw1: String::new(),
            setup_pw2: String::new(),
            setup_msg: String::new(),
            setup_tried: false,
            show_pw1: false,
            show_pw2: false,
            unlock_pw: String::new(),
            unlock_msg: String::new(),
            unlock_pw_err: String::new(),
            show_pw_unlock: false,
            focus_unlock_pw: true,
            projects: Vec::new(),
            selected: None,
            vault: None,
            new_project: String::new(),
            tab: Tab::Persons,
            status: String::new(),
            status_kind: Status::Neutral,
            confirm: Confirm::None,
            dialog_focus: false,
            roster: Roster::default(),
            sel_class: None,
            new_class: String::new(),
            cf_first: String::new(),
            cf_last: String::new(),
            cf_email: String::new(),
            cf_matr: String::new(),
            promote_name: String::new(),
            new_term: String::new(),
            pf_first: String::new(),
            pf_last: String::new(),
            pf_email: String::new(),
            pf_matr: String::new(),
            delim: ';',
            anon_files: Vec::new(),
            deanon_files: Vec::new(),
            out_dir_anon: None,
            out_dir_deanon: None,
            results: Vec::new(),
            review: Vec::new(),
            run_label: String::new(),
            run_files: Vec::new(),
            run_root: None,
            deanon_folder: None,
        }
    }

    fn enter_main(&mut self, store: VaultStore) {
        self.projects = store.list_projects().unwrap_or_default();
        match store.load_roster() {
            Ok(roster) => self.roster = roster,
            Err(e) => self.set_status(Status::Error, format!("Klassen nicht lesbar: {e}")),
        }
        self.sel_class = self.roster.classes.first().map(|c| c.id);
        self.store = Some(store);
        self.screen = Screen::Main;
        if let Some(id) = self.projects.first().map(|p| p.id) {
            self.select_project(id);
        }
    }

    fn select_project(&mut self, id: Uuid) {
        self.selected = Some(id);
        self.vault = self.store.as_ref().and_then(|s| s.load_project(id).ok());
        self.sync_current_project();
    }

    /// Übernimmt Änderungen aus den verknüpften Klassen ins geöffnete Projekt.
    fn sync_current_project(&mut self) {
        let (Some(store), Some(mut vault)) = (self.store.clone(), self.vault.clone()) else { return };
        if vault.sync_with_roster(&self.roster) {
            match store.save_project(&vault) {
                Ok(()) => self.vault = Some(vault),
                Err(e) => self.set_status(Status::Error, format!("Speichern fehlgeschlagen: {e}")),
            }
        }
    }

    /// Speichert die Klassen und gleicht das geöffnete Projekt ab. false bei Fehler.
    fn roster_changed(&mut self) -> bool {
        if let Some(store) = self.store.clone() {
            if let Err(e) = store.save_roster(&self.roster) {
                self.set_status(Status::Error, format!("Klassen speichern fehlgeschlagen: {e}"));
                return false;
            }
        }
        self.sync_current_project();
        true
    }

    fn set_status(&mut self, kind: Status, text: impl Into<String>) {
        self.status_kind = kind;
        self.status = text.into();
    }

    fn push_result(&mut self, kind: Msg, head: impl Into<String>, detail: impl Into<String>) {
        self.results.insert(0, ResultItem { kind, head: head.into(), detail: detail.into() });
    }

    fn ensure_textures(&mut self, ctx: &egui::Context) {
        if self.logo_light.is_none() {
            self.logo_light = Some(png_texture(ctx, "nema-icon-light", include_bytes!("../../packaging/nema-icon-light.png")));
        }
        if self.logo_dark.is_none() {
            self.logo_dark = Some(png_texture(ctx, "nema-icon-dark", include_bytes!("../../packaging/nema-icon-dark.png")));
        }
    }

    fn logo(&self) -> Option<egui::TextureId> {
        let tex = if self.mode == Mode::Dark { &self.logo_dark } else { &self.logo_light };
        tex.as_ref().map(|t| t.id())
    }

    /// Ein Frame der Oberfläche (unabhängig von eframe, damit headless testbar).
    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.fonts_ready {
            // Fonts werden erst im nächsten Frame aktiv: bis dahin nichts zeichnen.
            theme::install_fonts(ctx);
            self.fonts_ready = true;
            ctx.request_repaint();
            return;
        }
        let mode = self.mode_override.unwrap_or_else(|| theme::system_mode(ctx));
        if self.applied_mode != Some(mode) {
            theme::apply(ctx, mode);
            self.applied_mode = Some(mode);
        }
        self.mode = mode;
        self.ensure_textures(ctx);
        match self.screen {
            Screen::Setup => self.ui_setup(ctx),
            Screen::Unlock => self.ui_unlock(ctx),
            Screen::Main => self.ui_main(ctx),
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

// ---------------------------------------------------------------- Setup & Unlock

/// Zentrierte Spalte (440 px) wie in den Screens.
fn centered_column(ctx: &egui::Context, p: &Palette, add_contents: impl FnOnce(&mut Ui)) {
    egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ctx, |ui| {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let w = 440.0_f32.min(ui.available_width() - 32.0);
            let side = ((ui.available_width() - w) / 2.0).max(0.0);
            ui.add_space(32.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                ui.add_space(side);
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.spacing_mut().item_spacing.x = 8.0;
                    add_contents(ui);
                });
            });
            ui.add_space(32.0);
        });
    });
}

fn logo_image(ui: &mut Ui, logo: Option<egui::TextureId>, size: f32) {
    if let Some(id) = logo {
        ui.vertical_centered(|ui| {
            ui.image((id, vec2(size, size)));
        });
    }
}

/// Radio-Optionskarte im Setup (`.nm-opt`).
fn option_card(ui: &mut Ui, p: &Palette, selected: bool, enabled: bool, title: &str, tag: Option<&str>, desc: &str) -> bool {
    let w = ui.available_width();
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 66.0), sense);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::RadioButton, enabled, selected, title));
    let radius = CornerRadius::same(theme::RADIUS_LG);
    let t = ui.ctx().animate_bool_with_time(resp.id.with("hover"), enabled && resp.hovered() && !selected, theme::HOVER_SECONDS);
    let fill = if enabled { theme::mix(p.surface, p.hover, t) } else { p.sunken };
    let stroke = if selected && enabled {
        Stroke::new(2.0_f32, p.text)
    } else if enabled {
        Stroke::new(1.0_f32, p.border_input)
    } else {
        Stroke::new(1.0_f32, p.border)
    };
    let painter = ui.painter();
    painter.rect(rect, radius, fill, stroke, StrokeKind::Inside);

    // Radio
    let c = pos2(rect.left() + 12.0 + 8.0, rect.top() + 12.0 + 11.0);
    let on = selected && enabled;
    painter.circle(c, 7.5, p.surface, Stroke::new(1.5_f32, if on { p.primary } else { p.border_input }));
    if on {
        painter.circle_filled(c, 4.0, p.primary);
    }

    // Titel, Tag, Beschreibung
    let x = rect.left() + 12.0 + 16.0 + 12.0;
    let title_color = if enabled { p.text } else { p.muted };
    let g = painter.layout_no_wrap(title.to_owned(), widgets::semibold(15.0), title_color);
    let title_w = g.size().x;
    painter.galley(pos2(x, rect.top() + 12.0 + (22.0 - g.size().y) / 2.0), g, title_color);
    if let Some(tag) = tag {
        let tg = painter.layout_no_wrap(tag.to_owned(), widgets::medium(12.0), p.text);
        let tr = Rect::from_min_size(pos2(x + title_w + 8.0, rect.top() + 13.0), vec2(tg.size().x + 16.0, 20.0));
        painter.rect(tr, CornerRadius::same(10), p.sunken, Stroke::new(1.0_f32, p.border), StrokeKind::Inside);
        painter.galley(tr.center() - tg.size() / 2.0, tg, p.text);
    }
    let dg = painter.layout_no_wrap(desc.to_owned(), widgets::body(13.0), p.muted);
    painter
        .with_clip_rect(rect.shrink(8.0))
        .galley(pos2(x, rect.top() + 12.0 + 22.0 + (18.0 - dg.size().y) / 2.0), dg, p.muted);

    if resp.has_focus() {
        widgets::focus_ring(ui, p, rect, theme::RADIUS_LG);
    }
    let clicked = resp.clicked();
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
    clicked
}

impl App {
    fn try_create_vault(&mut self) {
        self.setup_tried = true;
        self.setup_msg.clear();
        if self.setup_pw1.len() < 8 || self.setup_pw1 != self.setup_pw2 {
            return; // Fehler stehen inline an den Feldern
        }
        match VaultStore::init(&self.dir, self.setup_keychain, Some(self.setup_pw1.as_str())) {
            Ok(store) => self.enter_main(store),
            Err(e) => self.setup_msg = format!("Fehler: {e}"),
        }
    }

    fn ui_setup(&mut self, ctx: &egui::Context) {
        let p = theme::palette(self.mode);
        let logo = self.logo();
        let kc = self.keychain_ok;
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let mut submit = enter && ctx.memory(|m| m.focused().is_none());
        let location = format!("Speicherort: {}", widgets::display_path(&self.dir));

        centered_column(ctx, &p, |ui| {
            logo_image(ui, logo, 56.0);
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                widgets::h1(ui, &p, "Verschlüsselten Tresor anlegen");
                widgets::lede(ui, &p, "Verschlüsselten Tresor für die Namenslisten anlegen. Alle Namen bleiben lokal und verschlüsselt auf diesem Gerät.");
            });
            ui.add_space(8.0);

            let kc_desc = if kc { "Der Tresor wird über das Betriebssystem entsperrt." } else { "Auf diesem Gerät nicht verfügbar." };
            if option_card(ui, &p, self.setup_keychain, kc, "OS-Schlüsselbund", kc.then_some("Empfohlen"), kc_desc) {
                self.setup_keychain = true;
            }
            if option_card(ui, &p, !self.setup_keychain, true, "Nur Backup-Passwort", None, "Das Passwort wird bei jedem Start verlangt.") {
                self.setup_keychain = false;
            }
            ui.add_space(8.0);

            let too_short = self.setup_tried && self.setup_pw1.len() < 8;
            let mismatch = (self.setup_tried || !self.setup_pw2.is_empty()) && self.setup_pw1 != self.setup_pw2;
            ui.spacing_mut().item_spacing.y = 6.0;

            widgets::field_label(ui, &p, "Backup-Passwort (min. 8 Zeichen)");
            let r1 = widgets::password_field(ui, &p, &mut self.setup_pw1, "", too_short, &mut self.show_pw1);
            if too_short {
                widgets::error_line(ui, &p, "Das Passwort braucht mindestens 8 Zeichen");
            } else {
                widgets::help(ui, &p, "Fallback, falls der Schlüsselbund mal nicht verfügbar ist.");
            }
            ui.add_space(6.0);

            widgets::field_label(ui, &p, "Passwort wiederholen");
            let r2 = widgets::password_field(ui, &p, &mut self.setup_pw2, "", mismatch, &mut self.show_pw2);
            if mismatch {
                widgets::error_line(ui, &p, "Passwörter stimmen nicht überein");
            }
            if (r1.lost_focus() || r2.lost_focus()) && enter {
                submit = true;
            }
            ui.add_space(10.0);

            if Button::primary("Tresor erstellen").icon(Icon::Lock).block().show(ui, &p).clicked() {
                submit = true;
            }
            if !self.setup_msg.is_empty() {
                ui.add_space(6.0);
                widgets::message(ui, &p, Msg::Error, &self.setup_msg);
            }
            ui.add_space(10.0);

            // Speicherort: Ordner-Icon + Mono-Pfad, zentriert
            let font = widgets::mono(13.0);
            let shown = widgets::elide_middle(ui.painter(), &location, &font, ui.available_width() - 24.0);
            let g = ui.painter().layout_no_wrap(shown, font, p.muted);
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
            let x = rect.center().x - (22.0 + g.size().x) / 2.0;
            let ic = Rect::from_min_size(pos2(x, rect.center().y - 8.0), vec2(16.0, 16.0));
            widgets::paint_icon(ui.painter(), Icon::Folder, ic, p.muted, 1.5);
            ui.painter().galley(pos2(x + 22.0, rect.center().y - g.size().y / 2.0), g, p.muted);
        });

        if submit {
            self.try_create_vault();
        }
    }

    fn ui_unlock(&mut self, ctx: &egui::Context) {
        let p = theme::palette(self.mode);
        let logo = self.logo();
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let mut unlock_pw = enter && ctx.memory(|m| m.focused().is_none());
        let mut unlock_kc = false;

        centered_column(ctx, &p, |ui| {
            ui.add_space(24.0);
            logo_image(ui, logo, 64.0);
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                widgets::h1(ui, &p, "Tresor entsperren");
                widgets::lede(ui, &p, "Alle Namen bleiben lokal und verschlüsselt auf diesem Gerät.");
            });
            ui.add_space(8.0);

            if Button::primary("Per OS-Schlüsselbund entsperren").icon(Icon::Lock).block().show(ui, &p).clicked() {
                unlock_kc = true;
            }
            if !self.unlock_msg.is_empty() {
                ui.add_space(4.0);
                widgets::message(ui, &p, Msg::Warning, &self.unlock_msg);
            }
            ui.add_space(4.0);
            widgets::divider(ui, &p, "oder Backup-Passwort");
            ui.add_space(4.0);

            ui.spacing_mut().item_spacing.y = 6.0;
            widgets::field_label(ui, &p, "Backup-Passwort");
            let has_err = !self.unlock_pw_err.is_empty();
            let r = widgets::password_field(ui, &p, &mut self.unlock_pw, "", has_err, &mut self.show_pw_unlock);
            if std::mem::take(&mut self.focus_unlock_pw) {
                r.request_focus();
            }
            if r.changed() {
                self.unlock_pw_err.clear();
            }
            if r.lost_focus() && enter {
                unlock_pw = true;
            }
            if !self.unlock_pw_err.is_empty() {
                widgets::error_line(ui, &p, &self.unlock_pw_err);
            }
            ui.add_space(6.0);
            if Button::secondary("Entsperren").block().show(ui, &p).clicked() {
                unlock_pw = true;
            }
        });

        if unlock_kc {
            match VaultStore::unlock(&self.dir) {
                Ok(store) => self.enter_main(store),
                Err(e) => {
                    self.unlock_msg = format!("Schlüsselbund nicht verfügbar ({e}) — bitte Backup-Passwort eingeben.");
                    self.focus_unlock_pw = true;
                }
            }
        }
        if unlock_pw && self.screen == Screen::Unlock {
            match VaultStore::unlock_with_password(&self.dir, &self.unlock_pw) {
                Ok(store) => self.enter_main(store),
                Err(e) => {
                    self.unlock_pw_err = format!("{e}");
                    self.focus_unlock_pw = true;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- Hauptfenster

impl App {
    fn ui_main(&mut self, ctx: &egui::Context) {
        let p = theme::palette(self.mode);
        let mode = self.mode;
        let mode_label = match self.store.as_ref().map(|s| s.mode()) {
            Some(KeyMode::Keychain) => "OS-Schlüsselbund",
            Some(KeyMode::Password) => "Backup-Passwort",
            None => "?",
        };
        let n_review = self.review.len();

        // Tab-Leiste oben (über die ganze Breite)
        egui::TopBottomPanel::top("nema-tabs")
            .exact_height(44.0)
            .frame(egui::Frame::new().fill(p.surface).inner_margin(Margin::symmetric(24, 0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 24.0;
                    let tabs = [
                        (Tab::Classes, "Klassen", None),
                        (Tab::Persons, "Personen", None),
                        (Tab::Anonymize, "Anonymisieren", None),
                        (Tab::Review, "Review", (n_review > 0).then_some(n_review)),
                        (Tab::Log, "Protokoll", None),
                    ];
                    for (t, label, badge) in tabs {
                        if widgets::tab(ui, &p, self.tab == t, label, badge).clicked() {
                            self.tab = t;
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        widgets::vault_pill(ui, &p, mode_label);
                    });
                });
            });

        // Statusleiste unten (über die ganze Breite)
        let status_text = if self.status.is_empty() { "Bereit." } else { self.status.as_str() };
        widgets::status_bar(ctx, &p, if self.status.is_empty() { Status::Neutral } else { self.status_kind }, status_text);

        // Seitenleiste: Projekte
        egui::SidePanel::left("nema-projects")
            .exact_width(theme::SIDEBAR_WIDTH)
            .resizable(false)
            .frame(egui::Frame::new().fill(p.surface).inner_margin(Margin::same(16)))
            .show(ctx, |ui| self.ui_sidebar(ui, &p));

        // Inhalt
        let tab = self.tab;
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.bg)).show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                egui::Frame::new().inner_margin(Margin::same(24)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match tab {
                        Tab::Classes => self.ui_classes(ui, &p, mode),
                        Tab::Persons => self.ui_persons(ui, &p, mode),
                        Tab::Anonymize => self.ui_anonymize(ui, &p, mode),
                        Tab::Review => self.ui_review(ui, &p),
                        Tab::Log => self.ui_log(ui, &p),
                    }
                });
            });
        });

        // Bestätigungs-Dialoge
        if self.confirm != Confirm::None {
            let (title, body, action) = match self.confirm {
                Confirm::DeleteClass(id) => {
                    let (name, count) = self.roster.class(id).map(|c| (c.name.clone(), c.persons.len())).unwrap_or_default();
                    (
                        "Klasse löschen?",
                        format!("Die Klasse «{name}» mit {count} Lernenden wird gelöscht. In Projekten, die sie verwenden, bleiben die Lernenden als Einzelpersonen erhalten."),
                        "Klasse löschen",
                    )
                }
                _ => {
                    let (name, count) = self.vault.as_ref().map(|v| (v.name.clone(), v.persons.len())).unwrap_or_default();
                    (
                        "Projekt löschen?",
                        format!("Das Projekt «{name}» mit {count} Personen wird gelöscht. Das lässt sich nicht rückgängig machen."),
                        "Projekt löschen",
                    )
                }
            };
            let focus = std::mem::take(&mut self.dialog_focus);
            match widgets::confirm_dialog(ctx, &p, mode, title, &body, action, focus) {
                Some(true) => match std::mem::replace(&mut self.confirm, Confirm::None) {
                    Confirm::DeleteProject => self.delete_selected_project(),
                    Confirm::DeleteClass(id) => self.delete_class(id),
                    Confirm::None => {}
                },
                Some(false) => self.confirm = Confirm::None,
                None => {}
            }
        }
    }

    fn ui_sidebar(&mut self, ui: &mut Ui, p: &Palette) {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.label(RichText::new("PROJEKTE").font(widgets::semibold(13.0)).color(p.muted).extra_letter_spacing(0.5));
        ui.add_space(4.0);

        // Fuss: Label 18 + 6 + Feld 36 + 12 + Button 36 + 12 + Button 36
        const FOOTER_H: f32 = 156.0;
        let list_h = (ui.available_height() - FOOTER_H - 12.0).max(38.0);
        let mut to_select = None;
        egui::ScrollArea::vertical()
            .id_salt("projects")
            .max_height(list_h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.projects.is_empty() {
                    widgets::muted(ui, p, "Noch keine Projekte — unten ein Projekt anlegen.");
                }
                for proj in &self.projects {
                    if widgets::project_item(ui, p, &proj.name, self.selected == Some(proj.id)).clicked() {
                        to_select = Some(proj.id);
                    }
                }
            });
        if let Some(id) = to_select {
            self.select_project(id);
        }

        ui.spacing_mut().item_spacing.y = 0.0;
        ui.add_space(12.0);
        widgets::field_label(ui, p, "Neues Projekt");
        ui.add_space(6.0);
        let field = widgets::text_field(ui, p, &mut self.new_project, "Projektname", false);
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.add_space(12.0);
        let create = Button::secondary("Anlegen").icon(Icon::Plus).block().show(ui, p).clicked();
        if create || enter {
            self.create_project();
        }
        ui.add_space(12.0);
        if Button::danger("Projekt löschen").block().enabled(self.selected.is_some()).show(ui, p).clicked() {
            self.confirm = Confirm::DeleteProject;
            self.dialog_focus = true;
        }
    }

    fn create_project(&mut self) {
        let name = self.new_project.trim().to_string();
        if name.is_empty() {
            return;
        }
        let Some(store) = self.store.clone() else { return };
        match store.create_project(&name) {
            Ok(v) => {
                self.projects = store.list_projects().unwrap_or_default();
                self.select_project(v.id);
                self.new_project.clear();
                self.set_status(Status::Success, format!("Projekt «{name}» angelegt."));
            }
            Err(e) => self.set_status(Status::Error, format!("Fehler: {e}")),
        }
    }

    fn delete_selected_project(&mut self) {
        let (Some(id), Some(store)) = (self.selected, self.store.clone()) else { return };
        match store.delete_project(id) {
            Ok(()) => {
                self.projects = store.list_projects().unwrap_or_default();
                self.selected = None;
                self.vault = None;
                self.set_status(Status::Success, "Projekt gelöscht.");
            }
            Err(e) => self.set_status(Status::Error, format!("Löschen fehlgeschlagen: {e}")),
        }
    }

    // ------------------------------------------------------------ Klassen

    fn create_class(&mut self) {
        let name = self.new_class.trim().to_string();
        if name.is_empty() {
            return;
        }
        if self.roster.classes.iter().any(|c| c.name.eq_ignore_ascii_case(&name)) {
            self.set_status(Status::Error, format!("Die Klasse «{name}» gibt es schon."));
            return;
        }
        let id = self.roster.add_class(name.clone());
        if self.roster_changed() {
            self.sel_class = Some(id);
            self.new_class.clear();
            self.set_status(Status::Success, format!("Klasse «{name}» angelegt."));
        }
    }

    fn delete_class(&mut self, id: Uuid) {
        self.roster.remove_class(id);
        if self.roster_changed() {
            self.sel_class = self.roster.classes.first().map(|c| c.id);
            self.set_status(Status::Success, "Klasse gelöscht.");
        }
    }

    fn ui_classes(&mut self, ui: &mut Ui, p: &Palette, mode: Mode) {
        ui.spacing_mut().item_spacing.y = 2.0;
        widgets::h2(ui, p, "Klassen");
        widgets::lede(ui, p, "Lernende einmal erfassen und in beliebig vielen Projekten verwenden.");
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.add_space(8.0);

        // Klassenwahl
        let mut select = None;
        if !self.roster.classes.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for c in &self.roster.classes {
                    let label = format!("{} · {}", c.name, c.persons.len());
                    if widgets::chip(ui, p, self.sel_class == Some(c.id), false, &label).clicked() {
                        select = Some(c.id);
                    }
                }
            });
        }
        if let Some(id) = select {
            self.sel_class = Some(id);
        }
        let mut create = false;
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 36.0), Layout::left_to_right(Align::Center), |ui| {
            ui.allocate_ui_with_layout(vec2(260.0, 36.0), Layout::top_down(Align::Min), |ui| {
                if widgets::text_field(ui, p, &mut self.new_class, "Neue Klasse, z.B. INF 24a", false).lost_focus() && enter {
                    create = true;
                }
            });
            if Button::secondary("Klasse anlegen").icon(Icon::Plus).show(ui, p).clicked() {
                create = true;
            }
        });
        if create {
            self.create_class();
        }
        ui.add_space(8.0);

        let Some(class) = self.sel_class.and_then(|id| self.roster.class(id)).cloned() else {
            widgets::empty_state(ui, p, Icon::UserPlus, "Noch keine Klasse — oben eine Klasse anlegen und danach die Namensliste importieren.");
            ui.add_space(16.0);
            self.ui_terms(ui, p, mode);
            return;
        };

        // Kopf der gewählten Klasse: Name links, Import (Hauptaktion) rechts
        let mut import = false;
        let mut delete_class = false;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 36.0), Layout::left_to_right(Align::Center), |ui| {
            widgets::h3(ui, p, &class.name);
            let replaced = if roster::class_name_is_replaced(&class.name) {
                format!("{} Lernende · in Unterlagen: {}", class.persons.len(), roster::class_placeholder(class.id))
            } else {
                format!("{} Lernende · Name zu kurz, bleibt in Unterlagen stehen", class.persons.len())
            };
            ui.add(egui::Label::new(RichText::new(replaced).font(widgets::body(13.0)).color(p.muted)).truncate());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                import = Button::primary("Namensliste importieren…").icon(Icon::File).show(ui, p).clicked();
                delim_combo(ui, p, &mut self.delim);
                ui.label(RichText::new("Trennzeichen").font(widgets::medium(13.0)).color(p.text));
            });
        });
        ui.add_space(4.0);

        let mut to_delete = None;
        if class.persons.is_empty() {
            widgets::empty_state(ui, p, Icon::UserPlus, "Noch keine Lernenden — Liste importieren oder einzeln hinzufügen.");
        } else {
            let cols = [
                Col::flex("Nachname", 1.1),
                Col::flex("Vorname", 1.0),
                Col::flex("E-Mail", 1.9),
                Col::flex("Matrikel", 1.0),
                Col::fixed("Aktion", 88.0).right(),
            ];
            widgets::table(ui, p, &cols, class.persons.len(), |ui, r, c| {
                let person = &class.persons[r];
                match c {
                    0 => widgets::cell(ui, p, &person.last_name),
                    1 => widgets::cell(ui, p, &person.first_name),
                    2 => widgets::cell(ui, p, if person.emails.is_empty() { "—".to_string() } else { person.emails.join(", ") }),
                    3 => widgets::cell(ui, p, person.matriculation.clone().unwrap_or_else(|| "—".into())),
                    _ => {
                        if widgets::link_button(ui, p, "löschen").clicked() {
                            to_delete = Some(person.id);
                        }
                    }
                }
            });
        }
        ui.add_space(8.0);
        let add = person_form(ui, p, mode, [&mut self.cf_last, &mut self.cf_first, &mut self.cf_email, &mut self.cf_matr]);
        ui.add_space(8.0);
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 28.0), Layout::right_to_left(Align::Center), |ui| {
            delete_class = Button::danger("Klasse löschen").small().show(ui, p).clicked();
        });

        // Aktionen anwenden
        if import {
            if let Some(path) = rfd::FileDialog::new().add_filter("Listen", &["csv", "txt", "tsv"]).pick_file() {
                match std::fs::read_to_string(&path) {
                    Ok(text) => match csv_import::parse(&text, self.delim) {
                        Ok(inputs) => {
                            let n = inputs.len();
                            if let Some(c) = self.roster.class_mut(class.id) {
                                c.persons.extend(inputs.into_iter().map(|i| i.into_person()));
                            }
                            if self.roster_changed() {
                                self.set_status(Status::Success, format!("Namensliste importiert — {n} Lernende in «{}».", class.name));
                            }
                        }
                        Err(e) => self.set_status(Status::Error, format!("Importfehler: {e}")),
                    },
                    Err(e) => self.set_status(Status::Error, format!("Lesefehler: {e}")),
                }
            }
        }
        if add {
            if let Some(person) = take_person(&mut self.cf_first, &mut self.cf_last, &mut self.cf_email, &mut self.cf_matr) {
                if let Some(c) = self.roster.class_mut(class.id) {
                    c.persons.push(person);
                }
                if self.roster_changed() {
                    self.set_status(Status::Success, "Lernende Person hinzugefügt.");
                }
            }
        }
        if let Some(id) = to_delete {
            if let Some(c) = self.roster.class_mut(class.id) {
                c.persons.retain(|x| x.id != id);
            }
            if self.roster_changed() {
                self.set_status(Status::Neutral, "Aus der Klasse entfernt. In bestehenden Projekten bleibt die Person als Einzelperson.");
            }
        }
        if delete_class {
            self.confirm = Confirm::DeleteClass(class.id);
            self.dialog_focus = true;
        }
        ui.add_space(16.0);
        self.ui_terms(ui, p, mode);
    }

    /// Weitere Begriffe (Schulname, Ort, Lehrperson …), die in allen Projekten ersetzt werden.
    fn ui_terms(&mut self, ui: &mut Ui, p: &Palette, mode: Mode) {
        let mut add = false;
        let mut remove = None;
        theme::card(p, mode).show(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::h3(ui, p, "Weitere Begriffe ersetzen");
            widgets::muted(
                ui,
                p,
                "Z.B. Schulname, Ort oder Name der Lehrperson. Sie werden in allen Projekten durch einen Platzhalter ersetzt und beim De-Anonymisieren zurückgesetzt. Die Namen der Klassen eines Projekts werden automatisch ersetzt.",
            );
            ui.add_space(4.0);
            if !self.roster.terms.is_empty() {
                let cols = [Col::flex("Begriff", 2.0), Col::flex("Platzhalter", 1.0), Col::fixed("Aktion", 88.0).right()];
                let terms = &self.roster.terms;
                widgets::table(ui, p, &cols, terms.len(), |ui, r, c| match c {
                    0 => widgets::cell(ui, p, &terms[r].text),
                    1 => widgets::cell_mono_line(ui, p, roster::term_placeholder(terms[r].id)),
                    _ => {
                        if widgets::link_button(ui, p, "löschen").clicked() {
                            remove = Some(terms[r].id);
                        }
                    }
                });
                ui.add_space(4.0);
            }
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 36.0), Layout::left_to_right(Align::Center), |ui| {
                ui.allocate_ui_with_layout(vec2(320.0, 36.0), Layout::top_down(Align::Min), |ui| {
                    if widgets::text_field(ui, p, &mut self.new_term, "Begriff, z.B. Berufsschule Musterstadt", false).lost_focus() && enter {
                        add = true;
                    }
                });
                if Button::secondary("Hinzufügen").show(ui, p).clicked() {
                    add = true;
                }
            });
        });
        if add && !self.new_term.trim().is_empty() {
            let text = self.new_term.trim().to_string();
            if self.roster.add_term(&text).is_some() {
                if self.roster_changed() {
                    self.new_term.clear();
                    self.set_status(Status::Success, format!("Begriff «{text}» wird ab jetzt ersetzt."));
                }
            } else {
                self.set_status(
                    Status::Error,
                    format!("«{text}» ist zu kurz (mindestens {} Zeichen) oder schon erfasst.", roster::MIN_TERM_LEN),
                );
            }
        }
        if let Some(id) = remove {
            self.roster.remove_term(id);
            if self.roster_changed() {
                self.set_status(Status::Neutral, "Begriff entfernt. Ältere Unterlagen lassen sich weiterhin de-anonymisieren.");
            }
        }
    }

    // ------------------------------------------------------------ Personen

    fn ui_persons(&mut self, ui: &mut Ui, p: &Palette, mode: Mode) {
        let Some(mut vault) = self.vault.clone() else {
            widgets::empty_state(ui, p, Icon::Folder, "Kein Projekt ausgewählt — links ein Projekt anlegen oder wählen.");
            return;
        };
        let Some(store) = self.store.clone() else { return };
        let mut dirty = false;

        ui.spacing_mut().item_spacing.y = 2.0;
        widgets::h2(ui, p, &vault.name);
        widgets::lede(ui, p, &format!("{} Personen mit Pseudonymen", vault.persons.len()));
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.add_space(8.0);

        // Klassen dieses Projekts
        let mut goto_classes = false;
        theme::card(p, mode).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                ui.label(RichText::new("Klassen in diesem Projekt").font(widgets::semibold(15.0)).color(p.text));
                ui.label(RichText::new("Anklicken, um eine Klasse hinzuzufügen oder zu entfernen").font(widgets::body(13.0)).color(p.muted));
            });
            if self.roster.classes.is_empty() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    ui.label(RichText::new("Noch keine Klassen erfasst.").font(widgets::body(14.0)).color(p.muted));
                    goto_classes = Button::secondary("Klassen verwalten").small().show(ui, p).clicked();
                });
            } else {
                ui.horizontal_wrapped(|ui| {
                    for c in &self.roster.classes {
                        let linked = vault.class_ids.contains(&c.id);
                        let label = format!("{} · {}", c.name, c.persons.len());
                        if widgets::chip(ui, p, linked, true, &label).clicked() {
                            if linked {
                                vault.unlink_class(c.id, &self.roster);
                            } else {
                                vault.link_class(c.id, &self.roster);
                            }
                            dirty = true;
                        }
                    }
                });
            }
        });
        if goto_classes {
            self.tab = Tab::Classes;
        }
        ui.add_space(8.0);

        if vault.persons.is_empty() {
            widgets::empty_state(ui, p, Icon::UserPlus, "Noch keine Personen — oben eine Klasse hinzufügen oder unten einzeln erfassen.");
        } else {
            let cols = [
                Col::flex("Nachname", 1.1),
                Col::flex("Vorname", 1.0),
                Col::flex("E-Mail", 1.7),
                Col::flex("Klasse", 1.0),
                Col::flex("Pseudonym", 1.5),
                Col::fixed("Aktion", 88.0).right(),
            ];
            let mut to_delete = None;
            widgets::table(ui, p, &cols, vault.persons.len(), |ui, r, c| {
                let person = &vault.persons[r];
                let class = vault.class_of(person.id, &self.roster);
                match c {
                    0 => widgets::cell(ui, p, &person.last_name),
                    1 => widgets::cell(ui, p, &person.first_name),
                    2 => widgets::cell(ui, p, if person.emails.is_empty() { "—".to_string() } else { person.emails.join(", ") }),
                    3 => match class {
                        Some(name) => widgets::cell(ui, p, name),
                        None => {
                            ui.add(egui::Label::new(RichText::new("einzeln").font(widgets::body(14.0)).color(p.muted)).truncate());
                        }
                    },
                    4 => {
                        let pseudo = vault
                            .assignment_for(person.id)
                            .map(|a| format!("{}, {}", a.last_name, a.first_name))
                            .unwrap_or_default();
                        widgets::cell_mono_line(ui, p, pseudo);
                    }
                    _ => {
                        // Lernende einer Klasse werden über die Klasse verwaltet
                        if class.is_none() && widgets::link_button(ui, p, "löschen").clicked() {
                            to_delete = Some(person.id);
                        }
                    }
                }
            });
            if let Some(id) = to_delete {
                vault.persons.retain(|x| x.id != id);
                vault.pseudonyms.retain(|a| a.person_id != id);
                dirty = true;
                self.set_status(Status::Neutral, "Person entfernt.");
            }
        }
        ui.add_space(8.0);

        // Einzelne Person nur für dieses Projekt
        ui.label(RichText::new("Einzelne Person nur für dieses Projekt").font(widgets::medium(13.0)).color(p.muted));
        let add = person_form(ui, p, mode, [&mut self.pf_last, &mut self.pf_first, &mut self.pf_email, &mut self.pf_matr]);
        if add {
            if let Some(person) = take_person(&mut self.pf_first, &mut self.pf_last, &mut self.pf_email, &mut self.pf_matr) {
                vault.persons.push(person);
                dirty = true;
                self.set_status(Status::Success, "Person hinzugefügt.");
            }
        }

        // Einzelpersonen (z.B. aus älteren Projekten) als Klasse übernehmen
        let singles: Vec<Person> = vault.persons.iter().filter(|x| vault.class_of(x.id, &self.roster).is_none()).cloned().collect();
        let mut promote = false;
        if singles.len() >= 2 {
            ui.add_space(8.0);
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 36.0), Layout::left_to_right(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{} Personen ohne Klasse:", singles.len())).font(widgets::body(13.0)).color(p.muted),
                );
                ui.allocate_ui_with_layout(vec2(220.0, 36.0), Layout::top_down(Align::Min), |ui| {
                    if widgets::text_field(ui, p, &mut self.promote_name, "Name der Klasse", false).lost_focus() && enter {
                        promote = true;
                    }
                });
                if Button::secondary("Als Klasse speichern").show(ui, p).clicked() {
                    promote = true;
                }
            });
        }
        if promote {
            let name = self.promote_name.trim().to_string();
            if name.is_empty() {
                self.set_status(Status::Error, "Bitte einen Namen für die Klasse eingeben.");
            } else if self.roster.classes.iter().any(|c| c.name.eq_ignore_ascii_case(&name)) {
                self.set_status(Status::Error, format!("Die Klasse «{name}» gibt es schon."));
            } else {
                // gleiche Personen-IDs: die Pseudonyme dieses Projekts bleiben gültig
                let n = singles.len();
                let id = self.roster.add_class_with(name.clone(), singles);
                match store.save_roster(&self.roster) {
                    Ok(()) => {
                        vault.class_ids.push(id);
                        dirty = true;
                        self.promote_name.clear();
                        self.set_status(Status::Success, format!("Klasse «{name}» mit {n} Lernenden gespeichert."));
                    }
                    Err(e) => {
                        self.roster.remove_class(id);
                        self.set_status(Status::Error, format!("Klassen speichern fehlgeschlagen: {e}"));
                    }
                }
            }
        }

        // Nur speichern, wenn sich etwas geändert hat (nicht in jedem Frame).
        if vault.ensure_pseudonyms() || dirty {
            match store.save_project(&vault) {
                Ok(()) => self.vault = Some(vault),
                Err(e) => self.set_status(Status::Error, format!("Speichern fehlgeschlagen: {e}")),
            }
        }
    }

    // ------------------------------------------------------------ Anonymisieren

    fn ui_anonymize(&mut self, ui: &mut Ui, p: &Palette, mode: Mode) {
        let Some(vault) = self.vault.clone() else {
            widgets::empty_state(ui, p, Icon::Folder, "Kein Projekt ausgewählt — links ein Projekt wählen, sonst fehlen die Pseudonyme.");
            return;
        };
        ui.spacing_mut().item_spacing.y = 2.0;
        widgets::h2(ui, p, &vault.name);
        widgets::lede(ui, p, "Vier Schritte, von der Anonymisierung bis zurück zu den Namen");
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.add_space(8.0);

        let mut do_anon = false;
        let mut do_deanon = false;
        let mut do_export = false;
        let mut do_deanon_run = false;

        card_row(
            ui,
            "row-1",
            |ui, min_h| {
                widgets::section_card(ui, p, mode, 1, "Anonymisieren", "Echtname → Pseudonym", min_h, |ui| {
                    file_picker(ui, p, "Prüfungen auswählen…", &mut self.anon_files);
                    folder_picker(ui, p, "Zielordner wählen…", &mut self.out_dir_anon);
                    let ready = !self.anon_files.is_empty() && self.out_dir_anon.is_some();
                    do_anon = card_action(ui, p, "Anonymisieren starten", ready);
                })
            },
            |ui, min_h| {
                widgets::section_card(ui, p, mode, 2, "De-Anonymisieren", "Pseudonym → Echtname", min_h, |ui| {
                    file_picker(ui, p, "Dateien auswählen…", &mut self.deanon_files);
                    folder_picker(ui, p, "Zielordner wählen…", &mut self.out_dir_deanon);
                    let ready = !self.deanon_files.is_empty() && self.out_dir_deanon.is_some();
                    do_deanon = card_action(ui, p, "De-Anonymisieren starten", ready);
                })
            },
        );
        ui.add_space(8.0);
        // Karte 4 teilt sich den Zielordner mit Karte 2: eigene Kopie für die Anzeige, danach zurückschreiben.
        let mut deanon_target = self.out_dir_deanon.clone();
        card_row(
            ui,
            "row-2",
            |ui, min_h| {
                widgets::section_card(ui, p, mode, 3, "Lauf exportieren", "Ordner für die KI", min_h, |ui| {
                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.y = 6.0;
                        widgets::field_label(ui, p, "Bezeichnung");
                        widgets::text_field(ui, p, &mut self.run_label, "z.B. Klassenarbeit 2026-06-01", false);
                        widgets::help(ui, p, "Nur fürs Protokoll. Der Export-Ordner und die Dateien bekommen neutrale Namen.");
                    });
                    ui.add_space(4.0);
                    file_picker(ui, p, "Prüfungen auswählen…", &mut self.run_files);
                    folder_picker(ui, p, "Export-Oberordner…", &mut self.run_root);
                    let ready = !self.run_files.is_empty() && self.run_root.is_some();
                    do_export = card_action(ui, p, "Lauf exportieren", ready);
                })
            },
            |ui, min_h| {
                widgets::section_card(ui, p, mode, 4, "Lauf-Ordner de-anonymisieren", "Nach der KI-Bewertung", min_h, |ui| {
                    folder_picker(ui, p, "Lauf-Ordner wählen…", &mut self.deanon_folder);
                    folder_picker(ui, p, "Zielordner wählen…", &mut deanon_target);
                    let ready = self.deanon_folder.is_some() && deanon_target.is_some();
                    do_deanon_run = card_action(ui, p, "Lauf de-anonymisieren", ready);
                })
            },
        );
        self.out_dir_deanon = deanon_target;

        if do_anon {
            self.run_files(&vault, true);
        }
        if do_deanon {
            self.run_files(&vault, false);
        }
        if do_export {
            self.export_run(&vault);
        }
        if do_deanon_run {
            self.deanon_run(&vault);
        }

        if !self.results.is_empty() {
            ui.add_space(8.0);
            theme::card(p, mode).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    widgets::h3(ui, p, "Ergebnisse");
                    ui.label(RichText::new("neueste oben").font(widgets::body(13.0)).color(p.muted));
                });
                ui.add_space(4.0);
                for item in &self.results {
                    widgets::message_with(ui, p, item.kind, |ui, fg| {
                        ui.add(egui::Label::new(RichText::new(&item.head).font(widgets::semibold(14.0)).color(fg)).wrap());
                        ui.add(egui::Label::new(RichText::new(&item.detail).font(widgets::body(14.0)).color(fg)).wrap());
                    });
                }
            });
        }
    }

    fn run_files(&mut self, vault: &ProjectVault, anonymize: bool) {
        let (files, out_dir) = if anonymize {
            (self.anon_files.clone(), self.out_dir_anon.clone())
        } else {
            (self.deanon_files.clone(), self.out_dir_deanon.clone())
        };
        let Some(out_dir) = out_dir else {
            self.set_status(Status::Error, "Bitte zuerst einen Zielordner wählen.");
            return;
        };
        if files.is_empty() {
            self.set_status(Status::Error, "Bitte zuerst Dateien auswählen.");
            return;
        }
        self.review.clear();
        let mapping = if anonymize {
            mapping::anonymize_mapping(vault)
        } else {
            mapping::deanonymize_mapping(vault)
        };
        // Tippfehler-Suche nur für Personen, nicht für Klassennamen und Begriffe
        let scan_mapping = mapping::person_mapping(vault, anonymize);
        let suffix = if anonymize { "_anonym" } else { "_deanonym" };
        let mut any_error = false;
        let mut any_residual = false;
        for f in &files {
            let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "datei".into());
            let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            // auch der Dateiname wird übersetzt («Müller_Anna.docx» verrät sonst den Namen)
            let out = out_dir.join(format!("{}{suffix}.{ext}", mapping::output_stem(&stem, &mapping)));
            // Ok((Meldung, hat PDF-Residuals))
            let result: Result<(String, bool), String> = match ext.as_str() {
                "docx" => docx::process_docx(f, &out, &mapping)
                    .map(|s| (format!("{} Ersetzungen ({})", s.replacements, fmt_per_identifier(&s.per_identifier)), false))
                    .map_err(|e| e.to_string()),
                "pdf" => pdf::process_pdf(f, &out, &mapping)
                    .map(|s| {
                        let mut msg = format!("{} Ersetzungen ({})", s.replacements, fmt_per_identifier(&s.per_identifier));
                        let residual = !s.residual_matches.is_empty();
                        if residual {
                            msg.push_str(&format!(" — ACHTUNG {} Treffer über Textobjekte verteilt: {}", s.residual_matches.len(), s.residual_matches.join(", ")));
                        }
                        (msg, residual)
                    })
                    .map_err(|e| e.to_string()),
                other => Err(format!("unbekanntes Format .{other}")),
            };
            match result {
                Ok((msg, residual)) => {
                    any_residual |= residual;
                    let kind = if residual { Msg::Warning } else { Msg::Success };
                    self.push_result(kind, format!("{}  →  {}", file_name(f), file_name(&out)), msg);
                }
                Err(e) => {
                    any_error = true;
                    self.push_result(Msg::Error, file_name(f), format!("FEHLER: {e}"));
                }
            }
            // Fuzzy-Scan: Tippfehler-Varianten von Namen in die Review-Liste
            let scan: Result<Vec<FuzzyHit>, String> = match ext.as_str() {
                "docx" => docx::scan_fuzzy_docx(f, &scan_mapping).map_err(|e| e.to_string()),
                "pdf" => pdf::scan_fuzzy_pdf(f, &scan_mapping).map_err(|e| e.to_string()),
                _ => Ok(Vec::new()),
            };
            if let Ok(hits) = scan {
                for hit in hits {
                    self.review.push(ReviewItem { input: f.clone(), anonymize, hit, selected: true });
                }
            }
        }
        let kind = if any_error {
            Status::Error
        } else if any_residual || !self.review.is_empty() {
            Status::Warning
        } else {
            Status::Success
        };
        self.set_status(
            kind,
            format!("Fertig: {} Dateien verarbeitet, {} unscharfe Treffer in Review.", files.len(), self.review.len()),
        );
    }

    // ------------------------------------------------------------ Review

    fn ui_review(&mut self, ui: &mut Ui, p: &Palette) {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            let (rect, _) = ui.allocate_exact_size(vec2(24.0, 28.0), Sense::hover());
            widgets::paint_icon(ui.painter(), Icon::Alert, Rect::from_center_size(rect.center(), vec2(24.0, 24.0)), p.warning, 1.8);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                widgets::h2(ui, p, "Unscharfe Treffer — bitte prüfen");
                widgets::lede(ui, p, "Unscharfe Treffer (Tippfehler), die das exakte Matching nicht fand. Ausgewählte werden übernommen und die Dateien neu verarbeitet.");
            });
        });
        ui.add_space(8.0);
        if self.review.is_empty() {
            widgets::empty_state(ui, p, Icon::Search, "Keine offenen Treffer — nach einem Lauf erscheinen hier unscharfe Namens-Varianten.");
            return;
        }
        let n_sel = self.review.iter().filter(|r| r.selected).count();
        let n_all = self.review.len();
        let mut apply = false;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 36.0), Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            if Button::secondary("Alle auswählen").small().show(ui, p).clicked() {
                for r in &mut self.review {
                    r.selected = true;
                }
            }
            if Button::secondary("Alle abwählen").small().show(ui, p).clicked() {
                for r in &mut self.review {
                    r.selected = false;
                }
            }
            ui.label(RichText::new(format!("{n_sel} von {n_all} ausgewählt")).font(widgets::body(13.0)).color(p.muted));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                apply = Button::primary("Ausgewählte übernehmen").enabled(n_sel > 0).show(ui, p).clicked();
            });
        });
        ui.add_space(8.0);

        let cols = [
            Col::fixed("", 40.0),
            Col::flex("Fundstelle", 1.85),
            Col::flex("Kontext", 1.9),
            Col::flex("Treffer → Ersetzung", 1.35),
            Col::fixed("Ähnlichkeit", 100.0).right(),
        ];
        widgets::table(ui, p, &cols, self.review.len(), |ui, r, c| {
            let item = &mut self.review[r];
            match c {
                0 => {
                    let label = format!("{} → {}", item.hit.matched, item.hit.replacement);
                    widgets::checkbox(ui, p, &mut item.selected, &label);
                }
                1 => widgets::cell_wrap(ui, p, &item.hit.location),
                2 => context_cell(ui, p, &item.hit.context, &item.hit.matched),
                3 => {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        ui.label(RichText::new(&item.hit.matched).font(widgets::body(14.0)).color(p.text));
                        widgets::icon(ui, Icon::ArrowRight, p.muted);
                        ui.label(RichText::new(&item.hit.replacement).font(widgets::semibold(14.0)).color(p.text));
                    });
                }
                _ => widgets::cell(ui, p, format!("{:.0} %", item.hit.similarity * 100.0)),
            }
        });

        if apply {
            self.apply_review();
        }
    }

    // ------------------------------------------------------------ Protokoll & Läufe

    fn export_run(&mut self, vault: &ProjectVault) {
        let Some(store) = self.store.clone() else { return };
        let Some(root) = self.run_root.clone() else {
            self.set_status(Status::Error, "Bitte Export-Oberordner wählen.");
            return;
        };
        if self.run_files.is_empty() {
            self.set_status(Status::Error, "Bitte Prüfungen für den Lauf auswählen.");
            return;
        }
        let label = if self.run_label.trim().is_empty() { "Lauf" } else { self.run_label.trim() };
        match runs::export_anonymized(&store, vault.id, label, &self.run_files, &root) {
            Ok((rec, folder)) => {
                let total: usize = rec.entries.iter().map(|e| e.replacements).sum();
                let tokens: Vec<String> = rec.entries.iter().flat_map(|e| e.replaced_tokens.clone()).collect();
                let residuals = rec.entries.iter().any(|e| !e.residuals.is_empty());
                self.push_result(
                    if residuals { Msg::Warning } else { Msg::Success },
                    format!("Lauf ‹{}› exportiert", rec.label),
                    format!(
                        "{} Dateien, {} Ersetzungen  →  {} · Tokens: {}",
                        rec.entries.len(),
                        total,
                        widgets::display_path(&folder),
                        tokens.join(", ")
                    ),
                );
                self.set_status(
                    Status::Success,
                    format!("Lauf ‹{}› exportiert — {} Dateien, {} Ersetzungen", rec.label, rec.entries.len(), total),
                );
                // Fuzzy-Review für die Lauf-Dateien
                let mapping = mapping::person_mapping(vault, true);
                for f in self.run_files.clone() {
                    let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                    let scan = match ext.as_str() {
                        "docx" => docx::scan_fuzzy_docx(&f, &mapping).map_err(|e| e.to_string()),
                        "pdf" => pdf::scan_fuzzy_pdf(&f, &mapping).map_err(|e| e.to_string()),
                        _ => Ok(Vec::new()),
                    };
                    if let Ok(hits) = scan {
                        for hit in hits {
                            self.review.push(ReviewItem { input: f.clone(), anonymize: true, hit, selected: true });
                        }
                    }
                }
            }
            Err(e) => self.set_status(Status::Error, format!("Export fehlgeschlagen: {e}")),
        }
    }

    fn deanon_run(&mut self, vault: &ProjectVault) {
        let Some(store) = self.store.clone() else { return };
        let Some(folder) = self.deanon_folder.clone() else {
            self.set_status(Status::Error, "Bitte Lauf-Ordner wählen.");
            return;
        };
        let Some(out_dir) = self.out_dir_deanon.clone() else {
            self.set_status(Status::Error, "Bitte Zielordner wählen.");
            return;
        };
        let label = if self.run_label.trim().is_empty() { "Lauf zurück" } else { self.run_label.trim() };
        match runs::deanonymize_folder(&store, vault.id, label, &folder, &out_dir) {
            Ok((rec, out_folder)) => {
                let total: usize = rec.entries.iter().map(|e| e.replacements).sum();
                self.push_result(
                    Msg::Success,
                    format!("Lauf ‹{}› {}", rec.label, rec.direction.as_str()),
                    format!(
                        "{} Dateien, {} Ersetzungen aus {}  →  {}",
                        rec.entries.len(),
                        total,
                        widgets::display_path(&folder),
                        widgets::display_path(&out_folder)
                    ),
                );
                self.set_status(Status::Success, "De-Anonymisierung fertig — Lauf im Protokoll.");
            }
            Err(e) => self.set_status(Status::Error, format!("De-Anonymisierung fehlgeschlagen: {e}")),
        }
    }

    fn ui_log(&mut self, ui: &mut Ui, p: &Palette) {
        ui.spacing_mut().item_spacing.y = 2.0;
        widgets::h2(ui, p, "Nur Pseudonyme, nie Klarnamen");
        widgets::lede(ui, p, "Verschlüsselt gespeichert. Enthält nur Pseudonyme, Dateinamen und Zähler — keine Klarnamen.");
        ui.spacing_mut().item_spacing.y = 8.0;
        ui.add_space(8.0);
        let Some(store) = self.store.clone() else {
            widgets::empty_state(ui, p, Icon::Lock, "Kein Tresor geladen.");
            return;
        };
        let run_list = match store.list_runs() {
            Ok(list) => list,
            Err(e) => {
                widgets::message(ui, p, Msg::Error, &format!("Protokoll nicht lesbar: {e}"));
                return;
            }
        };
        if run_list.is_empty() {
            widgets::empty_state(ui, p, Icon::File, "Noch keine Läufe protokolliert.");
            return;
        }
        // Standardbreite: Datum/Uhrzeit und Label/Projekt je zweizeilig, damit nichts mitten
        // im Wort umbricht. Ab ca. 1150 px Fensterbreite: alle Spalten einzeln wie im Screen.
        let compact = ui.available_width() < 880.0;
        let newest_first = |row: usize| &run_list[run_list.len() - 1 - row];
        let direction = |ui: &mut Ui, r: &runs::RunRecord| {
            let ic = if r.direction == runs::RunDirection::Anonymize { Icon::ArrowRight } else { Icon::ArrowLeft };
            widgets::tag(ui, p, Some(ic), r.direction.as_str());
        };
        let files = |r: &runs::RunRecord| r.entries.len().to_string();
        let replacements = |r: &runs::RunRecord| r.entries.iter().map(|e| e.replacements).sum::<usize>().to_string();
        let tokens = |r: &runs::RunRecord| {
            let t: Vec<&str> = r.entries.iter().flat_map(|e| e.replaced_tokens.iter().map(|s| s.as_str())).collect();
            t.join(", ")
        };
        let second_line = |ui: &mut Ui, text: String| {
            ui.add(egui::Label::new(RichText::new(text).font(widgets::body(13.0)).color(p.muted)).truncate());
        };
        if compact {
            let cols = [
                Col::fixed("Zeit", 100.0),
                Col::fixed("Richtung", 148.0),
                Col::flex("Label · Projekt", 1.6),
                Col::fixed("Dateien", 80.0).right(),
                Col::fixed("Ersetzungen", 108.0).right(),
                Col::flex("Tokens", 1.0),
            ];
            widgets::table(ui, p, &cols, run_list.len(), |ui, row, c| {
                let r = newest_first(row);
                match c {
                    0 => {
                        widgets::cell(ui, p, r.created_at.format("%d.%m.%Y").to_string());
                        second_line(ui, r.created_at.format("%H:%M").to_string());
                    }
                    1 => direction(ui, r),
                    2 => {
                        widgets::cell(ui, p, &r.label);
                        second_line(ui, r.project_name.clone());
                    }
                    3 => widgets::cell(ui, p, files(r)),
                    4 => widgets::cell(ui, p, replacements(r)),
                    _ => widgets::cell_mono(ui, p, tokens(r)),
                }
            });
        } else {
            let cols = [
                Col::fixed("Zeit", 140.0),
                Col::fixed("Richtung", 148.0),
                Col::flex("Label", 1.0),
                Col::flex("Projekt", 1.0),
                Col::fixed("Dateien", 80.0).right(),
                Col::fixed("Ersetzungen", 108.0).right(),
                Col::flex("Tokens", 1.0),
            ];
            widgets::table(ui, p, &cols, run_list.len(), |ui, row, c| {
                let r = newest_first(row);
                match c {
                    0 => widgets::cell(ui, p, r.created_at.format("%d.%m.%Y %H:%M").to_string()),
                    1 => direction(ui, r),
                    2 => widgets::cell_wrap(ui, p, &r.label),
                    3 => widgets::cell_wrap(ui, p, &r.project_name),
                    4 => widgets::cell(ui, p, files(r)),
                    5 => widgets::cell(ui, p, replacements(r)),
                    _ => widgets::cell_mono(ui, p, tokens(r)),
                }
            });
        }
    }

    fn apply_review(&mut self) {
        let Some(vault) = self.vault.clone() else {
            self.set_status(Status::Error, "Kein Projekt ausgewählt.");
            return;
        };
        // bestätigte Treffer pro Datei gruppieren
        let mut groups: std::collections::HashMap<(PathBuf, bool), Vec<(String, String)>> = Default::default();
        for item in &self.review {
            if item.selected {
                groups
                    .entry((item.input.clone(), item.anonymize))
                    .or_default()
                    .push((item.hit.matched.clone(), item.hit.replacement.clone()));
            }
        }
        if groups.is_empty() {
            self.set_status(Status::Neutral, "Nichts ausgewählt.");
            return;
        }
        let mut applied = 0usize;
        for ((input, anonymize), extra) in groups {
            let base = if anonymize {
                mapping::anonymize_mapping(&vault)
            } else {
                mapping::deanonymize_mapping(&vault)
            };
            let mut mapping = base;
            mapping.extend(extra);
            let out_dir = if anonymize { &self.out_dir_anon } else { &self.out_dir_deanon };
            let Some(out_dir) = out_dir.clone() else { continue };
            let stem = input.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "datei".into());
            let ext = input.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let suffix = if anonymize { "_anonym" } else { "_deanonym" };
            let out = out_dir.join(format!("{}{suffix}.{ext}", mapping::output_stem(&stem, &mapping)));
            let result: Result<String, String> = match ext.as_str() {
                "docx" => docx::process_docx(&input, &out, &mapping)
                    .map(|s| format!("{} Ersetzungen gesamt", s.replacements))
                    .map_err(|e| e.to_string()),
                "pdf" => pdf::process_pdf(&input, &out, &mapping)
                    .map(|s| format!("{} Ersetzungen gesamt", s.replacements))
                    .map_err(|e| e.to_string()),
                _ => Err("unbekanntes Format".into()),
            };
            match result {
                Ok(msg) => self.push_result(
                    Msg::Success,
                    format!("{}  →  {}", file_name(&input), file_name(&out)),
                    format!("Review angewendet: {msg}"),
                ),
                Err(e) => self.push_result(Msg::Error, file_name(&input), format!("FEHLER: {e}")),
            }
            applied += 1;
        }
        // übernommene Treffer aus der Review-Liste entfernen
        self.review.retain(|r| !r.selected);
        self.set_status(Status::Success, format!("Review übernommen: {applied} Dateien neu verarbeitet."));
    }
}

// ---------------------------------------------------------------- Helfer

/// Trennzeichen-Dropdown für den Listen-Import.
fn delim_combo(ui: &mut Ui, p: &Palette, delim: &mut char) {
    let label = if *delim == '\t' { "Tab".to_string() } else { delim.to_string() };
    egui::ComboBox::from_id_salt("delim")
        .width(64.0)
        .selected_text(RichText::new(label).font(widgets::body(14.0)).color(p.text))
        .show_ui(ui, |ui| {
            ui.selectable_value(delim, ';', ";");
            ui.selectable_value(delim, ',', ",");
            ui.selectable_value(delim, '\t', "Tab");
        });
}

/// Formular-Karte «Person hinzufügen»: Felder [Nachname, Vorname, E-Mail, Matrikel].
/// Rückgabe: true bei Klick auf «Hinzufügen» oder Enter in einem Feld.
fn person_form(ui: &mut Ui, p: &Palette, mode: Mode, values: [&mut String; 4]) -> bool {
    let mut add = false;
    theme::card(p, mode).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let gap = 12.0;
        let btn_w = 124.0;
        let fields_w = (ui.available_width() - btn_w - 4.0 * gap).max(200.0);
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.allocate_ui_with_layout(vec2(ui.available_width(), 60.0), Layout::left_to_right(Align::Max), |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            let labels = [("Nachname", 1.0), ("Vorname", 1.0), ("E-Mail (optional)", 1.3), ("Matrikel (optional)", 1.0)];
            for (value, (label, weight)) in values.into_iter().zip(labels) {
                let w = (fields_w * weight / 4.3).floor();
                ui.allocate_ui_with_layout(vec2(w, 60.0), Layout::top_down(Align::Min), |ui| {
                    ui.set_width(w);
                    ui.spacing_mut().item_spacing.y = 6.0;
                    widgets::field_label(ui, p, label);
                    if widgets::text_field(ui, p, value, "", false).lost_focus() && enter {
                        add = true;
                    }
                });
            }
            ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
                if Button::secondary("Hinzufügen").show(ui, p).clicked() {
                    add = true;
                }
            });
        });
    });
    add
}

/// Baut aus den Formularfeldern eine Person und leert sie. None, wenn kein Name eingegeben ist.
fn take_person(first: &mut String, last: &mut String, email: &mut String, matr: &mut String) -> Option<Person> {
    if first.trim().is_empty() && last.trim().is_empty() {
        return None;
    }
    let mut person = Person::new(first.trim(), last.trim());
    if !email.trim().is_empty() {
        person.emails.push(email.trim().to_string());
    }
    if !matr.trim().is_empty() {
        person.matriculation = Some(matr.trim().to_string());
    }
    first.clear();
    last.clear();
    email.clear();
    matr.clear();
    Some(person)
}

/// Zwei Karten nebeneinander, gleich hoch (Höhe aus dem vorigen Frame).
/// Die Closures bekommen die Mindest-Inhaltshöhe und liefern ihre natürliche Höhe zurück.
fn card_row(ui: &mut Ui, key: &str, left: impl FnOnce(&mut Ui, f32) -> f32, right: impl FnOnce(&mut Ui, f32) -> f32) {
    let id = ui.id().with(key);
    let prev: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    let gap = 16.0;
    let col_w = ((ui.available_width() - gap) / 2.0).floor();
    let mut heights = (0.0_f32, 0.0_f32);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        ui.allocate_ui_with_layout(vec2(col_w, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(col_w);
            heights.0 = left(ui, prev);
        });
        ui.allocate_ui_with_layout(vec2(col_w, 0.0), Layout::top_down(Align::Min), |ui| {
            ui.set_width(col_w);
            heights.1 = right(ui, prev);
        });
    });
    let now = heights.0.max(heights.1);
    if (now - prev).abs() > 0.5 {
        ui.data_mut(|d| d.insert_temp(id, now));
        ui.ctx().request_repaint();
    }
}

/// Datei-Picker einer Karte: Button, «Liste leeren», die ersten Dateien als Mono-Pfade.
fn file_picker(ui: &mut Ui, p: &Palette, label: &str, files: &mut Vec<PathBuf>) {
    ui.horizontal_wrapped(|ui| {
        if Button::secondary(label).icon(Icon::File).show(ui, p).clicked() {
            if let Some(picked) = rfd::FileDialog::new().add_filter("Dokumente", &["docx", "pdf"]).pick_files() {
                for f in picked {
                    if !files.contains(&f) {
                        files.push(f);
                    }
                }
            }
        }
        if Button::secondary("Liste leeren").small().enabled(!files.is_empty()).show(ui, p).clicked() {
            files.clear();
        }
    });
    if files.is_empty() {
        widgets::muted(ui, p, "Noch keine Dateien gewählt (.docx, .pdf)");
    } else {
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let shown = files.len().min(2);
            for f in &files[..shown] {
                widgets::path_line(ui, p, Icon::File, &widgets::display_path(f));
            }
            if files.len() > shown {
                let rest = files.len() - shown;
                widgets::muted(ui, p, if rest == 1 { "und 1 weitere Datei".to_string() } else { format!("und {rest} weitere Dateien") });
            }
        });
    }
    ui.add_space(4.0);
}

/// Ordner-Picker einer Karte: Button + gewählter Pfad (Mono, mittig gekürzt).
fn folder_picker(ui: &mut Ui, p: &Palette, label: &str, dir: &mut Option<PathBuf>) {
    if Button::secondary(label).icon(Icon::Folder).show(ui, p).clicked() {
        if let Some(picked) = rfd::FileDialog::new().pick_folder() {
            *dir = Some(picked);
        }
    }
    match dir {
        Some(d) => widgets::path_line(ui, p, Icon::Folder, &widgets::display_path(d)),
        None => widgets::muted(ui, p, "Noch kein Ordner gewählt"),
    }
    ui.add_space(4.0);
}

/// Hauptaktion einer Karte, rechtsbündig; nur aktiv, wenn die Eingaben vollständig sind.
fn card_action(ui: &mut Ui, p: &Palette, label: &str, ready: bool) -> bool {
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 36.0), Layout::right_to_left(Align::Center), |ui| {
        Button::primary(label).enabled(ready).show(ui, p).clicked()
    })
    .inner
}

/// Kontext in Mono, der Treffer hervorgehoben (`.nm-mark`).
fn context_cell(ui: &mut Ui, p: &Palette, context: &str, matched: &str) {
    let font = widgets::mono(13.0);
    let plain = |ui: &mut Ui, s: &str| {
        if !s.is_empty() {
            ui.label(RichText::new(s).font(font.clone()).color(p.text));
        }
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        match (!matched.is_empty()).then(|| context.find(matched)).flatten() {
            Some(idx) => {
                plain(ui, &context[..idx]);
                let g = ui.painter().layout_no_wrap(matched.to_owned(), font.clone(), p.warning_text);
                let (rect, _) = ui.allocate_exact_size(vec2(g.size().x + 6.0, 20.0), Sense::hover());
                ui.painter().rect(
                    rect.shrink2(vec2(1.0, 0.0)),
                    CornerRadius::same(2),
                    p.warning_bg,
                    Stroke::new(1.0_f32, p.warning),
                    StrokeKind::Inside,
                );
                ui.painter().galley(rect.center() - g.size() / 2.0, g, p.warning_text);
                plain(ui, &context[idx + matched.len()..]);
            }
            None => plain(ui, context),
        }
    });
}

fn fmt_per_identifier(map: &std::collections::HashMap<String, usize>) -> String {
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort();
    entries
        .iter()
        .map(|(k, v)| format!("{k}: {v}"))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod shots;
