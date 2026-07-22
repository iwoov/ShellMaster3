// 键盘映射 - 将 GPUI 按键事件转换为终端转义序列

use alacritty_terminal::term::TermMode;
use gpui::{Keystroke, Modifiers};

/// 将 GPUI Keystroke 转换为终端转义序列
pub fn keystroke_to_escape(
    keystroke: &Keystroke,
    modifiers: &Modifiers,
    mode: TermMode,
) -> Option<Vec<u8>> {
    // 平台快捷键（macOS Command / Windows Super）应留给应用层 key binding，
    // 避免 Cmd-C/Cmd-V/Cmd-Arrow 被误发送为普通字符或方向键。
    if modifiers.platform {
        return None;
    }

    if let Some(bytes) = named_key_to_escape(&keystroke.key, modifiers, mode) {
        return Some(bytes);
    }

    // 检查 Ctrl 组合键
    if modifiers.control {
        return ctrl_key_to_bytes(&keystroke.key);
    }

    // 检查 Alt/Meta 组合键 (发送 ESC 前缀)
    if modifiers.alt {
        // 对于 Alt 组合键，优先使用 key_char
        if let Some(ref key_char) = keystroke.key_char {
            let mut bytes = vec![0x1b]; // ESC prefix
            bytes.extend_from_slice(key_char.as_bytes());
            return Some(bytes);
        }
        return alt_key_to_bytes(&keystroke.key);
    }

    // 对于其他按键，优先使用 key_char（这是实际输入的字符，已经应用了 Shift 等修饰键）
    if let Some(ref key_char) = keystroke.key_char {
        Some(key_char.as_bytes().to_vec())
    } else if keystroke.key.len() == 1 {
        // 回退：使用 key（但这是逻辑键名，不包含 Shift 状态）
        Some(keystroke.key.as_bytes().to_vec())
    } else {
        // 未知按键
        None
    }
}

/// 将具名按键转换为终端转义序列。
pub fn named_key_to_escape(key: &str, modifiers: &Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    if modifiers.platform {
        return None;
    }

    match key {
        "enter" => Some(vec![0x0d]),     // CR
        "backspace" => Some(vec![0x7f]), // DEL
        "tab" if modifiers.shift && !modifiers.alt && !modifiers.control && !modifiers.function => {
            Some(b"\x1b[Z".to_vec())
        }
        "tab" if !modifiers.alt && !modifiers.control && !modifiers.function => Some(vec![0x09]),
        "escape" => Some(vec![0x1b]),
        "space" if no_terminal_modifiers(modifiers) => Some(vec![0x20]),

        "up" => arrow_key(b'A', modifiers, mode),
        "down" => arrow_key(b'B', modifiers, mode),
        "right" => arrow_key(b'C', modifiers, mode),
        "left" => arrow_key(b'D', modifiers, mode),

        "home" => home_end_key(b'H', modifiers, mode),
        "end" => home_end_key(b'F', modifiers, mode),
        "insert" => tilde_key(2, modifiers),
        "delete" => tilde_key(3, modifiers),
        "pageup" => tilde_key(5, modifiers),
        "pagedown" => tilde_key(6, modifiers),

        "f1" => function_key(b'P', None, modifiers),
        "f2" => function_key(b'Q', None, modifiers),
        "f3" => function_key(b'R', None, modifiers),
        "f4" => function_key(b'S', None, modifiers),
        "f5" => function_key(0, Some(15), modifiers),
        "f6" => function_key(0, Some(17), modifiers),
        "f7" => function_key(0, Some(18), modifiers),
        "f8" => function_key(0, Some(19), modifiers),
        "f9" => function_key(0, Some(20), modifiers),
        "f10" => function_key(0, Some(21), modifiers),
        "f11" => function_key(0, Some(23), modifiers),
        "f12" => function_key(0, Some(24), modifiers),

        _ => None,
    }
}

/// 构建粘贴字节；启用 bracketed paste 时按终端协议包裹。
pub fn paste_to_bytes(text: &str, mode: TermMode) -> Vec<u8> {
    if mode.contains(TermMode::BRACKETED_PASTE) {
        let mut bytes = Vec::with_capacity(text.len() + 12);
        bytes.extend_from_slice(b"\x1b[200~");
        bytes.extend_from_slice(text.as_bytes());
        bytes.extend_from_slice(b"\x1b[201~");
        bytes
    } else {
        text.as_bytes().to_vec()
    }
}

fn arrow_key(cmd: u8, modifiers: &Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    if let Some(param) = modifier_param(modifiers) {
        Some(format!("\x1b[1;{}{}", param, cmd as char).into_bytes())
    } else if mode.contains(TermMode::APP_CURSOR) {
        Some(vec![0x1b, b'O', cmd])
    } else {
        Some(vec![0x1b, b'[', cmd])
    }
}

fn home_end_key(cmd: u8, modifiers: &Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    if let Some(param) = modifier_param(modifiers) {
        Some(format!("\x1b[1;{}{}", param, cmd as char).into_bytes())
    } else if mode.contains(TermMode::APP_CURSOR) {
        Some(vec![0x1b, b'O', cmd])
    } else {
        Some(vec![0x1b, b'[', cmd])
    }
}

fn tilde_key(number: u8, modifiers: &Modifiers) -> Option<Vec<u8>> {
    if let Some(param) = modifier_param(modifiers) {
        Some(format!("\x1b[{};{}~", number, param).into_bytes())
    } else {
        Some(format!("\x1b[{}~", number).into_bytes())
    }
}

fn function_key(cmd: u8, number: Option<u8>, modifiers: &Modifiers) -> Option<Vec<u8>> {
    match (number, modifier_param(modifiers)) {
        (None, None) => Some(vec![0x1b, b'O', cmd]),
        (None, Some(param)) => Some(format!("\x1b[1;{}{}", param, cmd as char).into_bytes()),
        (Some(number), None) => Some(format!("\x1b[{}~", number).into_bytes()),
        (Some(number), Some(param)) => Some(format!("\x1b[{};{}~", number, param).into_bytes()),
    }
}

fn no_terminal_modifiers(modifiers: &Modifiers) -> bool {
    !modifiers.shift
        && !modifiers.alt
        && !modifiers.control
        && !modifiers.platform
        && !modifiers.function
}

fn modifier_param(modifiers: &Modifiers) -> Option<u8> {
    let mut value = 1;
    if modifiers.shift {
        value += 1;
    }
    if modifiers.alt {
        value += 2;
    }
    if modifiers.control {
        value += 4;
    }

    (value > 1).then_some(value)
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
        "z" => Some(vec![0x1a]),                 // SIGSTOP
        "space" | "@" | "2" => Some(vec![0x00]), // NUL
        "[" => Some(vec![0x1b]),                 // Escape
        "3" => Some(vec![0x1b]),
        "\\" => Some(vec![0x1c]),
        "4" => Some(vec![0x1c]),
        "]" => Some(vec![0x1d]),
        "5" => Some(vec![0x1d]),
        "^" => Some(vec![0x1e]),
        "6" => Some(vec![0x1e]),
        "_" => Some(vec![0x1f]),
        "7" => Some(vec![0x1f]),
        "8" | "?" => Some(vec![0x7f]),
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
