Du bekommst den Auftrag, das UI der Desktop-App **Nema** (Rust + egui, Workspace in diesem Repo) neu zu designen und umzusetzen.

## Kontext

Nema anonymisiert Prüfungen (DOCX/PDF) von Lernenden mit Pseudonymen, damit sie extern per KI bewertet werden können, und de-anonymisiert sie danach wieder. Alle Daten bleiben lokal und AES-256-GCM-verschlüsselt. Das UI muss Vertrauen ausstrahlen (DSGVO/school context), ruhig und klar sein — keine Cloud-/KI-Visuals.

## Unbedingt zuerst lesen

1. `docs/ui-design-brief.md` — vollständiger Design-Brief (Persona, Prinzipien, Design-Tokens, alle 7 Screens, Komponenten, Flows, Microcopy, Accessibility, offene Fragen). Dort stehen die Farbwerte (navy `#12202E`, orange `#F5A524`), die Font (Switzer) und alle Screen-Spezifikationen.
2. `app/src/app.rs` + `app/src/main.rs` — bestehender funktionaler Prototyp (egui 0.33). Alle Funktionen müssen erhalten bleiben; es geht um das Design.
3. `packaging/*.svg` — Original-Logo-Varianten (`nema-icon.svg`, `nema-icon-dark.svg`, `nema-icon-mono.svg`, `nema-logo*.svg`). PNGs: `packaging/nema.png` (256), `nema-512.png`, `nema-1024.png`.
4. `app/fonts/Switzer-{400,500,600,700}.ttf` — bereits eingebettet und in `main.rs` registriert (Proportional = Switzer, Headings = Switzer SemiBold).

## Dein Auftrag

1. **Design-Tokens zentral definieren** (z.B. neues Modul `app/src/theme.rs`): Farben aus dem Brief (navy-900 `#12202E`, orange-500 `#F5A524`, bg/surface/border/text/text-muted für Light UND Dark, success/danger/warning), Font-Familien, Größen (Body 15 px, Tabellen 14 px, Mono 13–14 px), Abstände, Radien. `main.rs` soll `theme::apply(ctx, theme)` aufrufen.
2. **Alle Screens restylen** gemäß Brief Abschnitt 6: Setup (S0), Unlock (S1), Main-Shell mit linker Projekt-Seitenleiste, Tab-Leiste mit Review-Badge und Tresor-Status-Pill (Schloss + Modus), Statusleiste unten (S2), Personen-Tabelle mit Import/Hinzufügen (S3), Anonymisieren mit 4 nummerierten Sektionen + Ergebnisliste (S4), Review-Tabelle mit Checkbox/Kontext/Treffer→Ersetzung/% (S5), Protokoll-Tabelle (S6).
3. **Komponenten** aus Brief Abschnitt 7 bauen: Primary/Secondary/Danger-Buttons, Textfelder (Passwort mit Sichtbarkeits-Toggle, Fehlerzustand rot), Datei-/Ordner-Picker mit Pfadanzeige, ComboBox, gestreifte Tabellen mit Hover, Checkboxen, Badges, Leer-Zustände mit Icon+Text+Aktion, Bestätigungs-Dialog fürs Projekt-Löschen.
4. **Dark Mode** über egui-Theme umsetzen (System follows), Logo-Variante dunkel verwenden wo relevant.
5. **Zustände** abdecken: leer, Fehler, Erfolg, Warnung (PDF-Residuals), siehe Brief Abschnitt 8.
6. **Microcopy** aus Brief Abschnitt 10 übernehmen (deutsche Texte).

## Spielregeln

- **Funktionalität bleibt 1:1 erhalten** — keine Features entfernen oder umbenennen, nur Design/Struktur. Wo das Brief-Layout von der aktuellen Struktur abweicht, strukturiere `app.rs` entsprechend um (z.B. theme-Modul auslagern), ohne Logik zu ändern.
- Keine neuen Netzwerk-Calls, keine Telemetrie, keine externen Ressourcen zur Laufzeit. Fonts und Logo bleiben eingebettet (`include_bytes!`).
- UI-Sprache Deutsch, kurze Sätze.
- Accessibility: sichtbarer Fokus-Ring (2 px orange), logische Tab-Reihenfolge, Kontrast WCAG AA.
- egui 0.33 API beachten (epaint hat kein font-weight-System — Switzer-Weights sind als eigene `FontFamily::Name`s registriert, siehe `main.rs`).

## Abnahme

- `cargo build --release -p nema-app` baut fehlerfrei, `cargo clippy --workspace --all-targets` ohne Warnungen, `cargo test -p nema-core` bestanden (Core darf nicht angefasst werden müssen).
- App startet (`nema-app`), Setup → Tresor anlegen → Projekt → Personen importieren → Anonymisieren → Review → Protokoll sind visuell und funktional nachvollziehbar.
- `NEMA_PDFIUM_LIB` ist für PDF-Tests nötig; ohne Display startet die GUI nicht (headless) — dann nur bauen und `--check-pdfium` prüfen.
- Am Ende kurz berichten: welche Design-Entscheidungen du getroffen hast und welche der 4 offenen Fragen aus dem Brief (Abschnitt 13) du wie gelöst hast.
