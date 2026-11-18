// 外观设置面板：界面语言、主题模式、界面字体

use gpui::prelude::*;
use gpui::*;

use crate::components::common::input::segmented;
use crate::i18n;
use crate::models::settings::{Language, ThemeMode};

use super::super::helpers::{
    render_card, render_font_input_row, render_number_row, render_row, render_section_title,
    UI_FONTS,
};
use super::super::{apply_theme_mode, SettingsDialogState};

/// 渲染外观设置面板
pub fn render_theme_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let state_read = state.read(cx);
    let lang = state_read.settings.theme.language.clone();
    let current_mode = state_read.settings.theme.mode.clone();

    // 获取输入状态
    let ui_font_family_input = state_read.ui_font_family_input.clone();
    let ui_font_size_input = state_read.ui_font_size_input.clone();

    // 语言：弹窗、主页立即切换，保存后其余界面跟着切换
    let state_for_language = state.clone();
    let language_control = segmented(
        "settings-language",
        vec![
            (Language::Chinese, Language::Chinese.label().into()),
            (Language::English, Language::English.label().into()),
        ],
        &lang,
        false,
        move |language, _, cx| {
            state_for_language.update(cx, |s, cx| {
                s.settings.theme.language = language;
                s.mark_changed();
                cx.notify();
            });
        },
        &p,
    );

    // 主题模式：立即生效，取消时撤销
    let state_for_mode = state.clone();
    let mode_control = segmented(
        "settings-theme-mode",
        vec![
            (
                ThemeMode::Light,
                i18n::t(&lang, "settings.theme.mode.light").into(),
            ),
            (
                ThemeMode::Dark,
                i18n::t(&lang, "settings.theme.mode.dark").into(),
            ),
            (
                ThemeMode::System,
                i18n::t(&lang, "settings.theme.mode.system").into(),
            ),
        ],
        &current_mode,
        false,
        move |mode, window, cx| {
            state_for_mode.update(cx, |s, cx| {
                s.settings.theme.mode = mode.clone();
                s.mark_changed();
                cx.notify();
            });
            apply_theme_mode(&mode, window, cx);
        },
        &p,
    );

    div()
        .flex()
        .flex_col()
        .gap_6()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(&lang, "settings.theme.interface"),
                    cx,
                ))
                .child(
                    render_card(cx)
                        .child(render_row(
                            i18n::t(&lang, "settings.theme.language"),
                            None,
                            language_control,
                            cx,
                        ))
                        .child(render_row(
                            i18n::t(&lang, "settings.theme.mode"),
                            None,
                            mode_control,
                            cx,
                        )),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(&lang, "settings.theme.font"),
                    cx,
                ))
                .child(
                    render_card(cx)
                        .children(ui_font_family_input.as_ref().map(|input| {
                            render_font_input_row(
                                cx,
                                i18n::t(&lang, "settings.theme.font_family"),
                                input,
                                UI_FONTS,
                            )
                        }))
                        .children(ui_font_size_input.as_ref().map(|input| {
                            render_number_row(i18n::t(&lang, "settings.theme.font_size"), input, cx)
                        })),
                ),
        )
}
