//! Rendert alle Screens headless (Hell + Dunkel) als PNG, zum Vergleich mit
//! docs/UI Design. Braucht eine GPU bzw. einen Software-Renderer, darum `#[ignore]`:
//!
//!     cargo test -p nema-app shots -- --ignored
//!
//! Ausgabe: target/shots/ (oder $NEMA_SHOT_DIR). Arbeitet nur in einem
//! temporären Tresor und ohne Zugriff auf den OS-Schlüsselbund.

use super::*;
use nema_core::runs::{RunDirection, RunEntry, RunRecord};

fn save_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) {
    let file = std::fs::File::create(path).expect("PNG anlegen");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().expect("PNG-Header").write_image_data(rgba).expect("PNG-Daten");
}

fn demo_store(dir: &std::path::Path) -> VaultStore {
    let _ = std::fs::remove_dir_all(dir);
    let store = VaultStore::init(dir, false, Some("demo-passwort")).expect("Demo-Tresor");
    let mut v = store.create_project("Klassenarbeit 2026").expect("Projekt");
    for (first, last, mail, matr) in [
        ("Anna", "Müller", Some("a.mueller@schule.ch"), Some("2024-017")),
        ("Jonas", "Keller", Some("j.keller@schule.ch"), Some("2024-021")),
        ("Mara", "Frei", None, None),
    ] {
        let mut person = Person::new(first, last);
        person.emails.extend(mail.map(str::to_string));
        person.matriculation = matr.map(str::to_string);
        v.persons.push(person);
    }
    // Klassen: die ersten beiden Personen gehören zur Klasse «INF 24a», Mara ist Einzelperson
    let mut roster = Roster::default();
    let class = roster.add_class_with("INF 24a", v.persons[..2].to_vec());
    let other = roster.add_class("MED 25b");
    for (first, last) in [("Lea", "Baumann"), ("Noah", "Gerber"), ("Sara", "Hug"), ("Tim", "Wyss")] {
        roster.class_mut(other).expect("Klasse").persons.push(Person::new(first, last));
    }
    roster.add_term("Berufsschule Musterstadt").expect("Begriff");
    store.save_roster(&roster).expect("Klassen speichern");
    v.link_class(class, &roster);
    store.save_project(&v).expect("speichern");
    store.create_project("Probe Mathe 3b").expect("Projekt 2");

    let run = |when: &str, label: &str, project: &str, direction, files: usize, repl: usize, tokens: &[&str]| RunRecord {
        id: Uuid::new_v4(),
        project_id: v.id,
        project_name: project.to_string(),
        label: label.to_string(),
        direction,
        created_at: when.parse().expect("Zeitstempel"),
        entries: (0..files)
            .map(|i| RunEntry {
                source: format!("arbeit_{i}.docx"),
                output: format!("arbeit_{i}_anonym.docx"),
                replacements: if i == 0 { repl } else { 0 },
                replaced_tokens: if i == 0 { tokens.iter().map(|s| s.to_string()).collect() } else { Vec::new() },
                residuals: Vec::new(),
            })
            .collect(),
    };
    for r in [
        run("2026-10-28T14:20:00Z", "Test Geschichte", "Klassenarbeit 2026", RunDirection::Anonymize, 2, 4, &["Roth", "Birke"]),
        run("2026-11-05T09:31:00Z", "Probe Mathe 3b", "Probe Mathe 3b", RunDirection::Anonymize, 3, 7, &["Eiche", "Farn"]),
        run("2026-11-12T16:05:00Z", "Klassenarbeit 1", "Klassenarbeit 2026", RunDirection::Deanonymize, 5, 12, &["Roth", "Linde", "Birke"]),
        run("2026-11-13T12:48:00Z", "Klassenarbeit 1", "Klassenarbeit 2026", RunDirection::Anonymize, 5, 12, &["Roth", "Linde", "Birke"]),
    ] {
        store.log_run(&r).expect("Lauf protokollieren");
    }
    store
}

fn hit(location: &str, context: &str, matched: &str, replacement: &str, similarity: f32) -> ReviewItem {
    ReviewItem {
        input: PathBuf::from("/tmp/arbeit.docx"),
        anonymize: true,
        selected: true,
        hit: FuzzyHit {
            matched: matched.into(),
            identifier: replacement.into(),
            replacement: replacement.into(),
            similarity,
            context: context.into(),
            location: location.into(),
        },
    }
}

fn main_app(store: &VaultStore, mode: Mode) -> App {
    let mut app = App::blank(PathBuf::from("/home/demo/.local/share/nema"), true);
    app.mode_override = Some(mode);
    app.enter_main(store.clone());
    app.review = vec![
        hit("word/document.xml, Absatz 3", "…Müler unterschrieb…", "Müler", "Roth", 0.91),
        hit("Seite 2", "…Frau Kelller notierte…", "Kelller", "Linde", 0.88),
        hit("word/document.xml, Absatz 7", "…Anna Mühler hat…", "Mühler", "Roth", 0.84),
        hit("Seite 4", "…Jonas Keler meldet…", "Keler", "Linde", 0.86),
    ];
    app.review[1].selected = false;
    app
}

/// Sicherheitsnetz: kein Zugriff auf den echten OS-Schlüsselbund, kein echter Tresor.
fn isolate() {
    std::env::set_var("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent-nema-tests");
}

/// Klicks und Tastatur auf den selbst gemalten Bedienelementen (ohne Rendering).
#[test]
fn bedienung() {
    use egui_kittest::kittest::Queryable as _;
    isolate();
    let tmp = std::env::temp_dir().join(format!("nema-ui-test-{}", std::process::id()));
    let store = demo_store(&tmp);
    let app = main_app(&store, Mode::Light);
    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1024.0, 680.0))
        .build_state(|ctx, app: &mut App| app.ui(ctx), app);
    harness.run_steps(4);

    // Tabs
    harness.get_by_label("Protokoll").click();
    harness.run_steps(3);
    assert!(harness.state().tab == Tab::Log);
    harness.get_by_label("Personen").click();
    harness.run_steps(3);
    assert!(harness.state().tab == Tab::Persons);

    // Klasse im Projekt verknüpfen und wieder lösen
    assert_eq!(harness.state().vault.as_ref().unwrap().persons.len(), 3);
    harness.get_by_label("MED 25b · 4").click();
    harness.run_steps(3);
    assert_eq!(harness.state().vault.as_ref().unwrap().persons.len(), 7);
    harness.get_by_label("MED 25b · 4").click();
    harness.run_steps(3);
    assert_eq!(harness.state().vault.as_ref().unwrap().persons.len(), 3);

    // Tab «Klassen»: Lernende einer Klasse sehen
    harness.get_by_label("Klassen").click();
    harness.run_steps(3);
    assert!(harness.state().tab == Tab::Classes);
    harness.get_by_label("Personen").click();
    harness.run_steps(3);

    // Projekt in der Seitenleiste wählen
    let second = harness.state().projects[1].id;
    harness.get_by_label("Probe Mathe 3b").click();
    harness.run_steps(3);
    assert_eq!(harness.state().selected, Some(second));

    // Dialog «Projekt löschen»: Abbrechen und Esc lassen alles stehen
    harness.get_by_label("Projekt löschen").click();
    harness.run_steps(3);
    assert!((harness.state().confirm != Confirm::None));
    harness.get_by_label("Abbrechen").click();
    harness.run_steps(3);
    assert!(harness.state().confirm == Confirm::None);
    assert_eq!(harness.state().projects.len(), 2);

    harness.get_by_label("Projekt löschen").click();
    harness.run_steps(3);
    harness.key_press(egui::Key::Escape);
    harness.run_steps(3);
    assert!(harness.state().confirm == Confirm::None);
    assert_eq!(harness.state().projects.len(), 2);

    // Bestätigen per Tastatur: Fokus liegt auf «Abbrechen», Shift+Tab → «Projekt löschen», Enter
    harness.get_by_label("Projekt löschen").click();
    harness.run_steps(3);
    harness.key_press_modifiers(egui::Modifiers::SHIFT, egui::Key::Tab);
    harness.run_steps(2);
    harness.key_press(egui::Key::Enter);
    harness.run_steps(3);
    assert!(harness.state().confirm == Confirm::None);
    assert_eq!(harness.state().projects.len(), 1);
    assert_eq!(harness.state().projects[0].name, "Klassenarbeit 2026");

    let _ = std::fs::remove_dir_all(&tmp);
}

static TEST_CLIPBOARD: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

fn test_clipboard() -> Result<Vec<PathBuf>, String> {
    Ok(TEST_CLIPBOARD.lock().unwrap().clone())
}

/// Projektordner setzen, dann im Dateimanager kopierte Dateien per Strg+V
/// auf die Karte unter dem Mauszeiger einfügen.
#[test]
fn projektordner_und_einfuegen() {
    use egui_kittest::kittest::Queryable as _;
    isolate();
    let tmp = std::env::temp_dir().join(format!("nema-ui-paste-{}", std::process::id()));
    let store = demo_store(&tmp.join("tresor"));
    let docs = tmp.join("Prüfungen M349");
    std::fs::create_dir_all(docs.join("Nema/anonymisiert")).unwrap();
    for f in ["a.docx", "b.pdf", "notiz.txt", "Nema/anonymisiert/alt_anonym.docx"] {
        std::fs::write(docs.join(f), b"").unwrap();
    }

    let mut app = main_app(&store, Mode::Light);
    app.tab = Tab::Anonymize;
    app.set_project_folder(Some(docs.clone()));
    assert_eq!(app.out_dir_anon, Some(docs.join("Nema/anonymisiert")));
    let id = app.selected.unwrap();
    assert_eq!(store.load_project(id).unwrap().folder, Some(docs.clone()), "Projektordner gespeichert");

    let mut harness = egui_kittest::Harness::builder()
        .with_size(egui::vec2(1024.0, 900.0))
        .build_state(|ctx, app: &mut App| app.ui(ctx), app);
    harness.run_steps(4);

    // Ordner als file://-URL (wie Dolphin) über Karte 2: Nemas Ausgaben bleiben draussen
    let card2 = harness.get_by_label("De-Anonymisieren starten").rect().center();
    harness.hover_at(card2);
    let url = format!("file://{}", docs.display()).replace(' ', "%20").replace('ü', "%C3%BC");
    harness.event(egui::Event::Paste(url));
    harness.run_steps(3);
    let names = |files: &[PathBuf]| files.iter().map(|f| super::file_name(f)).collect::<Vec<_>>();
    assert_eq!(names(&harness.state().deanon_files), ["a.docx", "b.pdf"]);
    assert!(harness.state().anon_files.is_empty());

    // Einzelne Datei als Pfad über Karte 1
    let card1 = harness.get_by_label("Anonymisieren starten").rect().center();
    harness.hover_at(card1);
    harness.event(egui::Event::Paste(docs.join("a.docx").display().to_string()));
    harness.run_steps(3);
    assert_eq!(names(&harness.state().anon_files), ["a.docx"]);

    // Gewöhnlicher Text wird ignoriert
    harness.event(egui::Event::Paste("Hallo".into()));
    harness.run_steps(3);
    assert_eq!(harness.state().anon_files.len(), 1);

    // «Einfügen»: Dateiliste aus der Zwischenablage (wie von Dolphin) in Karte 1 bzw. Ordner in Karte 4
    *TEST_CLIPBOARD.lock().unwrap() = vec![docs.join("b.pdf"), docs.join("notiz.txt")];
    harness.state_mut().clipboard_files = test_clipboard;
    harness.get_all_by_label("Einfügen").next().unwrap().click();
    harness.run_steps(3);
    assert_eq!(names(&harness.state().anon_files), ["a.docx", "b.pdf"], "nur Dokumente, keine Doppelten");
    *TEST_CLIPBOARD.lock().unwrap() = vec![docs.join("Nema/anonymisiert")];
    harness.get_all_by_label("Einfügen").nth(3).unwrap().click();
    harness.run_steps(3);
    assert_eq!(harness.state().deanon_folder, Some(docs.join("Nema/anonymisiert")));
    TEST_CLIPBOARD.lock().unwrap().clear();
    harness.get_all_by_label("Einfügen").next().unwrap().click();
    harness.run_steps(3);
    assert!(harness.state().status_kind == Status::Warning, "leere Zwischenablage → Hinweis");

    // Projektwechsel: die Vorschläge des vorigen Projekts wandern nicht mit
    harness.get_by_label("Probe Mathe 3b").click();
    harness.run_steps(3);
    assert!(harness.state().out_dir_anon.is_none());
    harness.get_by_label("Klassenarbeit 2026").click();
    harness.run_steps(3);
    assert_eq!(harness.state().out_dir_anon, Some(docs.join("Nema/anonymisiert")), "Projektordner wieder aktiv");

    // Projektordner entfernen: die vorgeschlagenen Zielordner verschwinden
    harness.get_by_label("Entfernen").click();
    harness.run_steps(3);
    assert!(harness.state().vault.as_ref().unwrap().folder.is_none());
    assert!(harness.state().out_dir_anon.is_none());
    assert!(harness.state().run_root.is_none());

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[ignore = "braucht GPU/Software-Renderer; schreibt PNGs nach target/shots"]
fn shots() {
    isolate();
    let out = std::env::var("NEMA_SHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../target/shots"));
    std::fs::create_dir_all(&out).expect("Ausgabeordner");
    let ppp: f32 = std::env::var("NEMA_SHOT_PPP").ok().and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let tmp = std::env::temp_dir().join(format!("nema-shots-{}", std::process::id()));
    let store = demo_store(&tmp);
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());

    type Scene = (&'static str, f32, fn(&VaultStore, Mode, &std::path::Path) -> App);
    let scenes: &[Scene] = &[
        ("01-setup", 680.0, |_, mode, _| {
            let mut app = App::blank(PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".local/share/nema"), true);
            app.mode_override = Some(mode);
            app.setup_pw1 = "geheim-12345".into();
            app.setup_pw2 = "geheim".into();
            app
        }),
        ("02-unlock", 680.0, |_, mode, _| {
            let mut app = App::blank(PathBuf::from("/x"), true);
            app.mode_override = Some(mode);
            app.screen = Screen::Unlock;
            app.unlock_pw = "geheim-12345".into();
            app.unlock_msg = "Schlüsselbund nicht verfügbar — bitte Backup-Passwort eingeben.".into();
            app
        }),
        ("03-personen", 680.0, |store, mode, _| {
            let mut app = main_app(store, mode);
            app.set_status(Status::Success, "Namensliste importiert — 3 Personen.");
            app
        }),
        ("04-personen-leer", 680.0, |store, mode, _| {
            let mut app = main_app(store, mode);
            app.review.clear();
            let id = app.projects[1].id;
            app.select_project(id);
            app.set_status(Status::Error, "Importfehler: Zeile 14: E-Mail fehlt.");
            app
        }),
        ("05-anonymisieren", 1180.0, |store, mode, home| {
            let mut app = main_app(store, mode);
            app.tab = Tab::Anonymize;
            app.run_label = "Klassenarbeit 1".into();
            app.run_files = (1..=5).map(|i| home.join(format!("Dokumente/Klassenarbeit_2026/Arbeit_{i}_Gruppe_A.docx"))).collect();
            app.run_root = Some(home.join("Dokumente/Klassenarbeit_2026/KI-Export"));
            app.push_result(Msg::Error, "Arbeit_5.pdf", "FEHLER: PDFium-Bibliothek nicht gefunden");
            app.push_result(Msg::Warning, "Arbeit_4.pdf  →  Arbeit_4_anonym.pdf", "3 Ersetzungen (Keller: 1, Müller: 2) — ACHTUNG 1 Treffer stehen noch im Text: Müller");
            app.push_result(Msg::Success, "Lauf ‹Klassenarbeit 1› exportiert", "5 Dateien, 12 Ersetzungen  →  ~/Dokumente/Klassenarbeit_2026/KI-Export/Klassenarbeit_1 · Tokens: Roth, Linde, Birke");
            app.set_status(Status::Success, "Lauf ‹Klassenarbeit 1› exportiert — 5 Dateien, 12 Ersetzungen");
            app
        }),
        ("12-projektordner", 900.0, |store, mode, home| {
            let mut app = main_app(store, mode);
            app.tab = Tab::Anonymize;
            // nur im Speicher, damit die anderen Szenen den Demo-Tresor unverändert sehen
            app.vault.as_mut().expect("Projekt").folder = Some(home.join("Dokumente/Schule/Leistungsbeurteilung (LB)/Klassenarbeit_2026"));
            app.apply_project_dirs();
            app
        }),
        ("06-review", 680.0, |store, mode, _| {
            let mut app = main_app(store, mode);
            app.tab = Tab::Review;
            app.set_status(Status::Warning, "Fertig: 5 Dateien verarbeitet, 4 unscharfe Treffer in Review.");
            app
        }),
        ("07-protokoll", 680.0, |store, mode, _| {
            let mut app = main_app(store, mode);
            app.tab = Tab::Log;
            app
        }),
        ("08-dialog", 680.0, |store, mode, _| {
            let mut app = main_app(store, mode);
            app.set_status(Status::Success, "Namensliste importiert — 3 Personen.");
            app.confirm = Confirm::DeleteProject;
            app.dialog_focus = true;
            app
        }),
        ("09-schmal-protokoll", 600.0, |store, mode, _| {
            let mut app = main_app(store, mode);
            app.tab = Tab::Log;
            app
        }),
        ("11-klassen", 900.0, |store, mode, _| {
            let mut app = main_app(store, mode);
            app.tab = Tab::Classes;
            app
        }),
        ("10-breit-protokoll", 720.0, |store, mode, _| {
            let mut app = main_app(store, mode);
            app.tab = Tab::Log;
            app
        }),
    ];

    for (name, height, build) in scenes {
        for (suffix, mode) in [("hell", Mode::Light), ("dunkel", Mode::Dark)] {
            let mut app = build(&store, mode, &home);
            let width = if name.starts_with("09") {
                900.0
            } else if name.starts_with("10") {
                1280.0
            } else {
                1024.0
            };
            let mut harness = egui_kittest::Harness::builder()
                .with_size(egui::vec2(width, *height))
                .with_pixels_per_point(ppp)
                .wgpu()
                .build(move |ctx| app.ui(ctx));
            harness.run_steps(6);
            let image = harness.render().expect("Rendern");
            let path = out.join(format!("{name}-{suffix}.png"));
            save_png(&path, image.width(), image.height(), image.as_raw());
            println!("geschrieben: {}", path.display());
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
