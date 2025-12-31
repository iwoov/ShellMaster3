// 按键绑定面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::i18n;

use super::super::SettingsDialogState;

/// 定义快捷键配置结构
struct KeyBindingItem {
    action: &'static str,
    shortcut_mac: &'static str,
    shortcut_other: &'static str,
}

/// 获取全局快捷键列表
fn get_global_keybindings() -> Vec<KeyBindingItem> {
    vec![KeyBindingItem {
        action: "settings.keybindings.quit",
        shortcut_mac: "⌘Q",
        shortcut_other: "Ctrl+Q",
    }]
}

/// 获取终端快捷键列表
fn get_terminal_keybindings() -> Vec<KeyBindingItem> {
    vec![
        KeyBindingItem {
            action: "settings.keybindings.copy",
            shortcut_mac: "⌘C",
            shortcut_other: "Ctrl+C",
        },
        KeyBindingItem {
            action: "settings.keybindings.paste",
            shortcut_mac: "⌘V",
            shortcut_other: "Ctrl+V",
        },
    ]
}

/// 渲染按键绑定面板
pub fn render_keybindings_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let lang = &state.read(cx).settings.theme.language;
    let border_color = cx.theme().border;
    let muted_fg = cx.theme().muted_foreground;
    let text_color = cx.theme().foreground;
    let bg_hover = cx.theme().secondary;

    // 根据平台选择不同的快捷键显示
    #[cfg(target_os = "macos")]
    let is_mac = true;
    #[cfg(not(target_os = "macos"))]
    let is_mac = false;

    let global_bindings = get_global_keybindings();
    let terminal_bindings = get_terminal_keybindings();

    // 渲染快捷键分组的辅助闭包
    let render_keybinding_section =
        |title_key: &'static str, bindings: Vec<KeyBindingItem>| -> Div {
            div()
                .flex()
                .flex_col()
                .gap_2()
                // 分组标题
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(text_color)
                        .child(i18n::t(lang, title_key)),
                )
                // 快捷键列表
                .child(
                    div()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(border_color)
                        .overflow_hidden()
                        .children(bindings.into_iter().enumerate().map(|(idx, item)| {
                            let shortcut = if is_mac {
                                item.shortcut_mac
                            } else {
                                item.shortcut_other
                            };

                            div()
                                .w_full()
                                .px_3()
                                .py_2()
                                .flex()
                                .items_center()
                                .justify_between()
                                .when(idx > 0, |s| s.border_t_1().border_color(border_color))
                                .hover(|s| s.bg(bg_hover))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(text_color)
                                        .child(i18n::t(lang, item.action)),
                                )
                                .child(
                                    div()
                                        .px_2()
                                        .py(px(2.))
                                        .rounded(px(4.))
                                        .bg(cx.theme().muted)
                                        .text_xs()
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(muted_fg)
                                        .child(shortcut),
                                )
                        })),
                )
        };

    div()
        .flex()
        .flex_col()
        .gap_4()
        // 全局快捷键分组
        .child(render_keybinding_section(
            "settings.keybindings.global_title",
            global_bindings,
        ))
        // 终端快捷键分组
        .child(render_keybinding_section(
            "settings.keybindings.terminal_title",
            terminal_bindings,
        ))
        // 底部提示
        .child(
            div()
                .mt_4()
                .text_sm()
                .text_color(muted_fg)
                .child(i18n::t(lang, "settings.keybindings.more_coming_soon")),
        )
}
