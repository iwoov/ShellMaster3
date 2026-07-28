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

/// 从十六进制颜色字符串解析为 (r, g, b)
pub fn hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let hex = hex.trim_start_matches('#');
    if hex.len() < 6 {
        return (255, 255, 255);
    }

    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);

    (r, g, b)
}

/// 从十六进制颜色字符串解析为 Hsla
pub fn hex_to_hsla(hex: &str) -> Hsla {
    let (r, g, b) = hex_to_rgb(hex);
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

/// 从 ANSI 索引颜色获取 (r, g, b)
pub fn ansi_indexed_rgb(index: u8) -> (u8, u8, u8) {
    if index < 16 {
        // 标准 16 色
        let [r, g, b] = ANSI_COLORS[index as usize];
        (r, g, b)
    } else if index < 232 {
        // 216 色立方体 (6x6x6)
        let idx = index - 16;
        let r = ((idx / 36) % 6) * 51;
        let g = ((idx / 6) % 6) * 51;
        let b = (idx % 6) * 51;
        (r, g, b)
    } else {
        // 24 级灰度
        let gray = (index - 232) * 10 + 8;
        (gray, gray, gray)
    }
}

/// 从 ANSI 索引颜色获取 Hsla
pub fn ansi_indexed_color(index: u8) -> Hsla {
    let (r, g, b) = ansi_indexed_rgb(index);
    rgb_to_hsla(r, g, b)
}

/// 从 alacritty 的 Rgb 转换为 Hsla
pub fn alac_rgb_to_hsla(rgb: alacritty_terminal::vte::ansi::Rgb) -> Hsla {
    rgb_to_hsla(rgb.r, rgb.g, rgb.b)
}

// ==================== 终端主题调色板 ====================

/// 一个终端主题的完整调色板。
#[derive(Clone, Debug)]
pub struct TerminalPalette {
    pub foreground: (u8, u8, u8),
    pub background: (u8, u8, u8),
    pub cursor: (u8, u8, u8),
    pub selection: (u8, u8, u8),
    /// 16 色 ANSI 调色板（0-7 普通，8-15 亮色）。
    pub ansi: [(u8, u8, u8); 16],
}

impl TerminalPalette {
    /// 获取 0-15 号 ANSI 颜色的 Hsla（越界回退到前景色）。
    pub fn ansi_hsla(&self, idx: u8) -> Hsla {
        let (r, g, b) = self
            .ansi
            .get(idx as usize)
            .copied()
            .unwrap_or(self.foreground);
        rgb_to_hsla(r, g, b)
    }
}

/// 从 16 个 hex 字符串构建 ANSI 调色板。
fn ansi_from_hex(hex: [&str; 16]) -> [(u8, u8, u8); 16] {
    std::array::from_fn(|i| hex_to_rgb(hex[i]))
}

/// 按主题名返回完整调色板；未知主题回退到 One Dark。
pub fn palette_for(scheme: &str) -> TerminalPalette {
    match scheme {
        "Dracula" => TerminalPalette {
            foreground: hex_to_rgb("#f8f8f2"),
            background: hex_to_rgb("#282a36"),
            cursor: hex_to_rgb("#f8f8f2"),
            selection: hex_to_rgb("#44475a"),
            ansi: ansi_from_hex([
                "#21222c", "#ff5555", "#50fa7b", "#f1fa8c", "#bd93f9", "#ff79c6", "#8be9fd",
                "#f8f8f2", "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5", "#d6acff", "#ff92df",
                "#a4ffff", "#ffffff",
            ]),
        },
        "Solarized Dark" => TerminalPalette {
            foreground: hex_to_rgb("#839496"),
            background: hex_to_rgb("#002b36"),
            cursor: hex_to_rgb("#93a1a1"),
            selection: hex_to_rgb("#073642"),
            ansi: ansi_from_hex([
                "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198",
                "#eee8d5", "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4",
                "#93a1a1", "#fdf6e3",
            ]),
        },
        "Solarized Light" => TerminalPalette {
            foreground: hex_to_rgb("#657b83"),
            background: hex_to_rgb("#fdf6e3"),
            cursor: hex_to_rgb("#586e75"),
            selection: hex_to_rgb("#eee8d5"),
            ansi: ansi_from_hex([
                "#073642", "#dc322f", "#859900", "#b58900", "#268bd2", "#d33682", "#2aa198",
                "#eee8d5", "#002b36", "#cb4b16", "#586e75", "#657b83", "#839496", "#6c71c4",
                "#93a1a1", "#fdf6e3",
            ]),
        },
        "Nord" => TerminalPalette {
            foreground: hex_to_rgb("#d8dee9"),
            background: hex_to_rgb("#2e3440"),
            cursor: hex_to_rgb("#d8dee9"),
            selection: hex_to_rgb("#434c5e"),
            ansi: ansi_from_hex([
                "#3b4252", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead", "#88c0d0",
                "#e5e9f0", "#4c566a", "#bf616a", "#a3be8c", "#ebcb8b", "#81a1c1", "#b48ead",
                "#8fbcbb", "#eceff4",
            ]),
        },
        "Monokai" => TerminalPalette {
            foreground: hex_to_rgb("#f8f8f2"),
            background: hex_to_rgb("#272822"),
            cursor: hex_to_rgb("#f8f8f0"),
            selection: hex_to_rgb("#49483e"),
            ansi: ansi_from_hex([
                "#272822", "#f92672", "#a6e22e", "#f4bf75", "#66d9ef", "#ae81ff", "#a1efe4",
                "#f8f8f2", "#75715e", "#f92672", "#a6e22e", "#f4bf75", "#66d9ef", "#ae81ff",
                "#a1efe4", "#f9f8f5",
            ]),
        },
        "Gruvbox Dark" => TerminalPalette {
            foreground: hex_to_rgb("#ebdbb2"),
            background: hex_to_rgb("#282828"),
            cursor: hex_to_rgb("#ebdbb2"),
            selection: hex_to_rgb("#504945"),
            ansi: ansi_from_hex([
                "#282828", "#cc241d", "#98971a", "#d79921", "#458588", "#b16286", "#689d6a",
                "#a89984", "#928374", "#fb4934", "#b8bb26", "#fabd2f", "#83a598", "#d3869b",
                "#8ec07c", "#ebdbb2",
            ]),
        },
        "Tokyo Night" => TerminalPalette {
            foreground: hex_to_rgb("#c0caf5"),
            background: hex_to_rgb("#1a1b26"),
            cursor: hex_to_rgb("#c0caf5"),
            selection: hex_to_rgb("#33467c"),
            ansi: ansi_from_hex([
                "#15161e", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7", "#7dcfff",
                "#a9b1d6", "#414868", "#f7768e", "#9ece6a", "#e0af68", "#7aa2f7", "#bb9af7",
                "#7dcfff", "#c0caf5",
            ]),
        },
        "GitHub Dark" => TerminalPalette {
            foreground: hex_to_rgb("#c9d1d9"),
            background: hex_to_rgb("#0d1117"),
            cursor: hex_to_rgb("#c9d1d9"),
            selection: hex_to_rgb("#163356"),
            ansi: ansi_from_hex([
                "#484f58", "#ff7b72", "#3fb950", "#d29922", "#58a6ff", "#bc8cff", "#39c5cf",
                "#b1bac4", "#6e7681", "#ffa198", "#56d364", "#e3b341", "#79c0ff", "#d2a8ff",
                "#56d4dd", "#f0f6fc",
            ]),
        },
        // One Dark（默认）
        _ => TerminalPalette {
            foreground: hex_to_rgb("#abb2bf"),
            background: hex_to_rgb("#282c34"),
            cursor: hex_to_rgb("#528bff"),
            selection: hex_to_rgb("#3e4451"),
            ansi: ansi_from_hex([
                "#282c34", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd", "#56b6c2",
                "#abb2bf", "#5c6370", "#e06c75", "#98c379", "#e5c07b", "#61afef", "#c678dd",
                "#56b6c2", "#ffffff",
            ]),
        },
    }
}
