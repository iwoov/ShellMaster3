// 键盘映射 - 将 GPUI 按键事件转换为终端转义序列

use gpui::{Keystroke, Modifiers};

/// 将 GPUI Keystroke 转换为终端转义序列
pub fn keystroke_to_escape(keystroke: &Keystroke, modifiers: &Modifiers) -> Option<Vec<u8>> {
    // 检查 Ctrl 组合键
    if modifiers.control {
        return ctrl_key_to_bytes(&keystroke.key);
    }

    // 检查 Alt/Meta 组合键 (发送 ESC 前缀)
    if modifiers.alt {
        return alt_key_to_bytes(&keystroke.key);
    }

    // 普通按键
    match keystroke.key.as_str() {
        // 特殊按键
        "enter" => Some(vec![0x0d]),     // CR
        "backspace" => Some(vec![0x7f]), // DEL
        "tab" => Some(vec![0x09]),
        "escape" => Some(vec![0x1b]),
        "space" => Some(vec![0x20]),

        // 方向键
        "up" => Some(vec![0x1b, b'[', b'A']),
        "down" => Some(vec![0x1b, b'[', b'B']),
        "right" => Some(vec![0x1b, b'[', b'C']),
        "left" => Some(vec![0x1b, b'[', b'D']),

        // 导航键
        "home" => Some(vec![0x1b, b'[', b'H']),
        "end" => Some(vec![0x1b, b'[', b'F']),
        "pageup" => Some(vec![0x1b, b'[', b'5', b'~']),
        "pagedown" => Some(vec![0x1b, b'[', b'6', b'~']),
        "insert" => Some(vec![0x1b, b'[', b'2', b'~']),
        "delete" => Some(vec![0x1b, b'[', b'3', b'~']),

        // 功能键
        "f1" => Some(vec![0x1b, b'O', b'P']),
        "f2" => Some(vec![0x1b, b'O', b'Q']),
        "f3" => Some(vec![0x1b, b'O', b'R']),
        "f4" => Some(vec![0x1b, b'O', b'S']),
        "f5" => Some(vec![0x1b, b'[', b'1', b'5', b'~']),
        "f6" => Some(vec![0x1b, b'[', b'1', b'7', b'~']),
        "f7" => Some(vec![0x1b, b'[', b'1', b'8', b'~']),
        "f8" => Some(vec![0x1b, b'[', b'1', b'9', b'~']),
        "f9" => Some(vec![0x1b, b'[', b'2', b'0', b'~']),
        "f10" => Some(vec![0x1b, b'[', b'2', b'1', b'~']),
        "f11" => Some(vec![0x1b, b'[', b'2', b'3', b'~']),
        "f12" => Some(vec![0x1b, b'[', b'2', b'4', b'~']),

        // 单个字符
        key if key.len() == 1 => {
            let bytes = key.as_bytes();
            Some(bytes.to_vec())
        }

        // 未知按键
        _ => None,
    }
}

/// Ctrl + 键映射
fn ctrl_key_to_bytes(key: &str) -> Option<Vec<u8>> {
    match key.to_lowercase().as_str() {
        "a" => Some(vec![0x01]),
        "b" => Some(vec![0x02]),
        "c" => Some(vec![0x03]), // SIGINT
        "d" => Some(vec![0x04]), // EOF
        "e" => Some(vec![0x05]),
        "f" => Some(vec![0x06]),
        "g" => Some(vec![0x07]), // Bell
        "h" => Some(vec![0x08]), // Backspace
        "i" => Some(vec![0x09]), // Tab
        "j" => Some(vec![0x0a]), // Line feed
        "k" => Some(vec![0x0b]),
        "l" => Some(vec![0x0c]), // Form feed / clear
        "m" => Some(vec![0x0d]), // Carriage return
        "n" => Some(vec![0x0e]),
        "o" => Some(vec![0x0f]),
        "p" => Some(vec![0x10]),
        "q" => Some(vec![0x11]),
        "r" => Some(vec![0x12]),
        "s" => Some(vec![0x13]),
        "t" => Some(vec![0x14]),
        "u" => Some(vec![0x15]),
        "v" => Some(vec![0x16]),
        "w" => Some(vec![0x17]),
        "x" => Some(vec![0x18]),
        "y" => Some(vec![0x19]),
        "z" => Some(vec![0x1a]), // SIGSTOP
        "[" => Some(vec![0x1b]), // Escape
        "\\" => Some(vec![0x1c]),
        "]" => Some(vec![0x1d]),
        "^" => Some(vec![0x1e]),
        "_" => Some(vec![0x1f]),
        _ => None,
    }
}

/// Alt + 键映射 (发送 ESC 前缀)
fn alt_key_to_bytes(key: &str) -> Option<Vec<u8>> {
    if key.len() == 1 {
        let mut bytes = vec![0x1b]; // ESC prefix
        bytes.extend_from_slice(key.as_bytes());
        Some(bytes)
    } else {
        None
    }
}
