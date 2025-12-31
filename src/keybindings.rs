// 全局快捷键管理
// 集中管理应用级别的快捷键绑定

use gpui::{actions, App, KeyBinding};

// 定义全局 actions
actions!(app, [Quit]);

/// 初始化全局快捷键
/// 注册应用级别的快捷键绑定
pub fn init(cx: &mut App) {
    // 注册全局快捷键
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        // Cmd+Q 退出应用
        KeyBinding::new("cmd-q", Quit, None),
    ]);

    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        // Ctrl+Q 退出应用 (Windows/Linux)
        KeyBinding::new("ctrl-q", Quit, None),
    ]);

    // 注册 action 处理器
    cx.on_action(|_: &Quit, cx| {
        cx.quit();
    });
}
