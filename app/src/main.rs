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

/// Registriert Switzer (Fontshare, free license) als App-Font.
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let regular: &'static [u8] = include_bytes!("../fonts/Switzer-400.ttf");
    let medium: &'static [u8] = include_bytes!("../fonts/Switzer-500.ttf");
    let semibold: &'static [u8] = include_bytes!("../fonts/Switzer-600.ttf");
    let bold: &'static [u8] = include_bytes!("../fonts/Switzer-700.ttf");
    fonts.font_data.insert("Switzer".into(), egui::FontData::from_static(regular).into());
    fonts.font_data.insert("Switzer Medium".into(), egui::FontData::from_static(medium).into());
    fonts.font_data.insert("Switzer SemiBold".into(), egui::FontData::from_static(semibold).into());
    fonts.font_data.insert("Switzer Bold".into(), egui::FontData::from_static(bold).into());
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "Switzer".to_string());
    // Hack als letzter Fallback: Pfeile (→ ←) hat Switzer nicht
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push("Hack".to_string());
    for name in ["Switzer Medium", "Switzer SemiBold", "Switzer Bold"] {
        fonts.families.insert(
            egui::FontFamily::Name(name.into()),
            vec![name.into()],
        );
    }
    ctx.set_fonts(fonts);

    let mut style = (*ctx.style()).clone();
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(24.0, FontFamily::Name("Switzer SemiBold".into())),
    );
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(15.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(15.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(14.0, FontFamily::Monospace),
    );
    ctx.set_style(style);
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
        Box::new(|cc| {
            setup_fonts(&cc.egui_ctx);
            let mode = theme::system_mode(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx, mode);
            Ok(Box::new(app::App::new()))
        }),
    )
}
