// SPDX-License-Identifier: GPL-3.0-or-later
//! Visual design: palette, fonts, egui style, and treemap colours.

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, Stroke, TextStyle};
use std::sync::Arc;

// ---------------------------------------------------------------- UI palette

pub const BG: Color32 = Color32::from_rgb(16, 16, 19);
pub const SURFACE: Color32 = Color32::from_rgb(24, 24, 28);
pub const SURFACE2: Color32 = Color32::from_rgb(33, 33, 39);
pub const SURFACE3: Color32 = Color32::from_rgb(44, 44, 52);
pub const BORDER: Color32 = Color32::from_rgb(48, 48, 56);
pub const TEXT: Color32 = Color32::from_rgb(234, 234, 240);
pub const TEXT_DIM: Color32 = Color32::from_rgb(150, 150, 162);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(104, 104, 116);
pub const ACCENT: Color32 = Color32::from_rgb(122, 162, 255);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(52, 70, 118);
pub const WARN: Color32 = Color32::from_rgb(240, 180, 90);
pub const DANGER: Color32 = Color32::from_rgb(235, 105, 95);
pub const DARK_TEXT: Color32 = Color32::from_rgb(18, 18, 22);


pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}
pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn font_bold(size: f32) -> FontId {
    FontId::new(size, bold())
}

// ---------------------------------------------------------------- fonts

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "inter".into(),
        Arc::new(egui::FontData::from_static(include_bytes!("../assets/Inter-Regular.otf"))),
    );
    fonts.font_data.insert(
        "inter-semibold".into(),
        Arc::new(egui::FontData::from_static(include_bytes!("../assets/Inter-SemiBold.otf"))),
    );
    fonts.font_data.insert(
        "phosphor".into(),
        Arc::new(egui_phosphor::Variant::Regular.font_data()),
    );

    // fallbacks from the system for scripts Inter lacks (CJK, ...)
    let candidates = [
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
    ];
    let mut fallbacks = Vec::new();
    for (i, path) in candidates.iter().enumerate() {
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("sys-fallback-{i}");
            fonts.font_data.insert(name.clone(), Arc::new(egui::FontData::from_owned(bytes)));
            fallbacks.push(name);
        }
    }

    let defaults = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut prop = vec!["inter".to_string(), "phosphor".to_string()];
    prop.extend(defaults.iter().cloned());
    prop.extend(fallbacks.iter().cloned());
    let mut bold_fam = vec!["inter-semibold".to_string(), "phosphor".to_string()];
    bold_fam.extend(defaults.iter().cloned());
    bold_fam.extend(fallbacks.iter().cloned());
    fonts.families.insert(FontFamily::Proportional, prop);
    fonts.families.insert(bold(), bold_fam);
    if let Some(mono) = fonts.families.get_mut(&FontFamily::Monospace) {
        mono.extend(fallbacks.iter().cloned());
    }
    ctx.set_fonts(fonts);
}

// ---------------------------------------------------------------- style

pub fn apply_style(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.text_styles = [
            (TextStyle::Heading, font_bold(22.0)),
            (TextStyle::Body, font(13.5)),
            (TextStyle::Button, font(13.5)),
            (TextStyle::Small, font(11.5)),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(6.0, 6.0);
        style.spacing.button_padding = egui::vec2(9.0, 5.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.window_margin = Margin::same(10);

        let v = &mut style.visuals;
        v.dark_mode = true;
        v.override_text_color = None;
        v.panel_fill = BG;
        v.window_fill = SURFACE2;
        v.window_stroke = Stroke::new(1.0_f32, BORDER);
        v.window_corner_radius = CornerRadius::same(10);
        v.menu_corner_radius = CornerRadius::same(10);
        v.window_shadow = egui::epaint::Shadow { offset: [0, 6], blur: 24, spread: 0, color: Color32::from_black_alpha(110) };
        v.popup_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(100) };
        v.extreme_bg_color = Color32::from_rgb(20, 20, 24);
        v.faint_bg_color = SURFACE;
        v.code_bg_color = SURFACE2;
        v.hyperlink_color = ACCENT;
        v.warn_fg_color = WARN;
        v.error_fg_color = DANGER;
        v.selection.bg_fill = ACCENT_DIM;
        v.selection.stroke = Stroke::new(1.0_f32, ACCENT);

        let r = CornerRadius::same(7);
        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE;
        w.noninteractive.weak_bg_fill = SURFACE;
        w.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0_f32, TEXT_DIM);
        w.noninteractive.corner_radius = r;

        w.inactive.bg_fill = SURFACE3;
        w.inactive.weak_bg_fill = SURFACE2;
        w.inactive.bg_stroke = Stroke::NONE;
        w.inactive.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.inactive.corner_radius = r;

        w.hovered.bg_fill = Color32::from_rgb(58, 58, 68);
        w.hovered.weak_bg_fill = Color32::from_rgb(50, 50, 60);
        w.hovered.bg_stroke = Stroke::NONE;
        w.hovered.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
        w.hovered.corner_radius = r;
        w.hovered.expansion = 0.0;

        w.active.bg_fill = Color32::from_rgb(70, 70, 82);
        w.active.weak_bg_fill = Color32::from_rgb(64, 64, 76);
        w.active.bg_stroke = Stroke::NONE;
        w.active.fg_stroke = Stroke::new(1.0_f32, Color32::WHITE);
        w.active.corner_radius = r;
        w.active.expansion = 0.0;

        w.open.bg_fill = SURFACE3;
        w.open.weak_bg_fill = SURFACE3;
        w.open.corner_radius = r;
        w.open.fg_stroke = Stroke::new(1.0_f32, TEXT);
    });
}

// ---------------------------------------------------------------- colour maths

pub fn hsl(h: f32, s: f32, l: f32) -> Color32 {
    let h = h.rem_euclid(360.0) / 360.0;
    let s = s.clamp(0.0, 1.0);
    let l = l.clamp(0.0, 1.0);
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let c = |x: f32| (x * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(c(f(h + 1.0 / 3.0)), c(f(h)), c(f(h - 1.0 / 3.0)))
}

pub fn luminance(c: Color32) -> f32 {
    (0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32) / 255.0
}

/// Text colour with good contrast on `bg`.
pub fn text_on(bg: Color32) -> (Color32, Color32) {
    if luminance(bg) > 0.48 {
        (DARK_TEXT, Color32::from_rgba_unmultiplied(18, 18, 22, 170))
    } else {
        (TEXT, Color32::from_rgba_unmultiplied(234, 234, 240, 150))
    }
}

/// Distinct, muted hues for the top-level sections of the current view.
/// (hue in degrees, saturation)
pub const SECTIONS: [(f32, f32); 10] = [
    (214.0, 0.62), // blue
    (168.0, 0.48), // teal
    (262.0, 0.48), // violet
    (32.0, 0.72),  // amber
    (342.0, 0.55), // rose
    (132.0, 0.40), // green
    (192.0, 0.58), // cyan
    (12.0, 0.62),  // coral
    (292.0, 0.42), // orchid
    (52.0, 0.55),  // sand
];
