<img src="packaging/nema-logo.png" alt="Nema" width="320">

# Nema — Anonymisierungstool für Prüfungen

Desktop-App (Rust + egui) für Windows, macOS und Linux. Sie anonymisiert Prüfungen
(DOCX, PDF) von Lernenden mit Pseudonymen, damit du sie extern durch eine KI
bewerten lassen kannst, und de-anonymisiert sie danach wieder. Alle sensiblen
Daten bleiben lokal und verschlüsselt.

## Schrift und Logo

- UI-Font: **Switzer** (Fontshare, kostenlose Lizenz) — eingebettet in
  `app/fonts/Switzer-{400,500,600,700}.ttf`, siehe <https://www.fontshare.com/fonts/switzer>
- Logo (Original-Vektoren): `packaging/nema-icon.svg` (Mark, auch als
  `-mono`/`-dark` für Monochrom/Dark-Mode), `packaging/nema-logo.svg`
  (horizontal), `packaging/nema-logo-gestapelt.svg`, `packaging/nema-favicon.svg`.
  PNGs daraus: `packaging/render_icons.sh` (App-Icon/Fenster-Icon: `nema.png` 256px)

## Sicherheitsmodell

- Namenslisten und die Zuordnung Echtname ↔ Pseudonym liegen in einem
  **verschlüsselten Tresor** (AES-256-GCM) im App-Datenverzeichnis
  (`%APPDATA%/nema`, `~/Library/Application Support/nema`, `~/.local/share/nema`).
- Der Masterkey liegt im **OS-Schlüsselbund** (Windows Credential Manager /
  macOS Keychain / Linux Secret Service via D-Bus — gnome-keyring/KWallet)
  UND zusätzlich mit einem **Backup-Passwort** verschlüsselt (Argon2id,
  64 MiB) im Tresor abgelegt. Beim Entsperren wird zuerst der Schlüsselbund
  versucht, danach das Backup-Passwort — so ist der Tresor auch nach einem
  Neustart ohne Schlüsselbund (z.B. gesperrtes Login-Keyring) erreichbar.
  Wird der Tresor per Passwort entsperrt, obwohl Keychain-Modus aktiv ist,
  wird der Key automatisch in den Schlüsselbund zurückgeschrieben (Migration).
- Das Audit-Log protokolliert Läufe **nur mit Pseudonymen**, nie mit Klarnamen.
- Die App sendet keine Daten irgendwohin — Export/Import für die KI machst du
  selbst.

## Anonymisierung im Detail

- **DOCX:** Text in allen Teilen (Dokument, Header, Footer, Fußnoten,
  Kommentare) wird ersetzt — auch wenn Namen über mehrere Word-Runs verteilt
  sind. Tracked Changes werden übernommen (Einfügungen bleiben, Löschungen
  entfernt). Metadaten (Autor, Firma, eigene Properties) werden neutralisiert.
- **PDF:** Text-Objekte werden per PDFium direkt umgeschrieben (der Name ist
  danach auch per Copy-Paste nicht mehr auslesbar). Info-/XMP-Metadaten werden
  entfernt. Namen, die über mehrere Text-Objekte verteilt sind, werden als
  Warnung gemeldet. Gescannte PDFs (nur Bild) werden nicht erfasst.
- **Namenerkennung:** exakte Varianten (Vollname, „Nachname, Vorname“, Vor-,
  Nachname, E-Mail, Matrikelnummer), case-insensitiv. Unscharfe Treffer
  (Tippfehler) landen in der **Review-Liste** und werden erst nach deiner
  Bestätigung ersetzt.
- **Pseudonyme** sind pro Projekt konsistent (gleicher Lernender = gleiches
  Pseudonym in allen Dateien eines Projekts).

## Ablauf

1. Tab **Klassen**: Klasse anlegen (z.B. „INF 24a") und die Namensliste importieren
   (CSV/TXT) oder Lernende einzeln erfassen. Das macht man einmal pro Klasse.
   Danach ein Projekt anlegen (z.B. ein Modul oder eine Prüfung) und im Tab
   **Personen** die gewünschten Klassen anklicken. Neue Lernende einer Klasse
   erscheinen automatisch in allen Projekten, die sie verwenden; die Pseudonyme
   sind pro Projekt verschieden.
2. Tab **Anonymisieren** → „Lauf exportieren": Prüfungen rein, Export-Ordner
   raus (nur Pseudonyme). Review-Tab prüfen, falls unscharfe Treffer offen sind.
3. Export-Ordner extern durch die KI bewerten lassen.
4. Tab **Anonymisieren** → „Lauf-Ordner de-anonymisieren": KI-Ergebnisse
   zurückholen mit echten Namen.
5. Tab **Protokoll** zeigt alle Läufe (verschlüsselt, nur Pseudonyme).

## Installation

### Linux
- **AppImage:** `nema-x86_64.AppImage` laden, ausführbar machen, starten.
  (Enthält `libpdfium.so` bereits.)
- Alternativ: Binary + `libpdfium.so` ins gleiche Verzeichnis legen
  (die App sucht die Bibliothek neben sich).

### Windows
- ZIP entpacken (`nema-app.exe` + `pdfium.dll` liegen beieinander), `nema-app.exe` starten.

### macOS
- DMG mounten, Nema in den Programme-Ordner ziehen. `libpdfium.dylib` liegt
  neben dem Binary im Bundle.

## PDFium-Bibliothek

Die PDF-Verarbeitung braucht Googles PDFium (`libpdfium.so` / `pdfium.dll` /
`libpdfium.dylib`). Die App sucht sie in dieser Reihenfolge:
1. Umgebungsvariable `NEMA_PDFIUM_LIB`
2. neben der ausführbaren Datei
3. Systempfad

Bezugsquelle: <https://github.com/bblanchon/pdfium-binaries/releases>

## Aus dem Source bauen

```sh
cargo build --release -p nema-app
# Tests (PDF-Tests brauchen libpdfium: NEMA_PDFIUM_LIB=/pfad/libpdfium.so cargo test -p nema-core)
cargo test -p nema-core
# Linux-AppImage:
./packaging/build-appimage.sh /pfad/libpdfium.so /pfad/appimagetool
```

CI baut bei jedem Tag `v*` automatisch AppImage (Linux), ZIP (Windows) und
DMG (macOS) — siehe `.github/workflows/release.yml`.

## Code-Signing (optional, später)

- Windows: EV-Zertifikat (~300–600 €/Jahr) für SmartScreen ohne Warnung;
  für Open Source gratis via <https://signpath.io>.
- macOS: Apple Developer Program (99 $/Jahr) + Notarisierung.
- Linux: nicht nötig.

## Projektstruktur

```
core/   nema-core — gesamte Logik, GUI-unabhängig und getestet
        crypto.rs store.rs project.rs person.rs pseudonym.rs mapping.rs
        matcher.rs docx.rs pdf.rs fuzzy.rs csv_import.rs runs.rs
app/    nema-app  — egui-Oberfläche (dünne Schicht über nema-core)
packaging/          AppImage-Build, Desktop-Datei, Icon
```
