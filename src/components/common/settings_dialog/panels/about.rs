// 关于面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::i18n;

use super::super::helpers::render_about_row;
use super::super::SettingsDialogState;

fn render_release_item(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .flex()
        .items_start()
        .gap_2()
        .child(
            div()
                .mt(px(7.))
                .w(px(5.))
                .h(px(5.))
                .flex_shrink_0()
                .rounded_full()
                .bg(cx.theme().primary),
        )
        .child(
            div()
                .text_sm()
                .line_height(relative(1.45))
                .text_color(cx.theme().foreground)
                .child(text),
        )
}

/// 渲染关于面板
pub fn render_about_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let lang = &state.read(cx).settings.theme.language;
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_5()
        .pt_4()
        // Logo / 应用名
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(64.))
                        .h(px(64.))
                        .rounded_xl()
                        .bg(cx.theme().primary)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::BOLD)
                                .text_color(cx.theme().primary_foreground)
                                .child("SM"),
                        ),
                )
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().foreground)
                        .child("ShellMaster"),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(concat!("v", env!("CARGO_PKG_VERSION"))),
                ),
        )
        // 技术信息
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_about_row(
                    i18n::t(lang, "settings.about.platform"),
                    "macOS",
                    cx,
                ))
                .child(render_about_row(
                    i18n::t(lang, "settings.about.arch"),
                    std::env::consts::ARCH,
                    cx,
                ))
                .child(render_about_row("Rust", env!("CARGO_PKG_RUST_VERSION"), cx)),
        )
        // 当前版本更新说明
        .child(
            div()
                .w_full()
                .max_w(px(460.))
                .p_4()
                .rounded_lg()
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().secondary)
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .child(i18n::t(lang, "settings.about.whats_new")),
                        )
                        .child(
                            div()
                                .px_2()
                                .py_1()
                                .rounded_md()
                                .bg(cx.theme().primary.opacity(0.12))
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().primary)
                                .child(concat!("v", env!("CARGO_PKG_VERSION"))),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_release_item(
                            i18n::t(lang, "settings.about.release_0_1_3.ime"),
                            cx,
                        ))
                        .child(render_release_item(
                            i18n::t(lang, "settings.about.release_0_1_3.backtab"),
                            cx,
                        ))
                        .child(render_release_item(
                            i18n::t(lang, "settings.about.release_0_1_3.input_routing"),
                            cx,
                        )),
                ),
        )
        // 版权
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(i18n::t(lang, "settings.about.copyright")),
        )
}
