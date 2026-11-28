// 关于面板

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::i18n;

use super::super::helpers::{
    render_about_row, render_block_row, render_card, render_section_title,
};
use super::super::SettingsDialogState;
use super::system::render_update_status_block;

/// 当前版本的更新说明
const RELEASE_NOTES: [&str; 3] = [
    "settings.about.release_0_1_8.browser",
    "settings.about.release_0_1_8.safety",
    "settings.about.release_0_1_8.performance",
];

/// 运行平台的显示名称
fn platform_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    }
}

/// 渲染关于面板
pub fn render_about_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let state_read = state.read(cx);
    let lang = state_read.settings.theme.language.clone();
    let update_state = state_read.update_state.clone();
    let auto_update = state_read.settings.system.auto_update;
    let version = concat!("v", env!("CARGO_PKG_VERSION"));

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 应用图标、名称、版本与快捷检查更新
        .child(
            render_card(cx)
                .child(
                    div()
                        .px(px(16.))
                        .py(px(16.))
                        .flex()
                        .items_center()
                        .gap(px(14.))
                        .child(
                            img("app-icon.png")
                                .flex_none()
                                .size(px(52.))
                                .rounded(px(12.)),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(
                                    div()
                                        .text_size(px(18.))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(p.text)
                                        .child("ShellMaster"),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .text_color(p.text_muted)
                                        .child(version),
                                ),
                        ),
                )
                .child(render_update_status_block(
                    update_state,
                    auto_update,
                    &lang,
                    "about-update",
                    cx,
                )),
        )
        // 运行环境
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(&lang, "settings.about.environment"),
                    cx,
                ))
                .child(
                    render_card(cx)
                        .child(render_about_row(
                            i18n::t(&lang, "settings.about.platform"),
                            platform_name(),
                            cx,
                        ))
                        .child(render_about_row(
                            i18n::t(&lang, "settings.about.arch"),
                            std::env::consts::ARCH,
                            cx,
                        )),
                ),
        )
        // 当前版本更新说明
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    format!(
                        "{} · {}",
                        i18n::t(&lang, "settings.about.whats_new"),
                        version
                    ),
                    cx,
                ))
                .child(render_card(cx).children(RELEASE_NOTES.iter().map(|key| {
                    render_block_row(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(10.))
                            .child(
                                div()
                                    .mt(px(7.))
                                    .size(px(5.))
                                    .flex_none()
                                    .rounded_full()
                                    .bg(p.primary),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(13.))
                                    .line_height(relative(1.5))
                                    .text_color(p.text)
                                    .child(i18n::t(&lang, key)),
                            ),
                        cx,
                    )
                }))),
        )
        // 版权
        .child(
            div()
                .px(px(2.))
                .text_size(px(12.))
                .text_color(p.text_faint)
                .child(i18n::t(&lang, "settings.about.copyright")),
        )
}
