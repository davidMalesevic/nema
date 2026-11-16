//! egui-Oberfläche gemäss docs/UI Design (Screens Hell/Dunkel).
//! Dünne Schicht über nema-core — die gesamte Logik liegt in nema-core und ist dort getestet.

use crate::theme::{self, Mode, Palette};
use crate::widgets::{self, Btn, Status};
use eframe::egui;
use nema_core::crypto;
use nema_core::csv_import;
use nema_core::docx;
use nema_core::fuzzy::FuzzyHit;
use nema_core::mapping;
use nema_core::pdf;
use nema_core::person::Person;
use nema_core::project::{ProjectMeta, ProjectVault};
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

pub struct App {
    dir: PathBuf,
    screen: Screen,
    store: Option<VaultStore>,

    // Theme
    mode: Mode,
    logo_light: Option<egui::TextureHandle>,
    logo_dark: Option<egui::TextureHandle>,

    // Setup
    setup_keychain: bool,
    setup_pw1: String,
    setup_pw2: String,
    setup_msg: String,
    show_pw1: bool,
    show_pw2: bool,

    // Unlock
    unlock_pw: String,
    unlock_msg: String,
    show_pw_unlock: bool,

    // Main
    projects: Vec<ProjectMeta>,
    selected: Option<Uuid>,
    vault: Option<ProjectVault>,
    new_project: String,
    tab: Tab,
    status: String,
    status_kind: Status,
    show_delete_dialog: bool,

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
    /// (Titel, Detail) — neueste oben
    results: Vec<(String, String)>,
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
    ctx.load_texture(name, egui::ImageData::Color(std::sync::Arc::new(img)), Default::default())
}

fn file_name(f: &std::path::Path) -> String {
    f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| f.display().to_string())
}

impl App {
    pub fn new() -> Self {
        let dir = VaultStore::default_dir();
        let mut app = Self {
            dir,
            screen: Screen::Setup,
            store: None,
            mode: Mode::Light,
            logo_light: None,
            logo_dark: None,
            setup_keychain: crypto::keychain_backend_available(),
            setup_pw1: String::new(),
            setup_pw2: String::new(),
            setup_msg: String::new(),
            show_pw1: false,
            show_pw2: false,
            unlock_pw: String::new(),
            unlock_msg: String::new(),
            show_pw_unlock: false,
            projects: Vec::new(),
            selected: None,
            vault: None,
            new_project: String::new(),
            tab: Tab::Persons,
            status: String::new(),
            status_kind: Status::Neutral,
            show_delete_dialog: false,
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
        };
        if VaultStore::is_initialized(&app.dir) {
            match VaultStore::unlock(&app.dir) {
                Ok(store) => app.enter_main(store),
                Err(_) => app.screen = Screen::Unlock,
            }
        }
        app
    }

    fn enter_main(&mut self, store: VaultStore) {
        self.projects = store.list_projects().unwrap_or_default();
        self.store = Some(store);
        self.screen = Screen::Main;
        if let Some(id) = self.projects.first().map(|p| p.id) {
            self.select_project(id);
        }
    }

    fn select_project(&mut self, id: Uuid) {
        self.selected = Some(id);
        self.vault = self
            .store
            .as_ref()
            .and_then(|s| s.load_project(id).ok());
    }

    fn set_status(&mut self, kind: Status, text: impl Into<String>) {
        self.status_kind = kind;
        self.status = text.into();
    }

    fn ensure_textures(&mut self, ctx: &egui::Context) {
        if self.logo_light.is_none() {
            // navy-Icon für helle Flächen (nema-icon-light.png = Variante für Light Mode)
            self.logo_light = Some(png_texture(ctx, "nema-icon-light", include_bytes!("../../packaging/nema-icon-light.png")));
        }
        if self.logo_dark.is_none() {
            // mist-Icon für dunkle Flächen (nema-icon-dark.png = Variante für Dark Mode)
            self.logo_dark = Some(png_texture(ctx, "nema-icon-dark", include_bytes!("../../packaging/nema-icon-dark.png")));
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.mode = theme::system_mode(ctx);
        theme::apply(ctx, self.mode);
        self.ensure_textures(ctx);
        // komplettes Hintergrund-Layer (Panels decken u.U. nicht alles ab)
        let p0 = theme::palette(self.mode);
        ctx.layer_painter(egui::LayerId::background())
            .rect_filled(ctx.content_rect(), egui::CornerRadius::ZERO, p0.bg);
        match self.screen {
            Screen::Setup => self.ui_setup(ctx),
            Screen::Unlock => self.ui_unlock(ctx),
            Screen::Main => self.ui_main(ctx),
        }
    }
}

// ---------------------------------------------------------------- Setup & Unlock

impl App {
    /// Zentrierte Spalte (440 px) wie in den Screens.
    fn centered_column(ctx: &egui::Context, add_contents: impl FnOnce(&mut egui::Ui)) {
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add_space(32.0);
                ui.horizontal(|ui| {
                    let side = ((ui.available_width() - 440.0) / 2.0).max(0.0);
                    ui.add_space(side);
                    ui.allocate_ui_with_layout(
                        egui::vec2(440.0, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        add_contents,
                    );
                });
            });
        });
    }

    fn logo_image(&self, ui: &mut egui::Ui, size: f32) {
        let tex = if self.mode == Mode::Dark { &self.logo_dark } else { &self.logo_light };
        if let Some(t) = tex {
            ui.vertical_centered(|ui| {
                ui.image((t.id(), egui::vec2(size, size)));
            });
        }
    }

    /// Radio-Optionskarte (Setup).
    fn option_card(ui: &mut egui::Ui, p: &Palette, selected: bool, enabled: bool, title: &str, tag: Option<&str>, desc: &str) -> bool {
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 62.0), egui::Sense::click());
        if enabled && resp.hovered() {
            ui.painter().rect_filled(rect, egui::CornerRadius::same(10), p.hover);
        }
        let stroke = if selected { egui::Stroke::new(1.5_f32, p.text) } else { egui::Stroke::new(1.0_f32, p.border_input) };
        ui.painter().rect_stroke(rect, egui::CornerRadius::same(10), stroke, egui::StrokeKind::Inside);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
            let (rrect, _r) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
            let c = rrect.center();
            let radio_color = if enabled { p.text } else { p.muted };
            ui.painter().circle_stroke(c, 7.0, egui::Stroke::new(1.5_f32, radio_color));
            if selected && enabled {
                ui.painter().circle_filled(c, 3.5, p.primary);
            }
            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(title).font(widgets::semibold(15.0)).color(if enabled { p.text } else { p.muted }));
                    if let Some(tag) = tag {
                        egui::Frame::new()
                            .fill(p.sunken)
                            .stroke(egui::Stroke::new(1.0_f32, p.border))
                            .corner_radius(egui::CornerRadius::same(100))
                            .inner_margin(egui::Margin::symmetric(8, 2))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(tag).font(widgets::medium(12.0)).color(p.text));
                            });
                    }
                });
                ui.label(egui::RichText::new(desc).font(widgets::body(13.0)).color(p.muted));
            });
        });
        resp.clicked() && enabled
    }

    fn try_create_vault(&mut self) {
        if self.setup_pw1.len() < 8 {
            self.setup_msg = "Backup-Passwort muss mindestens 8 Zeichen haben".into();
        } else if self.setup_pw1 != self.setup_pw2 {
            self.setup_msg.clear();
        } else {
            match VaultStore::init(&self.dir, self.setup_keychain, Some(self.setup_pw1.as_str())) {
                Ok(store) => self.enter_main(store),
                Err(e) => self.setup_msg = format!("Fehler: {e}"),
            }
        }
    }

    fn ui_setup(&mut self, ctx: &egui::Context) {
        let p = theme::palette(self.mode);
        let kc = crypto::keychain_backend_available();
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let mut create = false;
        Self::centered_column(ctx, |ui| {
            self.logo_image(ui, 56.0);
            ui.add_space(16.0);
            ui.vertical_centered(|ui| widgets::h1(ui, &p, "Verschlüsselten Tresor anlegen"));
            ui.vertical_centered(|ui| {
                widgets::lede(ui, &p, "Verschlüsselten Tresor für die Namenslisten anlegen. Alle Namen bleiben lokal und verschlüsselt auf diesem Gerät.")
            });
            ui.add_space(20.0);

            if Self::option_card(ui, &p, self.setup_keychain, kc, "OS-Schlüsselbund", Some("Empfohlen"), "Der Tresor wird über das Betriebssystem entsperrt.") {
                self.setup_keychain = true;
            }
            ui.add_space(8.0);
            if Self::option_card(ui, &p, !self.setup_keychain, true, "Nur Backup-Passwort", None, "Das Passwort wird bei jedem Start verlangt.") {
                self.setup_keychain = false;
            }
            ui.add_space(16.0);

            widgets::field_label(ui, &p, "Backup-Passwort (min. 8 Zeichen)");
            widgets::help(ui, &p, "Fallback, falls der Schlüsselbund mal nicht verfügbar ist.");
            widgets::password_field(ui, &p, &mut self.setup_pw1, "Backup-Passwort", false, &mut self.show_pw1);
            ui.add_space(8.0);
            widgets::field_label(ui, &p, "Passwort wiederholen");
            let mismatch = !self.setup_pw2.is_empty() && self.setup_pw1 != self.setup_pw2;
            widgets::password_field(ui, &p, &mut self.setup_pw2, "Passwort wiederholen", mismatch, &mut self.show_pw2);
            if mismatch {
                widgets::error_line(ui, &p, "Passwörter stimmen nicht überein");
            }
            ui.add_space(16.0);

            if widgets::block_button(ui, &p, Btn::Primary, "Tresor erstellen").clicked() {
                create = true;
            }
            if !self.setup_msg.is_empty() {
                ui.add_space(8.0);
                widgets::message(ui, &p, widgets::Msg::Error, &self.setup_msg);
            }
            ui.add_space(16.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new(format!("Speicherort: {}", self.dir.display()))
                        .font(widgets::mono(13.0))
                        .color(p.muted),
                );
            });
        });
        if enter || create {
            self.try_create_vault();
        }
    }

    fn ui_unlock(&mut self, ctx: &egui::Context) {
        let p = theme::palette(self.mode);
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        let mut unlock_pw = false;
        let mut unlock_kc = false;
        Self::centered_column(ctx, |ui| {
            self.logo_image(ui, 56.0);
            ui.add_space(16.0);
            ui.vertical_centered(|ui| widgets::h1(ui, &p, "Tresor entsperren"));
            ui.vertical_centered(|ui| widgets::lede(ui, &p, "Alle Namen bleiben lokal und verschlüsselt auf diesem Gerät."));
            ui.add_space(20.0);
            if widgets::block_button(ui, &p, Btn::Secondary, "Per OS-Schlüsselbund entsperren").clicked() {
                unlock_kc = true;
            }
            if !self.unlock_msg.is_empty() {
                ui.add_space(12.0);
                widgets::message(ui, &p, widgets::Msg::Error, &self.unlock_msg);
            }
            ui.add_space(16.0);
            divider(ui, &p, "oder Backup-Passwort");
            ui.add_space(16.0);
            widgets::field_label(ui, &p, "Backup-Passwort");
            widgets::password_field(ui, &p, &mut self.unlock_pw, "Backup-Passwort", false, &mut self.show_pw_unlock);
            ui.add_space(16.0);
            if widgets::block_button(ui, &p, Btn::Primary, "Entsperren").clicked() {
                unlock_pw = true;
            }
        });
        if unlock_kc {
            match VaultStore::unlock(&self.dir) {
                Ok(store) => self.enter_main(store),
                Err(e) => self.unlock_msg = format!("Schlüsselbund nicht verfügbar ({e}) — bitte Backup-Passwort eingeben."),
            }
        }
        if unlock_pw || enter {
            match VaultStore::unlock_with_password(&self.dir, &self.unlock_pw) {
                Ok(store) => self.enter_main(store),
                Err(e) => self.unlock_msg = format!("{e}"),
            }
        }
    }
}

fn divider(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.horizontal(|ui| {
        let avail = ui.available_width();
        let line_w = ((avail - 160.0) / 2.0).max(10.0);
        let (r1, _) = ui.allocate_exact_size(egui::vec2(line_w, 18.0), egui::Sense::hover());
        ui.painter().line_segment([r1.left_center(), r1.right_center()], egui::Stroke::new(1.0_f32, p.border));
        ui.label(egui::RichText::new(text).font(widgets::body(13.0)).color(p.muted));
        let (r2, _) = ui.allocate_exact_size(egui::vec2(line_w, 18.0), egui::Sense::hover());
        ui.painter().line_segment([r2.left_center(), r2.right_center()], egui::Stroke::new(1.0_f32, p.border));
    });
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
        let tab = self.tab;
        let n_review = self.review.len();

        // Tab-Leiste oben
        let mut new_tab = None;
        egui::TopBottomPanel::top("nema-tabs")
            .frame(egui::Frame::new().fill(p.surface).inner_margin(egui::Margin::symmetric(24, 0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if widgets::tab(ui, &p, tab == Tab::Persons, "Personen").clicked() {
                        new_tab = Some(Tab::Persons);
                    }
                    if widgets::tab(ui, &p, tab == Tab::Anonymize, "Anonymisieren").clicked() {
                        new_tab = Some(Tab::Anonymize);
                    }
                    ui.horizontal(|ui| {
                        if widgets::tab(ui, &p, tab == Tab::Review, "Review").clicked() {
                            new_tab = Some(Tab::Review);
                        }
                        if n_review > 0 {
                            widgets::badge(ui, &p, n_review);
                        }
                    });
                    if widgets::tab(ui, &p, tab == Tab::Log, "Protokoll").clicked() {
                        new_tab = Some(Tab::Log);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        widgets::vault_pill(ui, &p, mode_label);
                    });
                });
                let r = ui.min_rect();
                ui.painter().line_segment([r.left_bottom(), r.right_bottom()], egui::Stroke::new(1.0_f32, p.border));
            });
        if let Some(t) = new_tab {
            self.tab = t;
        }

        // Seitenleiste: Projekte
        egui::SidePanel::left("nema-projects")
            .exact_width(220.0)
            .resizable(false)
            .frame(egui::Frame::new().fill(p.surface).inner_margin(egui::Margin::same(16)))
            .show(ctx, |ui| {
                let r = ui.max_rect();
                ui.painter().line_segment([r.right_top(), r.right_bottom()], egui::Stroke::new(1.0_f32, p.border));
                ui.label(egui::RichText::new("PROJEKTE").font(widgets::semibold(13.0)).color(p.muted));
                ui.add_space(4.0);
                let mut to_select = None;
                for proj in &self.projects {
                    if widgets::project_item(ui, &p, &proj.name, self.selected == Some(proj.id)).clicked() {
                        to_select = Some(proj.id);
                    }
                }
                if let Some(id) = to_select {
                    self.select_project(id);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    if self.selected.is_some() && widgets::small_button(ui, &p, Btn::Danger, "Projekt löschen").clicked() {
                        self.show_delete_dialog = true;
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let w = (ui.available_width() - 76.0).max(60.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_project)
                                .hint_text("Neues Projekt")
                                .desired_width(w)
                                .font(widgets::body(15.0)),
                        );
                        if widgets::small_button(ui, &p, Btn::Secondary, "Anlegen").clicked() {
                            let name = self.new_project.trim().to_string();
                            if !name.is_empty() {
                                if let Some(store) = self.store.clone() {
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
                            }
                        }
                    });
                });
            });

        // Inhalt
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin::same(24)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match tab {
                    Tab::Persons => self.ui_persons(ui, &p, mode),
                    Tab::Anonymize => self.ui_anonymize(ui, &p, mode),
                    Tab::Review => self.ui_review(ui, &p),
                    Tab::Log => self.ui_log(ui, &p),
                });
            });

        // Statusleiste unten
        let status_text = if self.status.is_empty() { "Bereit.".to_string() } else { self.status.clone() };
        widgets::status_bar(ctx, &p, self.status_kind, &status_text);

        // Dialog «Projekt löschen»
        if self.show_delete_dialog {
            let (name, count) = self
                .vault
                .as_ref()
                .map(|v| (v.name.clone(), v.persons.len()))
                .unwrap_or_default();
            let body = format!("Das Projekt «{name}» mit {count} Personen wird gelöscht. Das lässt sich nicht rückgängig machen.");
            if let Some(confirmed) = widgets::confirm_dialog(ctx, &p, mode, &mut self.show_delete_dialog, "Projekt löschen?", &body, "Projekt löschen") {
                if confirmed {
                    if let Some(id) = self.selected {
                        if let Some(store) = self.store.clone() {
                            if store.delete_project(id).is_ok() {
                                self.projects = store.list_projects().unwrap_or_default();
                                self.selected = None;
                                self.vault = None;
                                self.set_status(Status::Success, "Projekt gelöscht.");
                            }
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------ Personen

    fn ui_persons(&mut self, ui: &mut egui::Ui, p: &Palette, _mode: Mode) {
        let Some(vault) = self.vault.clone() else {
            widgets::empty_state(ui, p, widgets::Icon::Folder, "Kein Projekt ausgewählt — links ein Projekt anlegen oder wählen.");
            return;
        };
        let Some(store) = self.store.clone() else { return };
        let mut vault = vault;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                widgets::h2(ui, p, &vault.name);
                widgets::muted(ui, p, format!("{} Personen mit Pseudonymen", vault.persons.len()));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::button(ui, p, Btn::Secondary, "Namensliste importieren…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Listen", &["csv", "txt", "tsv"])
                        .pick_file()
                    {
                        match std::fs::read_to_string(&path) {
                            Ok(text) => match csv_import::parse(&text, self.delim) {
                                Ok(inputs) => {
                                    let n = inputs.len();
                                    for i in inputs {
                                        vault.persons.push(i.into_person());
                                    }
                                    self.set_status(Status::Success, format!("Namensliste importiert — {n} Personen."));
                                }
                                Err(e) => self.set_status(Status::Error, format!("Importfehler: {e}")),
                            },
                            Err(e) => self.set_status(Status::Error, format!("Lesefehler: {e}")),
                        }
                    }
                }
                ui.horizontal(|ui| {
                    widgets::muted(ui, p, "Trennzeichen:");
                    let delim_label = if self.delim == '\t' { "Tab".to_string() } else { self.delim.to_string() };
                    egui::ComboBox::from_id_salt("delim")
                        .selected_text(delim_label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.delim, ';', ";");
                            ui.selectable_value(&mut self.delim, ',', ",");
                            ui.selectable_value(&mut self.delim, '\t', "Tab");
                        });
                });
            });
        });
        ui.add_space(12.0);

        if vault.persons.is_empty() {
            widgets::empty_state(ui, p, widgets::Icon::File, "Noch keine Personen — Liste importieren oder einzeln hinzufügen.");
        } else {
            let persons = &vault.persons;
            let ids: Vec<Uuid> = persons.iter().map(|x| x.id).collect();
            let mut to_delete2 = None;
            widgets::striped_table(
                ui,
                p,
                "persons",
                &[
                    egui_extras::Column::auto(),
                    egui_extras::Column::auto(),
                    egui_extras::Column::auto(),
                    egui_extras::Column::auto(),
                    egui_extras::Column::remainder(),
                    egui_extras::Column::auto(),
                ],
                &["Nachname", "Vorname", "E-Mail", "Matrikel", "Pseudonym", "Aktion"],
                32.0,
                persons.len(),
                |row| {
                    let i = row.index();
                    let person = &persons[i];
                    let pseudo = vault
                        .assignment_for(person.id)
                        .map(|a| format!("{}, {}", a.last_name, a.first_name))
                        .unwrap_or_default();
                    row.col(|ui| widgets::cell(ui, p, &person.last_name));
                    row.col(|ui| widgets::cell(ui, p, &person.first_name));
                    row.col(|ui| widgets::cell(ui, p, person.emails.join(", ")));
                    row.col(|ui| widgets::cell(ui, p, person.matriculation.clone().unwrap_or_else(|| "—".into())));
                    row.col(|ui| widgets::cell(ui, p, pseudo));
                    row.col(|ui| {
                        if widgets::link_button(ui, p, "löschen").clicked() {
                            to_delete2 = Some(ids[i]);
                        }
                    });
                },
            );
            if let Some(id) = to_delete2 {
                vault.persons.retain(|x| x.id != id);
                vault.pseudonyms.retain(|a| a.person_id != id);
                self.set_status(Status::Neutral, "Person entfernt.");
            }
        }
        ui.add_space(12.0);

        // Einzelne Person hinzufügen
        egui::Grid::new("pf").num_columns(5).spacing([8.0, 6.0]).show(ui, |ui| {
            labeled_field(ui, p, "Nachname", &mut self.pf_last, "Nachname", 130.0);
            labeled_field(ui, p, "Vorname", &mut self.pf_first, "Vorname", 130.0);
            labeled_field(ui, p, "E-Mail (optional)", &mut self.pf_email, "E-Mail", 180.0);
            labeled_field(ui, p, "Matrikel (optional)", &mut self.pf_matr, "Matrikel", 110.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                if widgets::small_button(ui, p, Btn::Secondary, "Hinzufügen").clicked()
                    && (!self.pf_last.trim().is_empty() || !self.pf_first.trim().is_empty())
                {
                    let mut person = Person::new(self.pf_first.trim(), self.pf_last.trim());
                    if !self.pf_email.trim().is_empty() {
                        person.emails.push(self.pf_email.trim().to_string());
                    }
                    if !self.pf_matr.trim().is_empty() {
                        person.matriculation = Some(self.pf_matr.trim().to_string());
                    }
                    vault.persons.push(person);
                    self.pf_first.clear();
                    self.pf_last.clear();
                    self.pf_email.clear();
                    self.pf_matr.clear();
                    self.set_status(Status::Success, "Person hinzugefügt.");
                }
            });
            ui.end_row();
        });

        vault.ensure_pseudonyms();
        match store.save_project(&vault) {
            Ok(()) => self.vault = Some(vault),
            Err(e) => self.set_status(Status::Error, format!("Speichern fehlgeschlagen: {e}")),
        }
    }

    // ------------------------------------------------------------ Anonymisieren

    fn ui_anonymize(&mut self, ui: &mut egui::Ui, p: &Palette, mode: Mode) {
        let Some(vault) = self.vault.clone() else {
            widgets::empty_state(ui, p, widgets::Icon::Folder, "Kein Projekt ausgewählt — links ein Projekt wählen, sonst fehlen die Pseudonyme.");
            return;
        };
        widgets::h2(ui, p, &vault.name);
        widgets::lede(ui, p, "Vier Schritte, von der Anonymisierung bis zurück zu den Namen");
        ui.add_space(12.0);

        let mut do_anon = false;
        let mut do_deanon = false;
        let mut do_export = false;
        let mut do_deanon_run = false;

        let avail = ui.available_width();
        let col_w = (avail - 16.0) / 2.0;
        let row_h = ui.available_height();
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(col_w, row_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                card_files(ui, p, mode, 1, "Anonymisieren", "Echtname → Pseudonym", "Prüfungen auswählen…", "Zielordner wählen…", "Anonymisieren starten", &mut self.anon_files, &mut self.out_dir_anon, &mut do_anon);
            });
            ui.add_space(16.0);
            ui.allocate_ui_with_layout(egui::vec2(col_w, row_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                card_files(ui, p, mode, 2, "De-Anonymisieren", "Pseudonym → Echtname", "Dateien auswählen…", "Zielordner wählen…", "De-Anonymisieren starten", &mut self.deanon_files, &mut self.out_dir_deanon, &mut do_deanon);
            });
        });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(egui::vec2(col_w, row_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                widgets::section_card(ui, p, mode, 3, "Lauf exportieren", "Ordner für die KI", |ui| {
                    widgets::field_label(ui, p, "Bezeichnung");
                    widgets::text_field(ui, p, &mut self.run_label, "z.B. Klassenarbeit 2026-06-01", false);
                    ui.add_space(8.0);
                    widgets::file_picker(ui, p, "Prüfungen auswählen…", &mut self.run_files, &["docx", "pdf"], "Noch keine Dateien gewählt (.docx, .pdf)");
                    ui.add_space(8.0);
                    if widgets::path_picker(ui, p, "Export-Oberordner…", &self.run_root) {
                        self.run_root = rfd::FileDialog::new().pick_folder();
                    }
                    ui.add_space(8.0);
                    let ready = !self.run_files.is_empty() && self.run_root.is_some();
                    ui.add_enabled_ui(ready, |ui| {
                        if widgets::button(ui, p, Btn::Primary, "Lauf exportieren").clicked() {
                            do_export = true;
                        }
                    });
                });
            });
            ui.add_space(16.0);
            ui.allocate_ui_with_layout(egui::vec2(col_w, row_h), egui::Layout::top_down(egui::Align::Min), |ui| {
                widgets::section_card(ui, p, mode, 4, "Lauf-Ordner de-anonymisieren", "Nach der KI-Bewertung", |ui| {
                    if widgets::path_picker(ui, p, "Lauf-Ordner wählen…", &self.deanon_folder) {
                        self.deanon_folder = rfd::FileDialog::new().pick_folder();
                    }
                    ui.add_space(8.0);
                    if widgets::path_picker(ui, p, "Zielordner wählen…", &self.out_dir_deanon) {
                        self.out_dir_deanon = rfd::FileDialog::new().pick_folder();
                    }
                    ui.add_space(8.0);
                    let ready = self.deanon_folder.is_some() && self.out_dir_deanon.is_some();
                    ui.add_enabled_ui(ready, |ui| {
                        if widgets::button(ui, p, Btn::Primary, "Lauf de-anonymisieren").clicked() {
                            do_deanon_run = true;
                        }
                    });
                });
            });
        });

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
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Ergebnisse").font(widgets::semibold(15.0)).color(p.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    widgets::muted(ui, p, "neueste oben");
                });
            });
            ui.add_space(4.0);
            for (head, detail) in &self.results {
                ui.horizontal(|ui| {
                    widgets::icon(ui, widgets::Icon::File, p.muted);
                    ui.label(egui::RichText::new(head).font(widgets::mono(13.0)).color(p.text));
                });
                ui.label(egui::RichText::new(detail).font(widgets::body(14.0)).color(p.muted));
                ui.add_space(4.0);
            }
        }
    }

    fn run_files(&mut self, vault: &ProjectVault, anonymize: bool) {
        let (files, out_dir) = if anonymize {
            (&self.anon_files, &self.out_dir_anon)
        } else {
            (&self.deanon_files, &self.out_dir_deanon)
        };
        let Some(out_dir) = out_dir.clone() else {
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
        let suffix = if anonymize { "_anonym" } else { "_deanonym" };
        let mut any_error = false;
        let mut any_residual = false;
        for f in files {
            let stem = f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "datei".into());
            let ext = f.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let out = out_dir.join(format!("{stem}{suffix}.{ext}"));
            let result: Result<String, String> = match ext.as_str() {
                "docx" => docx::process_docx(f, &out, &mapping)
                    .map(|s| format!("{} Ersetzungen ({})", s.replacements, fmt_per_identifier(&s.per_identifier)))
                    .map_err(|e| e.to_string()),
                "pdf" => pdf::process_pdf(f, &out, &mapping)
                    .map(|s| {
                        let mut msg = format!("{} Ersetzungen ({})", s.replacements, fmt_per_identifier(&s.per_identifier));
                        if !s.residual_matches.is_empty() {
                            any_residual = true;
                            msg.push_str(&format!(" — ACHTUNG {} Treffer über Textobjekte verteilt: {}", s.residual_matches.len(), s.residual_matches.join(", ")));
                        }
                        msg
                    })
                    .map_err(|e| e.to_string()),
                other => Err(format!("unbekanntes Format .{other}")),
            };
            match result {
                Ok(msg) => self.results.insert(0, (file_name(f), format!("→ {}: {msg}", file_name(&out)))),
                Err(e) => {
                    any_error = true;
                    self.results.insert(0, (file_name(f), format!("FEHLER: {e}")));
                }
            }
            // Fuzzy-Scan: Tippfehler-Varianten von Namen in die Review-Liste
            let scan: Result<Vec<FuzzyHit>, String> = match ext.as_str() {
                "docx" => docx::scan_fuzzy_docx(f, &mapping).map_err(|e| e.to_string()),
                "pdf" => pdf::scan_fuzzy_pdf(f, &mapping).map_err(|e| e.to_string()),
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
            format!(
                "Fertig: {} Dateien verarbeitet, {} unscharfe Treffer in Review.",
                files.len(),
                self.review.len()
            ),
        );
    }

    // ------------------------------------------------------------ Review

    fn ui_review(&mut self, ui: &mut egui::Ui, p: &Palette) {
        widgets::h2(ui, p, "Unscharfe Treffer — bitte prüfen");
        widgets::lede(ui, p, "Unscharfe Treffer (Tippfehler), die das exakte Matching nicht fand. Ausgewählte werden übernommen und die Dateien neu verarbeitet.");
        if self.review.is_empty() {
            ui.add_space(16.0);
            widgets::empty_state(ui, p, widgets::Icon::Search, "Keine offenen Treffer — nach einem Lauf erscheinen hier unscharfe Namen-Varianten.");
            return;
        }
        ui.add_space(12.0);
        let n_sel = self.review.iter().filter(|r| r.selected).count();
        let n_all = self.review.len();
        ui.horizontal(|ui| {
            if widgets::small_button(ui, p, Btn::Secondary, "Alle auswählen").clicked() {
                for r in &mut self.review {
                    r.selected = true;
                }
            }
            if widgets::small_button(ui, p, Btn::Secondary, "Alle abwählen").clicked() {
                for r in &mut self.review {
                    r.selected = false;
                }
            }
            widgets::muted(ui, p, format!("{n_sel} von {n_all} ausgewählt"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled_ui(n_sel > 0, |ui| {
                    if widgets::button(ui, p, Btn::Primary, "Ausgewählte übernehmen").clicked() {
                        self.apply_review();
                    }
                });
            });
        });
        ui.add_space(12.0);
        widgets::striped_table(
            ui,
            p,
            "review",
            &[
                egui_extras::Column::exact(36.0),
                egui_extras::Column::auto(),
                egui_extras::Column::remainder(),
                egui_extras::Column::auto(),
                egui_extras::Column::exact(80.0),
            ],
            &["", "Fundstelle", "Kontext", "Treffer → Ersetzung", "Ähnlichkeit"],
            34.0,
            self.review.len(),
            |row| {
                let i = row.index();
                let item = &mut self.review[i];
                row.col(|ui| {
                    ui.checkbox(&mut item.selected, "");
                });
                row.col(|ui| widgets::cell(ui, p, &item.hit.location));
                row.col(|ui| {
                    ui.label(context_job(p, &item.hit.context, &item.hit.matched));
                });
                row.col(|ui| {
                    ui.label(egui::RichText::new(format!("{} → {}", item.hit.matched, item.hit.replacement)).font(widgets::body(14.0)).color(p.text));
                });
                row.col(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        widgets::cell(ui, p, format!("{:.0} %", item.hit.similarity * 100.0));
                    });
                });
            },
        );
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
                self.results.insert(0, (
                    format!("Lauf ‹{}› exportiert", rec.label),
                    format!(
                        "{} Dateien, {} Ersetzungen → {} · Tokens: {}",
                        rec.entries.len(),
                        total,
                        folder.display(),
                        tokens.join(", ")
                    ),
                ));
                self.set_status(Status::Success, "Export fertig — Lauf im Protokoll.");
                // Fuzzy-Review für die Lauf-Dateien
                let mapping = mapping::anonymize_mapping(vault);
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
                self.results.insert(0, (
                    format!("Lauf ‹{}› {}", rec.label, rec.direction.as_str()),
                    format!(
                        "{} Dateien, {} Ersetzungen aus {} → {}",
                        rec.entries.len(),
                        total,
                        folder.display(),
                        out_folder.display()
                    ),
                ));
                self.set_status(Status::Success, "De-Anonymisierung fertig — Lauf im Protokoll.");
            }
            Err(e) => self.set_status(Status::Error, format!("De-Anonymisierung fehlgeschlagen: {e}")),
        }
    }

    fn ui_log(&mut self, ui: &mut egui::Ui, p: &Palette) {
        widgets::h2(ui, p, "Nur Pseudonyme, nie Klarnamen");
        widgets::lede(ui, p, "Verschlüsselt gespeichert. Enthält nur Pseudonyme, Dateinamen und Zähler — keine Klarnamen.");
        ui.add_space(12.0);
        let Some(store) = self.store.clone() else {
            widgets::empty_state(ui, p, widgets::Icon::File, "Kein Tresor geladen.");
            return;
        };
        match store.list_runs() {
            Ok(run_list) => {
                if run_list.is_empty() {
                    widgets::empty_state(ui, p, widgets::Icon::File, "Noch keine Läufe protokolliert.");
                    return;
                }
                widgets::striped_table(
                    ui,
                    p,
                    "runs",
                    &[
                        egui_extras::Column::exact(120.0),
                        egui_extras::Column::auto(),
                        egui_extras::Column::auto(),
                        egui_extras::Column::auto(),
                        egui_extras::Column::exact(60.0),
                        egui_extras::Column::exact(100.0),
                        egui_extras::Column::remainder(),
                    ],
                    &["Zeit", "Richtung", "Label", "Projekt", "Dateien", "Ersetzungen", "Tokens (Pseudonyme)"],
                    32.0,
                    run_list.len(),
                    |row| {
                        let r = &run_list[run_list.len() - 1 - row.index()];
                        row.col(|ui| widgets::cell(ui, p, r.created_at.format("%d.%m.%Y %H:%M").to_string()));
                        row.col(|ui| direction_pill(ui, p, r.direction.as_str()));
                        row.col(|ui| widgets::cell(ui, p, &r.label));
                        row.col(|ui| widgets::cell(ui, p, &r.project_name));
                        row.col(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                widgets::cell(ui, p, r.entries.len().to_string());
                            });
                        });
                        row.col(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                widgets::cell(ui, p, r.entries.iter().map(|e| e.replacements).sum::<usize>().to_string());
                            });
                        });
                        row.col(|ui| {
                            let tokens: Vec<&str> = r.entries.iter().flat_map(|e| e.replaced_tokens.iter().map(|s| s.as_str())).collect();
                            widgets::cell(ui, p, tokens.join(", "));
                        });
                    },
                );
            }
            Err(e) => {
                widgets::message(ui, p, widgets::Msg::Error, &format!("Log nicht lesbar: {e}"));
            }
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
            let out = out_dir.join(format!("{stem}{suffix}.{ext}"));
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
                Ok(msg) => self.results.insert(0, (file_name(&input), format!("Review angewendet → {}: {msg}", file_name(&out)))),
                Err(e) => self.results.insert(0, (file_name(&input), format!("FEHLER: {e}"))),
            }
            applied += 1;
        }
        // übernommene Treffer aus der Review-Liste entfernen
        self.review.retain(|r| !r.selected);
        self.set_status(Status::Success, format!("Review übernommen: {applied} Dateien neu verarbeitet."));
    }
}

// ---------------------------------------------------------------- Helfer


/// Karte 1/2: Datei-Picker + Zielordner + Start-Button.
#[allow(clippy::too_many_arguments)]
fn card_files(
    ui: &mut egui::Ui,
    p: &Palette,
    mode: Mode,
    num: u8,
    title: &str,
    sub: &str,
    pick_label: &str,
    out_label: &str,
    btn_label: &str,
    files: &mut Vec<PathBuf>,
    out_dir: &mut Option<PathBuf>,
    trigger: &mut bool,
) {
    widgets::section_card(ui, p, mode, num, title, sub, |ui| {
        widgets::file_picker(ui, p, pick_label, files, &["docx", "pdf"], "Noch keine Dateien gewählt (.docx, .pdf)");
        ui.add_space(8.0);
        if widgets::path_picker(ui, p, out_label, out_dir) {
            *out_dir = rfd::FileDialog::new().pick_folder();
        }
        ui.add_space(8.0);
        let ready = !files.is_empty() && out_dir.is_some();
        ui.add_enabled_ui(ready, |ui| {
            if widgets::button(ui, p, Btn::Primary, btn_label).clicked() {
                *trigger = true;
            }
        });
    });
}

fn labeled_field(ui: &mut egui::Ui, p: &Palette, label: &str, value: &mut String, hint: &str, width: f32) {
    ui.vertical(|ui| {
        ui.set_width(width);
        widgets::field_label(ui, p, label);
        widgets::text_field(ui, p, value, hint, false);
    });
}

/// Kontext in Mono, Treffer hervorgehoben (warning-text).
fn context_job(p: &Palette, context: &str, matched: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let base = egui::text::TextFormat {
        font_id: widgets::mono(13.0),
        color: p.muted,
        ..Default::default()
    };
    let mark = egui::text::TextFormat {
        font_id: widgets::mono(13.0),
        color: p.warning_text,
        ..Default::default()
    };
    if !matched.is_empty() {
        if let Some(idx) = context.find(matched) {
            job.append(&context[..idx], 0.0, base.clone());
            job.append(matched, 0.0, mark);
            job.append(&context[idx + matched.len()..], 0.0, base);
            return job;
        }
    }
    job.append(context, 0.0, base);
    job
}

/// Richtungs-Pill im Protokoll: «→ anonymisiert» / «← de-anonymisiert».
fn direction_pill(ui: &mut egui::Ui, p: &Palette, direction: &str) {
    let (arrow, label) = if direction == "anonymisiert" { ("→", "anonymisiert") } else { ("←", "de-anonymisiert") };
    egui::Frame::new()
        .fill(p.sunken)
        .stroke(egui::Stroke::new(1.0_f32, p.border))
        .corner_radius(egui::CornerRadius::same(100))
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(format!("{arrow} {label}")).font(widgets::medium(13.0)).color(p.text));
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
