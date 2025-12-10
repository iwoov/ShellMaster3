// 主题设置面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::theme::{Theme as GpuiTheme, ThemeMode as GpuiThemeMode};
use gpui_component::ActiveTheme;

use crate::i18n;
use crate::models::settings::{Language, ThemeMode};

use super::super::helpers::{render_font_input_row, render_number_row, render_section_title, UI_FONTS};
use super::super::SettingsDialogState;

/// 渲染主题设置面板
pub fn render_theme_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let current_mode = state_read.settings.theme.mode.clone();
    let current_language = state_read.settings.theme.language.clone();

    // 获取输入状态
    let ui_font_family_input = state_read.ui_font_family_input.clone();
    let ui_font_size_input = state_read.ui_font_size_input.clone();

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 语言设置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(
                        &state.read(cx).settings.theme.language,
                        "settings.theme.language",
                    ),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(render_language_button(
                            state.clone(),
                            Language::Chinese,
                            current_language == Language::Chinese,
                            cx,
                        ))
                        .child(render_language_button(
                            state.clone(),
                            Language::English,
                            current_language == Language::English,
                            cx,
                        )),
                ),
        )
        // 外观模式
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(
                        &state.read(cx).settings.theme.language,
                        "settings.theme.mode",
                    ),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(render_theme_mode_button(
                            state.clone(),
                            ThemeMode::Light,
                            i18n::t(
                                &state.read(cx).settings.theme.language,
                                "settings.theme.mode.light",
                            ),
                            current_mode == ThemeMode::Light,
                            cx,
                        ))
                        .child(render_theme_mode_button(
                            state.clone(),
                            ThemeMode::Dark,
                            i18n::t(
                                &state.read(cx).settings.theme.language,
                                "settings.theme.mode.dark",
                            ),
                            current_mode == ThemeMode::Dark,
                            cx,
                        ))
                        .child(render_theme_mode_button(
                            state.clone(),
                            ThemeMode::System,
                            i18n::t(
                                &state.read(cx).settings.theme.language,
                                "settings.theme.mode.system",
                            ),
                            current_mode == ThemeMode::System,
                            cx,
                        )),
                ),
        )
        // 字体设置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(
                        &state.read(cx).settings.theme.language,
                        "settings.theme.font",
                    ),
                    cx,
                ))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_3()
                        .children(ui_font_family_input.as_ref().map(|input| {
                            render_font_input_row(
                                cx,
                                i18n::t(
                                    &state.read(cx).settings.theme.language,
                                    "settings.theme.font_family",
                                ),
                                input,
                                UI_FONTS,
                            )
                        }))
                        .children(ui_font_size_input.as_ref().map(|input| {
                            render_number_row(
                                i18n::t(
                                    &state.read(cx).settings.theme.language,
                                    "settings.theme.font_size",
                                ),
                                input,
                                cx,
                            )
                        })),
                ),
        )
}

/// 渲染语言按钮
fn render_language_button(
    state: Entity<SettingsDialogState>,
    lang: Language,
    selected: bool,
    cx: &App,
) -> impl IntoElement {
    let bg_color = if selected {
        cx.theme().primary
    } else {
        cx.theme().secondary
    };
    let text_color = if selected {
        cx.theme().primary_foreground
    } else {
        cx.theme().secondary_foreground
    };
    let label = lang.label();

    let hover_selected_bg = cx.theme().primary_hover;
    let hover_unselected_bg = cx.theme().secondary_hover;

    div()
        .id(SharedString::from(format!("lang-{:?}", lang)))
        .px_4()
        .py_2()
        .rounded_md()
        .bg(bg_color)
        .cursor_pointer()
        .hover(move |s| {
            if selected {
                s.bg(hover_selected_bg)
            } else {
                s.bg(hover_unselected_bg)
            }
        })
        .on_click(move |_, _, cx| {
            state.update(cx, |s, _| {
                s.settings.theme.language = lang.clone();
                s.mark_changed();
            });
        })
        .child(div().text_sm().text_color(text_color).child(label))
}

/// 渲染主题模式按钮
fn render_theme_mode_button(
    state: Entity<SettingsDialogState>,
    mode: ThemeMode,
    label: &'static str,
    selected: bool,
    cx: &App,
) -> impl IntoElement {
    let bg_color = if selected {
        cx.theme().primary
    } else {
        cx.theme().secondary
    };
    let text_color = if selected {
        cx.theme().primary_foreground
    } else {
        cx.theme().secondary_foreground
    };

    let hover_selected_bg = cx.theme().primary_hover;
    let hover_unselected_bg = cx.theme().secondary_hover;

    div()
        .id(SharedString::from(format!("theme-mode-{:?}", mode)))
        .px_4()
        .py_2()
        .rounded_md()
        .bg(bg_color)
        .cursor_pointer()
        .hover(move |s| {
            if selected {
                s.bg(hover_selected_bg)
            } else {
                s.bg(hover_unselected_bg)
            }
        })
        .on_click(move |_, window, cx| {
            // Save to settings
            state.update(cx, |s, _| {
                s.settings.theme.mode = mode.clone();
                s.mark_changed();
            });

            // Apply theme immediately
            match mode {
                ThemeMode::Light => GpuiTheme::change(GpuiThemeMode::Light, Some(window), cx),
                ThemeMode::Dark => GpuiTheme::change(GpuiThemeMode::Dark, Some(window), cx),
                ThemeMode::System => GpuiTheme::sync_system_appearance(Some(window), cx),
            }
        })
        .child(div().text_sm().text_color(text_color).child(label))
}
