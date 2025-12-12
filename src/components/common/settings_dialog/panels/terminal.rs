// 终端设置面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::button::Button;
use gpui_component::menu::DropdownMenu;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::CursorStyle;

use super::super::helpers::{
    render_font_input_row, render_number_row, render_section_title, render_switch_row,
    render_theme_select_row, TERMINAL_FONTS, TERMINAL_THEMES,
};
use super::super::SettingsDialogState;

/// 渲染终端设置面板
pub fn render_terminal_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let terminal = &state_read.settings.terminal;
    let lang = &state_read.settings.theme.language;

    // 获取输入状态
    let font_family_input = state_read.terminal_font_family_input.clone();
    let font_size_input = state_read.terminal_font_size_input.clone();
    let line_height_input = state_read.terminal_line_height_input.clone();
    let scrollback_input = state_read.scrollback_lines_input.clone();

    // 光标样式选项
    let cursor_style = terminal.cursor_style.clone();
    let cursor_style_label = match cursor_style {
        CursorStyle::Block => i18n::t(lang, "settings.terminal.cursor_style.block"),
        CursorStyle::Bar => i18n::t(lang, "settings.terminal.cursor_style.bar"),
        CursorStyle::Underline => i18n::t(lang, "settings.terminal.cursor_style.underline"),
    };

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 字体
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.terminal.font"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(font_family_input.as_ref().map(|input| {
                            render_font_input_row(
                                cx,
                                i18n::t(lang, "settings.terminal.font_family"),
                                input,
                                TERMINAL_FONTS,
                            )
                        }))
                        .children(font_size_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.terminal.font_size"),
                                input,
                                cx,
                            )
                        }))
                        .children(line_height_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.terminal.line_height"),
                                input,
                                cx,
                            )
                        }))
                        .child(render_switch_row(
                            "terminal-ligatures",
                            i18n::t(lang, "settings.terminal.ligatures"),
                            terminal.ligatures,
                            state.clone(),
                            |s, v| s.settings.terminal.ligatures = v,
                            cx,
                        )),
                ),
        )
        // 配色
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.terminal.color_scheme"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .child(render_theme_select_row(
                            i18n::t(lang, "settings.terminal.theme"),
                            &terminal.color_scheme,
                            TERMINAL_THEMES,
                            state.clone(),
                            cx,
                        )),
                ),
        )
        // 显示
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.terminal.display"),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        // 光标样式选择器
                        .child(render_cursor_style_row(
                            i18n::t(lang, "settings.terminal.cursor_style"),
                            cursor_style_label,
                            state.clone(),
                            cx,
                        ))
                        .child(render_switch_row(
                            "terminal-cursor-blink",
                            i18n::t(lang, "settings.terminal.cursor_blink"),
                            terminal.cursor_blink,
                            state.clone(),
                            |s, v| s.settings.terminal.cursor_blink = v,
                            cx,
                        ))
                        .children(scrollback_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(lang, "settings.terminal.scrollback"),
                                input,
                                cx,
                            )
                        })),
                ),
        )
}

/// 渲染光标样式选择行
fn render_cursor_style_row(
    label: &'static str,
    current_label: &'static str,
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    use gpui::Corner;
    use gpui_component::menu::PopupMenuItem;

    let state_for_block = state.clone();
    let state_for_bar = state.clone();
    let state_for_underline = state.clone();

    div()
        .flex()
        .items_center()
        .justify_between()
        .py_3()
        .px_4()
        .bg(cx.theme().muted)
        .rounded_lg()
        .mb_2()
        .child(
            div()
                .w(px(120.))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(
            Button::new("cursor-style-dropdown")
                .w(px(200.))
                .h(px(32.))
                .outline()
                .justify_start()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .w(px(180.))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().foreground)
                                .child(current_label),
                        )
                        .child(render_icon(
                            icons::CHEVRON_DOWN,
                            cx.theme().muted_foreground.into(),
                        )),
                )
                .dropdown_menu_with_anchor(Corner::TopLeft, move |menu, _, _| {
                    menu.min_w(px(200.))
                        .item(PopupMenuItem::new("Block").on_click({
                            let state = state_for_block.clone();
                            move |_, _, cx| {
                                state.update(cx, |s, _| {
                                    s.settings.terminal.cursor_style = CursorStyle::Block;
                                    s.mark_changed();
                                });
                            }
                        }))
                        .item(PopupMenuItem::new("Bar").on_click({
                            let state = state_for_bar.clone();
                            move |_, _, cx| {
                                state.update(cx, |s, _| {
                                    s.settings.terminal.cursor_style = CursorStyle::Bar;
                                    s.mark_changed();
                                });
                            }
                        }))
                        .item(PopupMenuItem::new("Underline").on_click({
                            let state = state_for_underline.clone();
                            move |_, _, cx| {
                                state.update(cx, |s, _| {
                                    s.settings.terminal.cursor_style = CursorStyle::Underline;
                                    s.mark_changed();
                                });
                            }
                        }))
                }),
        )
}
