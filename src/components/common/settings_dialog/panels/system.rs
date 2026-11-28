// 系统配置面板

use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::components::common::button::{button, primary_button, secondary_button, ButtonKind};
use crate::components::common::icon::icon_sized;
use crate::components::common::panel::spinner;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::services::updater::{self, UpdateStatus, CURRENT_VERSION};
use crate::state::UpdateState;

use super::super::helpers::{
    render_block_row, render_card, render_hint, render_section_title, render_switch_row,
};
use super::super::SettingsDialogState;

/// 截取发布说明前几行作为精简预览
fn release_notes_preview(body: &str) -> String {
    let lines: Vec<&str> = body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(4)
        .collect();
    lines.join("\n")
}

/// 渲染软件更新卡片内的状态栏与操作按钮
pub(super) fn render_update_status_block(
    update_state: Option<Entity<UpdateState>>,
    auto_update: bool,
    lang: &Language,
    id_prefix: &'static str,
    cx: &App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    let status = update_state
        .as_ref()
        .map(|s| s.read(cx).status.clone())
        .unwrap_or_default();

    let version_badge = format!(
        "{} v{}",
        i18n::t(lang, "settings.update.current_version"),
        CURRENT_VERSION
    );

    // 左侧状态说明
    let status_line: AnyElement = match &status {
        UpdateStatus::Idle => div()
            .text_size(px(12.))
            .text_color(p.text_muted)
            .child(format!(
                "GitHub: {}/{}",
                updater::GITHUB_OWNER,
                updater::GITHUB_REPO
            ))
            .into_any_element(),
        UpdateStatus::Checking => div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(spinner(
                SharedString::from(format!("{}-check-spinner", id_prefix)),
                12.,
                p.primary,
            ))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.text_muted)
                    .child(i18n::t(lang, "settings.update.checking")),
            )
            .into_any_element(),
        UpdateStatus::UpToDate { checked_at } => div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(icon_sized(icons::CHECK, 13., p.success))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.success)
                    .child(format!(
                        "{} · {}",
                        i18n::t(lang, "settings.update.up_to_date"),
                        checked_at
                    )),
            )
            .into_any_element(),
        UpdateStatus::Available(release) => {
            let size_text = release
                .asset
                .as_ref()
                .filter(|a| a.size > 0)
                .map(|a| format!(" ({})", updater::format_bytes(a.size)))
                .unwrap_or_default();
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(icon_sized(icons::ARROW_UP, 13., p.primary))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(p.primary)
                        .child(format!(
                            "{}: {}{}",
                            i18n::t(lang, "settings.update.available"),
                            release.tag_name,
                            size_text
                        )),
                )
                .into_any_element()
        }
        UpdateStatus::Downloading {
            release,
            downloaded,
            total,
        } => {
            let progress_text = match total {
                Some(t) if *t > 0 => {
                    let pct = ((*downloaded as f64 / *t as f64) * 100.0).clamp(0.0, 100.0);
                    format!(
                        "{} {} · {} / {} ({:.0}%)",
                        i18n::t(lang, "settings.update.downloading"),
                        release.tag_name,
                        updater::format_bytes(*downloaded),
                        updater::format_bytes(*t),
                        pct
                    )
                }
                _ => format!(
                    "{} {} · {}",
                    i18n::t(lang, "settings.update.downloading"),
                    release.tag_name,
                    updater::format_bytes(*downloaded)
                ),
            };
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(spinner(
                    SharedString::from(format!("{}-dl-spinner", id_prefix)),
                    12.,
                    p.primary,
                ))
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.primary)
                        .child(progress_text),
                )
                .into_any_element()
        }
        UpdateStatus::Downloaded { release, .. } => div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(icon_sized(icons::CHECK, 13., p.success))
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(p.success)
                    .child(format!(
                        "{} ({})",
                        i18n::t(lang, "settings.update.ready_to_install"),
                        release.tag_name
                    )),
            )
            .into_any_element(),
        UpdateStatus::Installing { release } => div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(spinner(
                SharedString::from(format!("{}-inst-spinner", id_prefix)),
                12.,
                p.primary,
            ))
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.primary)
                    .child(format!(
                        "{} ({})",
                        i18n::t(lang, "settings.update.installing"),
                        release.tag_name
                    )),
            )
            .into_any_element(),
        UpdateStatus::InstalledOpened { message } => div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(icon_sized(icons::CHECK, 13., p.success))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(12.))
                    .text_color(p.success)
                    .child(message.clone()),
            )
            .into_any_element(),
        UpdateStatus::Error(err) => div()
            .text_size(px(12.))
            .text_color(p.danger)
            .child(err.clone())
            .into_any_element(),
    };

    // 右侧主操作按钮区
    let mut actions = div().flex().flex_none().items_center().gap(px(8.));

    match &status {
        UpdateStatus::Idle | UpdateStatus::UpToDate { .. } | UpdateStatus::InstalledOpened { .. } => {
            if let Some(u) = update_state.clone() {
                actions = actions.child(
                    secondary_button(
                        SharedString::from(format!("{}-check-btn", id_prefix)),
                        icons::REFRESH,
                        i18n::t(lang, "settings.update.check_now"),
                        &p,
                    )
                    .on_click(move |_, window, cx| {
                        UpdateState::check_for_updates(
                            u.clone(),
                            auto_update,
                            false,
                            Some(window.window_handle()),
                            cx,
                        );
                    }),
                );
            }
        }
        UpdateStatus::Checking => {
            actions = actions.child(button(
                SharedString::from(format!("{}-checking-btn", id_prefix)),
                ButtonKind::Secondary,
                Some(icons::REFRESH),
                i18n::t(lang, "settings.update.checking"),
                false,
                &p,
            ));
        }
        UpdateStatus::Available(release) => {
            if !release.html_url.is_empty() {
                let url = release.html_url.clone();
                actions = actions.child(
                    secondary_button(
                        SharedString::from(format!("{}-release-btn", id_prefix)),
                        icons::GLOBE,
                        i18n::t(lang, "settings.update.view_release"),
                        &p,
                    )
                    .on_click(move |_, _, _| {
                        let _ = updater::open_external_url(&url);
                    }),
                );
            }
            if release.asset.is_some() {
                if let Some(u) = update_state.clone() {
                    let release_clone = release.clone();
                    actions = actions.child(
                        primary_button(
                            SharedString::from(format!("{}-download-btn", id_prefix)),
                            icons::DOWNLOAD,
                            i18n::t(lang, "settings.update.download_and_install"),
                            &p,
                        )
                        .on_click(move |_, window, cx| {
                            UpdateState::start_download(
                                u.clone(),
                                release_clone.clone(),
                                true,
                                false,
                                Some(window.window_handle()),
                                cx,
                            );
                        }),
                    );
                }
            }
        }
        UpdateStatus::Downloading {
            downloaded, total, ..
        } => {
            let pct_label = match total {
                Some(t) if *t > 0 => {
                    let pct = ((*downloaded as f64 / *t as f64) * 100.0).clamp(0.0, 100.0);
                    format!("{:.0}%", pct)
                }
                _ => updater::format_bytes(*downloaded),
            };
            actions = actions.child(button(
                SharedString::from(format!("{}-downloading-btn", id_prefix)),
                ButtonKind::Secondary,
                Some(icons::DOWNLOAD),
                pct_label,
                false,
                &p,
            ));
        }
        UpdateStatus::Downloaded { release, file_path } => {
            if !release.html_url.is_empty() {
                let url = release.html_url.clone();
                actions = actions.child(
                    secondary_button(
                        SharedString::from(format!("{}-ready-release-btn", id_prefix)),
                        icons::GLOBE,
                        i18n::t(lang, "settings.update.view_release"),
                        &p,
                    )
                    .on_click(move |_, _, _| {
                        let _ = updater::open_external_url(&url);
                    }),
                );
            }
            if let Some(u) = update_state.clone() {
                let release_clone = release.clone();
                let path_clone = file_path.clone();
                actions = actions.child(
                    primary_button(
                        SharedString::from(format!("{}-install-btn", id_prefix)),
                        icons::ARROW_UP,
                        i18n::t(lang, "settings.update.install_and_restart"),
                        &p,
                    )
                    .on_click(move |_, _, cx| {
                        UpdateState::install_downloaded(
                            u.clone(),
                            release_clone.clone(),
                            path_clone.clone(),
                            cx,
                        );
                    }),
                );
            }
        }
        UpdateStatus::Installing { .. } => {
            actions = actions.child(button(
                SharedString::from(format!("{}-installing-btn", id_prefix)),
                ButtonKind::Primary,
                Some(icons::ARROW_UP),
                i18n::t(lang, "settings.update.installing"),
                false,
                &p,
            ));
        }
        UpdateStatus::Error(_) => {
            if let Some(u) = update_state.clone() {
                actions = actions.child(
                    secondary_button(
                        SharedString::from(format!("{}-retry-btn", id_prefix)),
                        icons::REFRESH,
                        i18n::t(lang, "settings.update.retry"),
                        &p,
                    )
                    .on_click(move |_, window, cx| {
                        UpdateState::check_for_updates(
                            u.clone(),
                            auto_update,
                            false,
                            Some(window.window_handle()),
                            cx,
                        );
                    }),
                );
            }
        }
    }

    // 下载进度条（仅 Downloading 时展示）
    let progress_bar = match &status {
        UpdateStatus::Downloading {
            downloaded, total, ..
        } => {
            let ratio = match total {
                Some(t) if *t > 0 => (*downloaded as f32 / *t as f32).clamp(0.02, 1.0),
                _ => 0.15,
            };
            Some(
                div()
                    .w_full()
                    .h(px(6.))
                    .rounded_full()
                    .bg(p.hover_strong)
                    .overflow_hidden()
                    .child(div().h_full().w(relative(ratio)).rounded_full().bg(p.primary)),
            )
        }
        _ => None,
    };

    // 新版本发布说明预览（仅 Available / Downloaded 且有内容或缺少平台包时展示）
    let extra_notice = match &status {
        UpdateStatus::Available(release) if release.asset.is_none() => Some(
            div()
                .text_size(px(12.))
                .text_color(p.warning)
                .child(i18n::t(lang, "settings.update.no_asset"))
                .into_any_element(),
        ),
        UpdateStatus::Available(release) | UpdateStatus::Downloaded { release, .. } => {
            let preview = release_notes_preview(&release.body);
            if preview.is_empty() {
                None
            } else {
                Some(
                    div()
                        .w_full()
                        .px(px(10.))
                        .py(px(8.))
                        .rounded(px(6.))
                        .bg(p.surface)
                        .border_1()
                        .border_color(p.line)
                        .text_size(px(12.))
                        .line_height(relative(1.45))
                        .text_color(p.text_muted)
                        .child(preview)
                        .into_any_element(),
                )
            }
        }
        _ => None,
    };

    render_block_row(
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(
                                div()
                                    .text_size(px(13.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(p.text)
                                    .child(version_badge),
                            )
                            .child(status_line),
                    )
                    .child(actions),
            )
            .children(progress_bar)
            .children(extra_notice),
        cx,
    )
}

/// 渲染系统配置面板
pub fn render_system_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let system = &state_read.settings.system;
    let lang = &state_read.settings.theme.language;
    let update_state = state_read.update_state.clone();
    let auto_update = system.auto_update;

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 软件更新
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.update.section"),
                    cx,
                ))
                .child(
                    render_card(cx)
                        .child(render_update_status_block(
                            update_state,
                            auto_update,
                            lang,
                            "sys-update",
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-check-updates",
                            i18n::t(lang, "settings.update.check_on_startup"),
                            system.check_updates,
                            state.clone(),
                            |s, v| s.settings.system.check_updates = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sys-auto-update",
                            i18n::t(lang, "settings.update.auto_update"),
                            system.auto_update,
                            state.clone(),
                            |s, v| s.settings.system.auto_update = v,
                            cx,
                        ))
                        .child(render_hint(
                            i18n::t(lang, "settings.update.auto_update_hint"),
                            cx,
                        )),
                ),
        )
        // 窗口
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(lang, "settings.system.window"),
                    cx,
                ))
                .child(render_card(cx).child(render_switch_row(
                    "sys-close-tray",
                    i18n::t(lang, "settings.system.close_to_tray"),
                    system.close_to_tray,
                    state.clone(),
                    |s, v| s.settings.system.close_to_tray = v,
                    cx,
                ))),
        )
}
