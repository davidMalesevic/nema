Restyle der Nema-GUI (egui 0.33). Die Funktionalität bleibt 1:1 erhalten.

## Zuerst lesen
1. docs/ui-design-brief.md: Persona, Prinzipien, Screens (Abschnitt 6), Komponenten (7), Zustände (8), Microcopy (10), Accessibility (11).
2. docs/design/*.png: die abgenommenen Screens, Hell und Dunkel (01-setup … 08-dialog). Das ist die visuelle Referenz. Wo sie vom Brief abweicht, gilt die Referenz.
3. docs/design/tokens.json: die verbindlichen Werte für Farben (Hell/Dunkel), Typo, Abstände, Radien, Schatten, Layout. Übernimm sie 1:1. Erfinde keine Werte.
4. docs/design/nema-reference.css: HTML/CSS-Referenz aller Komponenten (Höhen, Padding, Zustände, Hover, Fokus, Fehler). Übersetze sie nach egui, nicht umgekehrt.
5. app/src/app.rs und app/src/main.rs: der bestehende Prototyp.
6. packaging/*.svg und packaging/*.png: Logos. Fonts liegen in app/fonts/ (Switzer 400/500/600/700, bereits in main.rs registriert).

## Bereits entschieden (nicht neu beantworten)
- Dark Mode ist navy-basiert (bg #0F1720). Der Primary-Button ist dunkel hell gefüllt (mist-100 mit Navy-Text).
- Anonymisieren: vier nummerierte Karten im 2×2-Raster, darunter die Ergebnisliste (neueste oben). Nur Karten mit gültiger Eingabe haben einen aktiven Primary-Button, die anderen sind deaktiviert.
- Review: Kontext in Mono mit hervorgehobenem Treffer, dazu «Treffer → Ersetzung» und Ähnlichkeit in %. Kein separater Vorher/Nachher-Vergleich.
- Protokoll bleibt eine Tabelle. Die Richtung hat Pfeil-Icon und Wort.
- Orange (#F5A524) ist nur Akzent: Tab-Unterstreichung, Review-Badge, Fokus-Ring, aktives Projekt in der Seitenleiste. Text auf Orange ist immer Navy. Als Textfarbe auf hellem Grund nie.
- Status (Erfolg/Warnung/Fehler) trägt immer Icon und Wort. Texte nutzen die *-text-Töne (AA), die Markenfarben nur für Icons und Rahmen.

## Vorgehen
Schritt 0: Zeige mir einen kurzen Plan, BEVOR du Code änderst: Modulstruktur, welche Funktionen aus app.rs wohin wandern, welche egui-Mittel du für Fokus-Ring, gestreifte Tabellen, Tab-Leiste mit Badge und Dialog einsetzt. Warte auf mein OK.

Vorgeschlagene Struktur:
- app/src/theme.rs: Tokens als Konstanten (Farben Hell/Dunkel, Grössen, Abstände, Radien), theme::apply(ctx, theme), Font-Setup bleibt in main.rs.
- app/src/widgets.rs: wiederverwendbare Komponenten: primary_button/secondary_button/danger_button, text_field (inkl. Passwort mit Sichtbarkeits-Toggle und Fehlerzustand), path_picker (Button + Pfad in Mono, in der Mitte gekürzt), badge, vault_pill, tab_bar, section_card, status_bar, message (Info/Erfolg/Warnung/Fehler), empty_state, confirm_dialog, striped_table-Helfer.
- app/src/app.rs: nur noch Zustand, Logik und Screen-Aufbau. Keine Logik ändern.

## Anforderungen
1. Design-Tokens zentral in theme.rs, main.rs ruft theme::apply(ctx, theme) auf. Dark Mode folgt dem System (egui::Theme).
2. Alle Screens gemäss docs/design/*.png: Setup, Unlock, Hauptfenster (linke Seitenleiste 220 px, Tab-Leiste mit Review-Badge und Tresor-Pill rechts, Statusleiste unten), Personen, Anonymisieren, Review, Protokoll, Dialog «Projekt löschen».
3. Komponenten gemäss Brief Abschnitt 7 und nema-reference.css.
4. Logo: im Dark Mode die dunkle Variante, im Light Mode die helle. Fonts und Logo bleiben eingebettet (include_bytes!).
5. Zustände: leer, Fehler, Erfolg, Warnung (PDF-Residuals), jeweils Hell und Dunkel.
6. Microcopy aus Brief Abschnitt 10, deutsche Texte, kurze Sätze. Schweizer Rechtschreibung (ss statt ß, Guillemets «…»).
7. Accessibility: sichtbarer Fokus-Ring (2 px orange, 2 px Abstand, dazu Rahmenwechsel auf text), logische Tab-Reihenfolge, Enter löst den Primary aus, Esc schliesst Dialoge, Kontrast WCAG AA. Rahmen von Bedienelementen nutzen border-input (≥ 3:1), nicht border.
8. Body 15 px, Tabellen 14 px, Mono 13–14 px, nichts unter 12 px. Switzer-Gewichte sind eigene FontFamily::Names (siehe main.rs). egui kennt kein font-weight.
9. Hover-Übergänge 150 ms, sonst keine Animationen.

## Spielregeln
- Keine Features entfernen oder umbenennen, keine Logik ändern. Core (nema-core) bleibt unangetastet.
- Keine neuen Netzwerk-Calls, keine Telemetrie, keine externen Ressourcen zur Laufzeit.
- Neue Abhängigkeiten nur, wenn zwingend nötig (z. B. egui_extras für Tabellen). Nenne sie im Plan.
- Arbeite in kleinen Commits: erst theme.rs, dann widgets.rs, dann ein Screen nach dem anderen.

## Abnahme
- cargo build --release -p nema-app fehlerfrei
- cargo clippy --workspace --all-targets ohne Warnungen
- cargo test -p nema-core bestanden
- Mit Display: App starten und Setup → Tresor anlegen → Projekt → Personen importieren → Anonymisieren → Review → Protokoll durchklicken, in Hell und Dunkel, und mit docs/design/*.png vergleichen. Ohne Display (headless): nur bauen und --check-pdfium prüfen (NEMA_PDFIUM_LIB setzen).
- Am Ende kurz berichten: Abweichungen von den Screens (mit Grund), nicht umsetzbare Details und egui-Workarounds.