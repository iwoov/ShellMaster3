// Terminal 模块 - 基于 alacritty_terminal 的终端模拟器

mod batched_run;
mod colors;
mod keys;
mod renderer;
mod scroll_handle;
mod state;
mod terminal_bridge;

// pub use batched_run::*; // 内部使用，不导出
pub use colors::*;
pub use keys::*;
pub use renderer::*;
pub use scroll_handle::*;
pub use state::*;
pub use terminal_bridge::*;

use gpui::{actions, App, KeyBinding};

// 定义终端专用 actions
actions!(
    terminal,
    [SendTab, SendEscape, SendEnter, SendUp, SendDown, SendLeft, SendRight,]
);

/// 终端上下文名称
pub const TERMINAL_CONTEXT: &str = "Terminal";

/// 初始化终端模块
/// 注册 Terminal 上下文的按键绑定，确保特殊按键（如 Tab）能正确发送到 PTY
pub fn init(cx: &mut App) {
    // 在 Terminal 上下文中，绑定特殊按键
    // 这些绑定会覆盖默认的焦点切换行为
    cx.bind_keys([
        // Tab 键：发送到终端而非切换焦点
        KeyBinding::new("tab", SendTab, Some(TERMINAL_CONTEXT)),
        // 其他常用按键
        KeyBinding::new("escape", SendEscape, Some(TERMINAL_CONTEXT)),
        KeyBinding::new("enter", SendEnter, Some(TERMINAL_CONTEXT)),
        KeyBinding::new("up", SendUp, Some(TERMINAL_CONTEXT)),
        KeyBinding::new("down", SendDown, Some(TERMINAL_CONTEXT)),
        KeyBinding::new("left", SendLeft, Some(TERMINAL_CONTEXT)),
        KeyBinding::new("right", SendRight, Some(TERMINAL_CONTEXT)),
    ]);
}
