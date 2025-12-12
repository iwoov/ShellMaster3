// 终端颜色转换 - ANSI 颜色到 GPUI Hsla

use gpui::Hsla;

/// 16 色 ANSI 调色板（One Dark 风格）
pub const ANSI_COLORS: [[u8; 3]; 16] = [
    // 普通颜色 (0-7)
    [40, 44, 52],    // Black
    [224, 108, 117], // Red
    [152, 195, 121], // Green
    [229, 192, 123], // Yellow
    [97, 175, 239],  // Blue
    [198, 120, 221], // Magenta
    [86, 182, 194],  // Cyan
    [171, 178, 191], // White
    // 亮色 (8-15)
    [92, 99, 112],   // Bright Black
    [224, 108, 117], // Bright Red
    [152, 195, 121], // Bright Green
    [229, 192, 123], // Bright Yellow
    [97, 175, 239],  // Bright Blue
    [198, 120, 221], // Bright Magenta
    [86, 182, 194],  // Bright Cyan
    [255, 255, 255], // Bright White
];

/// 从十六进制颜色字符串解析为 Hsla
pub fn hex_to_hsla(hex: &str) -> Hsla {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return gpui::white();
    }

    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);

    rgb_to_hsla(r, g, b)
}

/// 从 RGB 转换为 Hsla
pub fn rgb_to_hsla(r: u8, g: u8, b: u8) -> Hsla {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f32::EPSILON {
        // 灰度
        Hsla {
            h: 0.0,
            s: 0.0,
            l,
            a: 1.0,
        }
    } else {
        let d = max - min;
        let s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };

        let h = if (max - r).abs() < f32::EPSILON {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if (max - g).abs() < f32::EPSILON {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };

        Hsla {
            h: h / 6.0,
            s,
            l,
            a: 1.0,
        }
    }
}

/// 从 ANSI 索引颜色获取 Hsla
pub fn ansi_indexed_color(index: u8) -> Hsla {
    if index < 16 {
        // 标准 16 色
        let [r, g, b] = ANSI_COLORS[index as usize];
        rgb_to_hsla(r, g, b)
    } else if index < 232 {
        // 216 色立方体 (6x6x6)
        let idx = index - 16;
        let r = ((idx / 36) % 6) * 51;
        let g = ((idx / 6) % 6) * 51;
        let b = (idx % 6) * 51;
        rgb_to_hsla(r, g, b)
    } else {
        // 24 级灰度
        let gray = (index - 232) * 10 + 8;
        rgb_to_hsla(gray, gray, gray)
    }
}

/// 从 alacritty 的 Rgb 转换为 Hsla
pub fn alac_rgb_to_hsla(rgb: alacritty_terminal::vte::ansi::Rgb) -> Hsla {
    rgb_to_hsla(rgb.r, rgb.g, rgb.b)
}
