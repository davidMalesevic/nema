mod app;
mod theme;
mod widgets;

use eframe::egui;

/// Dekodiert das Nema-Logo (packaging/nema.png) als Fenster-Icon.
fn load_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../../packaging/nema.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(256 * 256 * 4)];
    let info = reader.next_frame(&mut buf).ok()?;
    if info.color_type != png::ColorType::Rgba || info.bit_depth != png::BitDepth::Eight {
        return None;
    }
    Some(egui::IconData {
        rgba: buf[..info.buffer_size()].to_vec(),
        width: info.width,
        height: info.height,
    })
}

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();
    // Health-Check ohne GUI (z.B. nach der Installation):
    // `nema-app --check-pdfium` prüft, ob die PDFium-Bibliothek gefunden wird.
    if std::env::args().any(|a| a == "--check-pdfium") {
        match nema_core::pdf::pdfium_instance() {
            Ok(_) => {
                println!("OK: PDFium-Bibliothek geladen.");
                std::process::exit(0);
            }
            Err(e) => {
                eprintln!("FEHLER: {e}");
                std::process::exit(1);
            }
        }
    }
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Nema — Anonymisierung")
        .with_inner_size([1024.0, 680.0])
        .with_min_inner_size([900.0, 600.0]);
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Nema",
        options,
        // Fonts und Theme setzt App::ui im ersten Frame.
        Box::new(|_cc| Ok(Box::new(app::App::new()))),
    )
}
