// 鼠标上报协议 - 将鼠标事件编码为 xterm 鼠标序列（X10 / SGR 1006）
// 当远端应用（vim/tmux/htop/lazygit 等）启用鼠标模式时，把点击/拖动/滚轮上报给应用。

use alacritty_terminal::term::TermMode;
use gpui::Modifiers;

/// 按钮基础编码：左=0 中=1 右=2；滚轮上=64 下=65。
pub const MOUSE_LEFT: u8 = 0;
pub const MOUSE_MIDDLE: u8 = 1;
pub const MOUSE_RIGHT: u8 = 2;
pub const MOUSE_WHEEL_UP: u8 = 64;
pub const MOUSE_WHEEL_DOWN: u8 = 65;

/// 终端是否启用了任意鼠标上报模式（1000/1002/1003）。
pub fn mouse_mode_enabled(mode: TermMode) -> bool {
    mode.intersects(TermMode::MOUSE_MODE)
}

/// 是否应上报鼠标移动：1003（任意移动）总是上报；1002（拖动）仅在按住按钮时上报。
pub fn should_report_motion(mode: TermMode, button_pressed: bool) -> bool {
    mode.contains(TermMode::MOUSE_MOTION)
        || (button_pressed && mode.contains(TermMode::MOUSE_DRAG))
}

/// 将一次鼠标事件编码为上报字节。
///
/// - `base_button`：`MOUSE_*` 常量之一。
/// - `is_motion`：是否为移动/拖动事件（叠加 +32）。
/// - `released`：是否为按钮释放（滚轮恒为 false）。
/// - `col`/`row`：1-based 可视区坐标。
///
/// 未启用鼠标模式时返回 `None`。优先使用 SGR(1006)，否则回退 X10 编码。
pub fn mouse_report_bytes(
    base_button: u8,
    is_motion: bool,
    released: bool,
    col: usize,
    row: usize,
    mods: &Modifiers,
    mode: TermMode,
) -> Option<Vec<u8>> {
    if !mouse_mode_enabled(mode) {
        return None;
    }

    let motion_bit = if is_motion { 32 } else { 0 };
    let mod_bits = (if mods.shift { 4 } else { 0 })
        + (if mods.alt { 8 } else { 0 })
        + (if mods.control { 16 } else { 0 });

    if mode.contains(TermMode::SGR_MOUSE) {
        // SGR(1006)：ESC [ < Cb ; Cx ; Cy M(按下/移动) 或 m(释放)
        let cb = base_button as u16 + motion_bit + mod_bits;
        let action = if released { 'm' } else { 'M' };
        Some(format!("\x1b[<{};{};{}{}", cb, col, row, action).into_bytes())
    } else {
        // X10/normal：ESC [ M Cb Cx Cy，各值 +32；释放用按钮码 3；坐标上限 223。
        let button_code = if released { 3 } else { base_button as u16 };
        let cb = 32 + button_code + motion_bit + mod_bits;
        let cx_byte = 32 + col.min(223);
        let cy_byte = 32 + row.min(223);
        let mut bytes = vec![0x1b, b'[', b'M'];
        bytes.push(cb.min(255) as u8);
        bytes.push(cx_byte as u8);
        bytes.push(cy_byte as u8);
        Some(bytes)
    }
}
