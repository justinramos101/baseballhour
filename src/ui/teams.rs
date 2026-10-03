use ratatui::{
    style::{Color, Style},
    text::Span,
};

use crate::model::Team;

/// Primary and secondary club colors, keyed by the provider's MLB team ID.
/// Minor league clubs fall back to a neutral swatch.
fn colors(id: u32) -> Option<(Color, Color)> {
    let rgb = |hex: u32| {
        let [_, r, g, b] = hex.to_be_bytes();
        Color::Rgb(r, g, b)
    };
    let (primary, secondary) = match id {
        108 => (0xBA_0021, 0x86_3F4F),
        109 => (0xA7_1930, 0xE3_D4AD),
        110 => (0xDF_4601, 0x27_251F),
        111 => (0xBD_3039, 0x0C_2340),
        112 => (0x0E_3386, 0xCC_3433),
        113 => (0xC6_011F, 0xF4_F4F4),
        114 => (0xE5_0022, 0x00_385D),
        115 => (0x5A_2D91, 0xC4_CED4),
        116 => (0xFA_4616, 0x0C_2340),
        117 => (0xEB_6E1F, 0x00_2D62),
        118 => (0x00_4687, 0xBD_9B60),
        119 => (0x00_5A9C, 0xEF_3E42),
        120 => (0xAB_0003, 0x14_225A),
        121 => (0xFF_5910, 0x00_2D72),
        133 => (0xEF_B21E, 0x00_3831),
        134 => (0xFD_B827, 0x27_251F),
        135 => (0xFF_C425, 0x2F_241D),
        136 => (0x00_5C5C, 0x0C_2C56),
        137 => (0xFD_5A1E, 0x27_251F),
        138 => (0xC4_1E3A, 0x0C_2340),
        139 => (0x8F_BCE6, 0x09_2C5C),
        140 => (0xC0_111F, 0x00_3278),
        141 => (0x13_4A8E, 0xE8_291C),
        142 => (0xD3_1145, 0x00_2B5C),
        143 => (0xE8_1828, 0x00_2D72),
        144 => (0xCE_1141, 0x13_274F),
        145 => (0xC4_CED4, 0x27_251F),
        146 => (0x00_A3E0, 0xEF_3340),
        147 => (0xC4_CED3, 0x0C_2340),
        158 => (0xFF_C52F, 0x12_284B),
        _ => return None,
    };
    Some((rgb(primary), rgb(secondary)))
}

/// A stable, muted hue for clubs without a color entry, such as minor
/// league teams, so neighboring rows stay distinguishable.
fn derived(id: u32) -> (Color, Color) {
    let hue = f64::from(id.wrapping_mul(2_654_435_761) % 360);
    let (primary, secondary) = (hsl(hue, 0.45, 0.55), hsl(hue, 0.35, 0.28));
    (primary, secondary)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names,
    reason = "Standard HSL notation; channels are clamped to the u8 range"
)]
fn hsl(hue: f64, saturation: f64, lightness: f64) -> Color {
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue / 60.0;
    let x = chroma * (1.0 - (sector % 2.0 - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let m = lightness - chroma / 2.0;
    let channel = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color::Rgb(channel(r), channel(g), channel(b))
}

/// A one-cell, two-tone club swatch.
pub(super) fn chip(team: &Team) -> Span<'static> {
    let (primary, secondary) = colors(team.id).unwrap_or_else(|| derived(team.id));
    Span::styled("▌", Style::default().fg(primary).bg(secondary))
}
