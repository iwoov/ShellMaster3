// 终端设置面板

use gpui::prelude::*;
use gpui::*;

use crate::i18n;

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
