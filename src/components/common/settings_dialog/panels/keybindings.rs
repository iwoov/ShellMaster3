// 按键绑定面板

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::components::common::input::{kbd, shortcut_keys};
use crate::i18n;

use super::super::helpers::{render_card, render_row, render_section_title};
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
            shortcut_other: "Ctrl+Shift+C",
        },
        KeyBindingItem {
            action: "settings.keybindings.paste",
            shortcut_mac: "⌘V",
            shortcut_other: "Ctrl+Shift+V",
        },
        KeyBindingItem {
            action: "settings.keybindings.search",
            shortcut_mac: "⌘F",
            shortcut_other: "Ctrl+Shift+F",
        },
        KeyBindingItem {
            action: "settings.keybindings.select_all",
            shortcut_mac: "⌘A",
            shortcut_other: "Ctrl+Shift+A",
        },
        KeyBindingItem {
            action: "settings.keybindings.open_url",
            shortcut_mac: "⌘+Click",
            shortcut_other: "Ctrl+Click",
        },
    ]
}

/// 获取 SFTP 与编辑器快捷键列表
fn get_sftp_editor_keybindings() -> Vec<KeyBindingItem> {
    vec![
        KeyBindingItem {
            action: "settings.keybindings.sftp_filter",
            shortcut_mac: "⌘F",
            shortcut_other: "Ctrl+F",
        },
        KeyBindingItem {
            action: "settings.keybindings.editor_save",
            shortcut_mac: "⌘S",
            shortcut_other: "Ctrl+S",
        },
        KeyBindingItem {
            action: "settings.keybindings.editor_close",
            shortcut_mac: "⌘W",
            shortcut_other: "Ctrl+W",
        },
    ]
}

/// 渲染按键绑定面板
pub fn render_keybindings_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let lang = state.read(cx).settings.theme.language.clone();

    // 根据平台选择不同的快捷键显示
    let is_mac = cfg!(target_os = "macos");

    let section = |title_key: &'static str, bindings: Vec<KeyBindingItem>| -> Div {
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child(render_section_title(i18n::t(&lang, title_key), cx))
            .child(render_card(cx).children(bindings.into_iter().map(|item| {
                let shortcut = if is_mac {
                    item.shortcut_mac
                } else {
                    item.shortcut_other
                };
                render_row(
                    i18n::t(&lang, item.action),
                    None,
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .children(shortcut_keys(shortcut).into_iter().map(|key| kbd(key, &p))),
                    cx,
                )
            })))
    };

    div()
        .flex()
        .flex_col()
        .gap_6()
        .child(section(
            "settings.keybindings.global_title",
            get_global_keybindings(),
        ))
        .child(section(
            "settings.keybindings.terminal_title",
            get_terminal_keybindings(),
        ))
        .child(section(
            "settings.keybindings.sftp_editor_title",
            get_sftp_editor_keybindings(),
        ))
        .child(
            div()
                .px(px(2.))
                .text_size(px(12.))
                .text_color(p.text_muted)
                .child(i18n::t(&lang, "settings.keybindings.more_coming_soon")),
        )
}
