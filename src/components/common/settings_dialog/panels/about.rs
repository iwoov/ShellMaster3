// 关于面板

use gpui::prelude::*;
use gpui::*;
use gpui_component::ActiveTheme;

use crate::i18n;

use super::super::helpers::render_about_row;
use super::super::SettingsDialogState;

/// 渲染关于面板
pub fn render_about_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let lang = &state.read(cx).settings.theme.language;
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_6()
        .pt_8()
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
                        .child("v1.0.0"),
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
        // 版权
        .child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(i18n::t(lang, "settings.about.copyright")),
        )
}
