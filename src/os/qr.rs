use bevy_egui::egui;

pub const QUIET_MODULES: usize = 4;
pub const SCALE: usize = 6;

pub fn image(text: &str) -> Result<egui::ColorImage, String> {
    let code = qrcode::QrCode::new(text.as_bytes()).map_err(|error| error.to_string())?;
    let colors = code.to_colors();
    let modules = code.width();
    let side = (modules + QUIET_MODULES * 2) * SCALE;
    let mut pixels = vec![egui::Color32::WHITE; side * side];
    for y in 0..modules {
        for x in 0..modules {
            if colors[y * modules + x] != qrcode::Color::Dark {
                continue;
            }
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    let px = (x + QUIET_MODULES) * SCALE + dx;
                    let py = (y + QUIET_MODULES) * SCALE + dy;
                    pixels[py * side + px] = egui::Color32::BLACK;
                }
            }
        }
    }
    Ok(egui::ColorImage {
        size: [side, side],
        pixels,
        source_size: egui::vec2(side as f32, side as f32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_paints_a_square_with_a_white_quiet_zone_and_dark_modules() {
        let image = image("CMAAAAAAAAAQCAAAFIAACAIAABMQAAYGAAAC2LR2HRPWOAIDAASAEBAAIVHAGAQAAAVV2AIDAAVACBAAKMBQEAAANF5QIAYAEA25OAOZAEBAIAF5AHIQCAACAEBQAIACAUACQPIDAIAAA5D3AEBQASQBAQAFGAIEABJA").unwrap();
        let [w, h] = image.size;
        assert_eq!(w, h);
        assert_eq!(w % SCALE, 0);
        let quiet = QUIET_MODULES * SCALE;
        assert!(image.pixels[..quiet * w]
            .iter()
            .all(|pixel| *pixel == egui::Color32::WHITE));
        assert!(image.pixels.contains(&egui::Color32::BLACK));
        assert_eq!(image.pixels[quiet * w + quiet], egui::Color32::BLACK);
    }

    #[test]
    fn an_oversized_text_reports_the_reason() {
        let huge = "x".repeat(8000);
        assert!(image(&huge).is_err());
    }
}
