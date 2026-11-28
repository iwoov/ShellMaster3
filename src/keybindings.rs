// 全局快捷键管理
// 集中管理应用级别的快捷键绑定

use gpui_kit::{actions, App, KeyBinding};

// 定义全局 actions
actions!(app, [Quit]);

// 定义 SFTP 内置编辑器上下文 actions（仅在 key_context("SftpEditor") 中生效，不与终端冲突）
actions!(sftp_editor, [SaveFile, CloseEditorTab]);

/// 初始化全局快捷键
/// 注册应用级别的快捷键绑定
pub fn init(cx: &mut App) {
    // 注册全局快捷键与局部编辑器快捷键
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        // Cmd+Q 退出应用
        KeyBinding::new("cmd-q", Quit, None),
        // Cmd+S 保存当前编辑的远程文件
        KeyBinding::new("cmd-s", SaveFile, Some("SftpEditor")),
        // Cmd+W 关闭当前编辑的远程文件标签
        KeyBinding::new("cmd-w", CloseEditorTab, Some("SftpEditor")),
    ]);

    #[cfg(not(target_os = "macos"))]
    cx.bind_keys([
        // Ctrl+Q 退出应用 (Windows/Linux)
        KeyBinding::new("ctrl-q", Quit, None),
        // Ctrl+S 保存当前编辑的远程文件
        KeyBinding::new("ctrl-s", SaveFile, Some("SftpEditor")),
        // Ctrl+W 关闭当前编辑的远程文件标签
        KeyBinding::new("ctrl-w", CloseEditorTab, Some("SftpEditor")),
    ]);

    // 注册 action 处理器
    cx.on_action(|_: &Quit, cx| {
        cx.quit();
    });
}
