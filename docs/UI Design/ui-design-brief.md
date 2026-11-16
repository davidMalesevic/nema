# Nema — UI Design Brief

**Version:** 1.0 · **Zielgruppe des Briefs:** UI/UX-Design (z.B. Claude Design) · **Sprache der App:** Deutsch
**Status des Projekts:** funktionaler Prototyp in Rust/egui existiert; das Design soll auf den bestehenden Screens aufsetzen und sie polieren.

---

## 1. Was ist Nema?

Nema ist eine **Desktop-App** (Windows, macOS, Linux), mit der Lehrpersonen **Prüfungen und Bewertungsunterlagen anonymisieren**, um sie afterwards extern durch eine KI bewerten zu lassen — und die Ergebnisse danach wieder **de-anonymisieren**.

Kernversprechen (must be visible in UI): **„Alle Namen bleiben lokal und verschlüsselt auf diesem Gerät."** Die App übermittelt keine Daten irgendwohin.

Sensibles Setting: Es geht um echte Schülerdaten (DSGVO / school context). Vertrauen, Ruhe und Nachvollziehbarkeit sind wichtiger als „slick".

---

## 2. Zielgruppe & Nutzungssituation

- **Persona:** Lehrperson (30–60), routiniert mit Word/PDF, nicht technikaffin, arbeitet am eigenen Laptop, oft unter Zeitdruck (Klassenarbeiten korrigieren).
- **Kontext:** App läuft lokal auf dem Laptop; Bildschirm meist hell; Nutzung am Schreibtisch, konzentriert, seriös.
- **Digitale Souveränität:** Nutzer:in will jederzeit sehen, dass nichts das Gerät verlässt.

---

## 3. Design-Prinzipien

1. **Vertrauen vor Effekt.** Ruhige Flächen, klare Hierarchie, keine Ablenkung.
2. **Sichtbare Sicherheit.** Verschlüsselungs-/Lokal-Status immer präsent (Schloss-Icon + Text), nie versteckt.
3. **Klare Richtung.** Jeder Screen hat genau eine Hauptaktion (Primary Button).
4. **Nachvollziehbarkeit.** Anonymisierung ist ein Prozess mit Review-Schritt — das UI macht jeden Schritt sichtbar (was wurde ersetzt, was ist unscharf?).
5. **Kein Cloud-/KI-Branding.** Die KI-Nutzung passiert extern; die App positioniert sich nicht als „KI-Tool", sondern als „sicherer Tresor + Übersetzer".

---

## 4. Branding & Design Tokens

### Logo (Original-Vektoren liegen im Projekt: `packaging/*.svg`)
- **Mark:** radialer Strichkranz (24 Striche, runde Kappen), oberster Strich **orange**, Rest **dunkles Navy**.
- Varianten: `nema-icon.svg` (für helle Hintergründe), `nema-icon-dark.svg` (dunkel), `nema-icon-mono.svg` (einfarbig), `nema-logo.svg` (horizontal mit Wortmarke), `nema-logo-gestapelt.svg`, `nema-favicon.svg`.
- App-Icon + Fenster-Icon: `nema.png` (256 px, aus `nema-icon.svg` gerendert).

### Farben (aus dem Logo abgeleitet)
| Token | Wert | Verwendung |
|---|---|---|
| `navy-900` | `#12202E` | Primärtext, Logo, Primary Buttons (Text weiß) |
| `orange-500` | `#F5A524` | Akzent, Logo-Strich, Highlights, „Review"-Badge |
| `bg` | `#FAFBFC` hell / `#0F1720` dunkel | App-Hintergrund |
| `surface` | `#FFFFFF` / `#182430` | Karten, Panels, Tabellen |
| `border` | `#E3E8EE` / `#2A3947` | Rahmen, Trenner |
| `text` | `#12202E` / `#E8EDF2` | Fließtext |
| `text-muted` | `#5B6B7B` / `#93A3B3` | Sekundärtext, Platzhalter |
| `success` | `#2E9E5B` | Erfolgsmeldungen |
| `danger` | `#D64545` | Fehler, Löschen |
| `warning` | `#F5A524` | Warnhinweise (z.B. PDF-Residuals) |

### Typografie
- **Font: Switzer** (Fontshare, kostenlos) — Gewichte 400 (Regular), 500 (Medium), 600 (SemiBold), 700 (Bold) sind in der App eingebettet (`app/fonts/`).
- Headings: SemiBold; Body: Regular 15 px; Tabellen: 14 px; Mono (Dateinamen, Pfade): Monospace 13–14 px.

### Iconografie
- Dünne Line-Icons (1.5–1.75 px Strich), im Navy-Ton; Schloss-Icon für Sicherheit, Warn-Dreieck für Review/Residuals, Datei-/Ordner-Icons für File-Picker.

---

## 5. Plattform & technische Rahmendaten

- **Desktop-App** (egui/Rust), Fenster initial **1024×680 px**, resizable, min. ca. 900×600.
- **Light & Dark Mode** (folgt dem System); beide Logo-Varianten liegen bereit.
- Maus + Tastatur (vollständig bedienbar, sichtbarer Fokus-Ring in Orange).
- **Deutschsprachige UI**, kurze, klare Sätze.

---

## 6. Informationsarchitektur (Screens)

### S0 — Erster Start / Tresor erstellen (Setup)
- Logo oben zentriert, App-Name, ein Satz: „Verschlüsselten Tresor für die Namenslisten anlegen."
- Wahl: **OS-Schlüsselbund** (empfohlen, Radio) oder **Nur Backup-Passwort**.
- Backup-Passwort-Felder (immer sichtbar, min. 8 Zeichen, mit Wiederholung, Passwort-Maske + "anzeigen"-Toggle).
- Primary Button: „Tresor erstellen". Darunter: Speicherort als Mono-Text (`~/.local/share/nema` etc.).
- Fehlertexte inline in Rot (z.B. „Passwörter stimmen nicht überein").

### S1 — Entsperren (Unlock)
- Logo, „Tresor entsperren".
- Primary: „Per OS-Schlüsselbund entsperren".
- Divider „oder Backup-Passwort", Passwortfeld + Button „Entsperren".
- Hinweistext, wenn Schlüsselbund nicht klappt: „Schlüsselbund nicht verfügbar — bitte Backup-Passwort eingeben."

### S2 — Hauptfenster (Shell)
- **Linke Seitenleiste (ca. 220 px):** Überschrift „Projekte", Liste der Projekte (auswählbar), darunter Eingabefeld + Button „Anlegen", sowie „Projekt löschen" (Danger, mit Bestätigungs-Dialog).
- **Obere Tab-Leiste:** `Personen` · `Anonymisieren` · `Review (n)` (Badge mit offener Anzahl in Orange) · `Protokoll`. Rechts in der Leiste: Tresor-Status „🔒 OS-Schlüsselbund" bzw. „🔒 Passwort".
- **Untere Statusleiste:** letzte Aktionsmeldung (z.B. „Fertig: 5 Dateien verarbeitet, 2 unscharfe Treffer in Review.").
- **Zentralbereich:** je nach Tab (siehe S3–S6).

### S3 — Tab „Personen" (Projekt выбран)
- Kopf: Projektname als Heading + „n Personen mit Pseudonymen".
- Toolbar: Trennzeichen-Dropdown (`;` `,` Tab) + Button „Namensliste importieren…" (öffnet Dateidialog, .csv/.txt/.tsv).
- **Tabelle:** Nachname | Vorname | E-Mail | Matrikel | Pseudonym | Aktion („löschen").
- Darunter: Eingabefelder Nachname, Vorname, E-Mail (optional), Matrikel (optional) + Button „Hinzufügen".
- Leer-Zustand: „Noch keine Personen — Liste importieren oder einzeln hinzufügen."

### S4 — Tab „Anonymisieren"
Vier klar getrennte Sektionen (nummeriert, mit Zwischenüberschriften):
1. **Anonymisieren (Echtname → Pseudonym):** Button „Prüfungen auswählen…" (Dateiliste, .docx/.pdf, mit „Liste leeren"), Zielordner-Picker, Primary „Anonymisieren starten".
2. **De-Anonymisieren (Pseudonym → Echtname):** analog.
3. **Lauf exportieren (Ordner für die KI):** Label-Eingabe („z.B. Klassenarbeit 2026-06-01"), Dateiliste, Export-Oberordner, Primary „Lauf exportieren".
4. **Lauf-Ordner de-anonymisieren (nach der KI-Bewertung):** Lauf-Ordner-Picker, Zielordner, Primary „Lauf de-anonymisieren".
- Darunter: **Ergebnisliste** (neueste oben): Datei → Ausgabedatei: „n Ersetzungen (name: x)“; PDF-Warnung in Orange: „ACHTUNG n Treffer über Textobjekte verteilt: …"; Fehler in Rot.

### S5 — Tab „Review" (Fuzzy-Treffer)
- Erklärungssatz: „Unscharfe Treffer (Tippfehler), die das exakte Matching nicht fand. Ausgewählte werden übernommen und die Dateien neu verarbeitet."
- Buttons: „Alle auswählen" / „Alle abwählen" / Primary „Ausgewählte übernehmen".
- **Tabelle mit Checkbox:** Fundstelle (z.B. „word/document.xml, Absatz 3“ / „Seite 2") | Kontext (Mono, Ausschnitt „…Müler unterschrieb…“) | Treffer → Ersetzung („Müler → Roth“) | Ähnlichkeit in %.
- Leer-Zustand: „Keine offenen Treffer."

### S6 — Tab „Protokoll"
- Hinweis: „Verschlüsselt gespeichert. Enthält nur Pseudonyme, Dateinamen und Zähler — keine Klarnamen."
- **Tabelle (neueste zuerst):** Zeit | Richtung („anonymisiert“/„de-anonymisiert") | Label | Projekt | Dateien | Ersetzungen | Tokens (Pseudonyme).
- Leer-Zustand: „Noch keine Läufe protokolliert."

---

## 7. Komponenten (Library)

- **Buttons:** Primary (navy, weißer Text), Secondary (weiß/surface, navy Text, Border), Danger (danger Text), Icon-Buttons, kleine Inline-Buttons in Tabellen („löschen").
- **Textfelder:** Standard, Passwort (Maske + Sichtbarkeits-Toggle), mit Platzhalter; Fehlerzustand mit rotem Rahmen + Meldung.
- **Datei-/Ordner-Picker:** als Button + Anzeige der gewählten Pfade (Mono, truncated) darunter.
- **Dropdown/ComboBox:** klein, für Trennzeichen.
- **Tab-Leiste:** oben, aktiv unterstrichen/akzentuiert in Orange.
- **Badge:** runde Zahl in Orange für „Review (n)".
- **Tabelle:** gestreift, Header SemiBold, Hover-Highlight, scrollbar.
- **Checkbox** in Tabellen-Kopf/Spalte.
- **Statusleiste** unten (Info/Erfolg/Warnung/Farbe + Icon).
- **Dialoge:** Bestätigung „Projekt löschen", Fehlermeldungen (mit Kopieren-Button?).
- **Leer-Zustände** mit Icon + Text + Primäraktion.
- **Tresor-Status-Pill** oben rechts (Schloss + Modus).

---

## 8. Zustände, die abgedeckt sein müssen

- Leer-Zustände (keine Projekte, keine Personen, keine Review-Treffer, kein Protokoll)
- Fehler (falsches Passwort, Importfehler mit Zeilennummer, PDFium nicht gefunden, Schreibfehler)
- Erfolg (grüne Statusmeldung mit Anzahl Ersetzungen)
- Warnung (PDF-Residuals, „Treffer über Textobjekte verteilt")
- Setup vs. Unlock vs. Main
- Light + Dark Mode für alle Screens

---

## 9. Zentrale User Flows (End-to-End)

1. **Erster Start:** S0 → Tresor erstellen → direkt in Main (Projekt anlegen).
2. **Projekt anlegen + Liste importieren:** Seitenleiste → Name → Anlegen → Tab Personen → CSV importieren → Pseudonyme erscheinen sofort in der Tabelle.
3. **Prüfungen anonymisieren (KI-ready):** Tab Anonymisieren → Sektion 3 „Lauf exportieren" → Label + Dateien + Zielordner → Export → Ordner enthält nur Pseudonyme → Tab Review prüfen → ggf. übernehmen.
4. **Nach der KI-Bewertung:** Sektion 4 „Lauf-Ordner de-anonymisieren" → Ordner mit KI-Ergebnissen wählen → Zielordner → Namen sind wieder echt.
5. **Neustart:** S1 → Schlüsselbund-Klick (oder Backup-Passwort) → Main.
6. **Nachvollziehen:** Tab Protokoll zeigt alle Läufe mit Pseudonymen.

---

## 10. Microcopy (Vorschläge, Deutsch)

- App-Titel: **„Nema — Anonymisierung"**
- S0: „Verschlüsselten Tresor anlegen" / „Backup-Passwort (min. 8 Zeichen) — Fallback, falls der Schlüsselbund mal nicht verfügbar ist"
- Tresor-Status: „Lokal verschlüsselt · OS-Schlüsselbund" / „Lokal verschlüsselt · Backup-Passwort"
- Review: „Unscharfe Treffer — bitte prüfen"
- Protokoll: „Nur Pseudonyme, nie Klarnamen"
- Export-Erfolg: „Lauf ‚Klassenarbeit 1‘ exportiert — 5 Dateien, 12 Ersetzungen"

---

## 11. Accessibility & Qualität

- Kontrast mind. WCAG AA (navy auf weiß, weiß auf navy, orange nur für Akzente/große Flächen, nie kleiner Text auf orange).
- Tastatur: Tab-Reihenfolge logisch, Enter aktiviert Primary, Esc schließt Dialoge.
- Fokus-Ring sichtbar (2 px orange).
- Text Größen: Body 15 px, nicht skalierbar unter 12 px.
- Keine Animationen außer subtilen Hover-Transitions (150 ms).

---

## 12. Technische Umsetzung (Hinweise ans Engineering)

- Umsetzung in **egui 0.33** (bestehender Rust-Prototyp: `app/src/app.rs`), Font Switzer ist bereits eingebettet und gesetzt (Proportional + SemiBold-Headings), Fenster-Icon ist gesetzt.
- Design-Tokens als Konstanten zentral definieren (Farben oben), Dark Mode über `ctx.style()` bzw. `egui::Theme`.
- Logo-Varianten liegen in `packaging/` (SVG + PNG); für Dark Mode `nema-icon-dark.svg` verwenden.
- Keine Netzwerk-Calls, keine Telemetrie — das UI darf nichts suggerieren, was die App nicht tut.

---

## 13. Offene Fragen ans Design

1. Dark Mode: neutrales dunkles Navy (`#0F1720`) oder kräftigeres Schwarz? Vorschlag: navy-basiert.
2. Sollen die vier Sektionen im „Anonymisieren"-Tab als Karten mit Nummerierung gestaltet werden (empfohlen) oder als einfache Abschnitte?
3. Review-Tabelle: Kontext in Mono neben der Ersetzung — genug, oder zusätzlich Vorher/Nachher-Vergleich?
4. Protokoll: als Tabelle (aktuell) oder Timeline-Ansicht?
