// 数据同步面板（WebDAV 连通性测试 + AES-256-GCM 端到端加密 3-Way 多端同步）

use crate::components::common::button::{button, ButtonKind};
use crate::components::common::input::{segmented, text_field};
use crate::constants::icons;
use crate::models::settings::ConflictStrategy;
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::i18n;

use super::super::helpers::{
    render_card, render_hint, render_input_row, render_row, render_section_title,
    render_switch_row, CONTROL_WIDTH,
};
use super::super::SettingsDialogState;

/// 渲染数据同步面板
pub fn render_sync_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let sync = &state_read.settings.sync;
    let lang = state_read.settings.theme.language.clone();
    let p = crate::theme::palette(cx);

    let webdav_url_input = state_read.webdav_url_input.clone();
    let webdav_username_input = state_read.webdav_username_input.clone();
    let webdav_password_input = state_read.webdav_password_input.clone();
    let webdav_path_input = state_read.webdav_path_input.clone();
    let webdav_passphrase_input = state_read.webdav_passphrase_input.clone();
    let sync_testing = state_read.sync_testing;
    let sync_running = state_read.sync_running;
    let sync_notice = state_read.sync_notice.clone();
    let last_sync_text = sync
        .last_sync_at
        .as_deref()
        .map(format_sync_time)
        .unwrap_or_else(|| i18n::t(&lang, "settings.sync.never_synced").to_string());

    let current_strategy = match sync.conflict_strategy {
        ConflictStrategy::LocalFirst => ConflictStrategy::LocalFirst,
        ConflictStrategy::RemoteFirst => ConflictStrategy::RemoteFirst,
        ConflictStrategy::Merge | ConflictStrategy::Ask => ConflictStrategy::Merge,
    };

    let busy = sync_testing || sync_running;

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 同步状态与冲突策略
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(&lang, "settings.sync.status"),
                    cx,
                ))
                .child(
                    render_card(cx)
                        .child(render_switch_row(
                            "sync-enabled",
                            i18n::t(&lang, "settings.sync.enabled"),
                            sync.enabled,
                            state.clone(),
                            |s, v| s.settings.sync.enabled = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-auto",
                            i18n::t(&lang, "settings.sync.auto"),
                            sync.auto_sync,
                            state.clone(),
                            |s, v| s.settings.sync.auto_sync = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-backup",
                            i18n::t(&lang, "settings.sync.backup_before"),
                            sync.backup_before_sync,
                            state.clone(),
                            |s, v| s.settings.sync.backup_before_sync = v,
                            cx,
                        ))
                        .child({
                            let state_for_strategy = state.clone();
                            render_row(
                                i18n::t(&lang, "settings.sync.conflict_strategy"),
                                None,
                                segmented(
                                    "sync-conflict-strategy",
                                    vec![
                                        (
                                            ConflictStrategy::Merge,
                                            i18n::t(&lang, "settings.sync.conflict_merge").into(),
                                        ),
                                        (
                                            ConflictStrategy::LocalFirst,
                                            i18n::t(&lang, "settings.sync.conflict_local").into(),
                                        ),
                                        (
                                            ConflictStrategy::RemoteFirst,
                                            i18n::t(&lang, "settings.sync.conflict_remote").into(),
                                        ),
                                    ],
                                    &current_strategy,
                                    false,
                                    move |strategy, _, cx| {
                                        state_for_strategy.update(cx, |s, cx| {
                                            s.settings.sync.conflict_strategy = strategy;
                                            s.mark_changed();
                                            cx.notify();
                                        });
                                    },
                                    &p,
                                ),
                                cx,
                            )
                        })
                        .child(render_row(
                            i18n::t(&lang, "settings.sync.last_sync"),
                            None,
                            div()
                                .text_size(px(12.))
                                .text_color(p.text_muted)
                                .child(last_sync_text),
                            cx,
                        )),
                ),
        )
        // 同步内容
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(&lang, "settings.sync.content"),
                    cx,
                ))
                .child(
                    render_card(cx)
                        .child(render_switch_row(
                            "sync-servers",
                            i18n::t(&lang, "settings.sync.servers"),
                            sync.sync_servers,
                            state.clone(),
                            |s, v| s.settings.sync.sync_servers = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-groups",
                            i18n::t(&lang, "settings.sync.groups"),
                            sync.sync_groups,
                            state.clone(),
                            |s, v| s.settings.sync.sync_groups = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-keys",
                            i18n::t(&lang, "settings.sync.keys"),
                            sync.sync_keys,
                            state.clone(),
                            |s, v| s.settings.sync.sync_keys = v,
                            cx,
                        ))
                        .child(render_hint(i18n::t(&lang, "settings.sync.keys_hint"), cx))
                        .child(render_switch_row(
                            "sync-snippets",
                            i18n::t(&lang, "settings.sync.snippets"),
                            sync.sync_snippets,
                            state.clone(),
                            |s, v| s.settings.sync.sync_snippets = v,
                            cx,
                        ))
                        .child(render_switch_row(
                            "sync-settings",
                            i18n::t(&lang, "settings.sync.settings"),
                            sync.sync_settings,
                            state.clone(),
                            |s, v| s.settings.sync.sync_settings = v,
                            cx,
                        )),
                ),
        )
        // WebDAV 配置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title(
                    i18n::t(&lang, "settings.sync.webdav"),
                    cx,
                ))
                .child(
                    render_card(cx)
                        .children(webdav_url_input.as_ref().map(|input| {
                            render_input_row(i18n::t(&lang, "settings.sync.webdav_url"), input, cx)
                        }))
                        .children(webdav_username_input.as_ref().map(|input| {
                            render_input_row(
                                i18n::t(&lang, "settings.sync.webdav_username"),
                                input,
                                cx,
                            )
                        }))
                        .children(webdav_password_input.as_ref().map(|input| {
                            render_row(
                                i18n::t(&lang, "settings.sync.webdav_password"),
                                None,
                                div().w(CONTROL_WIDTH).child(text_field(input).mask_toggle()),
                                cx,
                            )
                        }))
                        .children(webdav_path_input.as_ref().map(|input| {
                            render_input_row(i18n::t(&lang, "settings.sync.webdav_path"), input, cx)
                        }))
                        .children(webdav_passphrase_input.as_ref().map(|input| {
                            render_row(
                                i18n::t(&lang, "settings.sync.passphrase"),
                                None,
                                div().w(CONTROL_WIDTH).child(text_field(input).mask_toggle()),
                                cx,
                            )
                        }))
                        .child(render_hint(
                            i18n::t(&lang, "settings.sync.passphrase_hint"),
                            cx,
                        )),
                )
                .children(sync_notice.map(|(ok, msg)| {
                    let tone = if ok { p.success } else { p.danger };
                    div()
                        .px(px(12.))
                        .py(px(9.))
                        .rounded(px(8.))
                        .bg(tone.opacity(0.1))
                        .border_1()
                        .border_color(tone.opacity(0.32))
                        .text_size(px(12.))
                        .text_color(tone)
                        .child(msg)
                }))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child({
                            let btn = button(
                                "test-webdav",
                                ButtonKind::Secondary,
                                Some(icons::PLAY),
                                if sync_testing {
                                    i18n::t(&lang, "settings.sync.testing")
                                } else {
                                    i18n::t(&lang, "settings.sync.test_connection")
                                },
                                !busy,
                                &p,
                            );
                            if busy {
                                btn
                            } else {
                                let state = state.clone();
                                btn.on_click(move |_, _, cx| run_test_webdav(state.clone(), cx))
                            }
                        })
                        .child({
                            let btn = button(
                                "sync-now",
                                ButtonKind::Primary,
                                Some(icons::REFRESH),
                                if sync_running {
                                    i18n::t(&lang, "settings.sync.syncing")
                                } else {
                                    i18n::t(&lang, "settings.sync.sync_now")
                                },
                                !busy,
                                &p,
                            );
                            if busy {
                                btn
                            } else {
                                let state = state.clone();
                                btn.on_click(move |_, _, cx| run_sync_now(state.clone(), cx))
                            }
                        }),
                ),
        )
}

fn format_sync_time(rfc3339: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| rfc3339.to_string())
}

fn run_test_webdav(state: Entity<SettingsDialogState>, cx: &mut App) {
    let cfg = {
        let app: &App = cx;
        state.read(app).collect_sync_settings(app)
    };

    state.update(cx, |s, cx| {
        s.sync_testing = true;
        s.sync_notice = None;
        cx.notify();
    });

    let (tx, rx) = tokio::sync::oneshot::channel();
    crate::ssh::manager::SshManager::global()
        .runtime()
        .spawn(async move {
            let res = crate::services::sync::test_webdav_connection(&cfg).await;
            let _ = tx.send(res);
        });

    cx.spawn({
        let state = state.clone();
        async move |async_cx| {
            let res = rx.await;
            let _ = async_cx.update(|cx| {
                state.update(cx, |s, cx| {
                    s.sync_testing = false;
                    s.sync_notice = Some(match res {
                        Ok(Ok(msg)) => (true, msg),
                        Ok(Err(err)) => (false, err),
                        Err(_) => (false, "internal channel error".to_string()),
                    });
                    cx.notify();
                });
            });
            Some(())
        }
    })
    .detach();
}

fn run_sync_now(state: Entity<SettingsDialogState>, cx: &mut App) {
    state.update(cx, |s, cx| {
        s.flush_before_manual_sync(cx);
    });
    let cfg = {
        let app: &App = cx;
        state.read(app).collect_sync_settings(app)
    };

    state.update(cx, |s, cx| {
        s.sync_running = true;
        s.sync_notice = None;
        cx.notify();
    });

    let (tx, rx) = tokio::sync::oneshot::channel();
    crate::ssh::manager::SshManager::global()
        .runtime()
        .spawn(async move {
            let res = crate::services::sync::sync_webdav(&cfg).await;
            let _ = tx.send(res);
        });

    cx.spawn({
        let state = state.clone();
        async move |async_cx| {
            let res = rx.await;
            let _ = async_cx.update(|cx| {
                state.update(cx, |s, cx| {
                    s.sync_running = false;
                    let lang = s.settings.theme.language.clone();
                    match res {
                        Ok(Ok(report)) => {
                            s.reload_after_sync();
                            s.settings.sync.last_sync_at = Some(report.synced_at.clone());
                            s.sync_data_updated = true;
                            let mode_label = match (&lang, report.encrypted) {
                                (crate::models::settings::Language::Chinese, true) => "端到端已加密",
                                (crate::models::settings::Language::Chinese, false) => "无口令·已剥离密码",
                                (crate::models::settings::Language::English, true) => "E2E encrypted",
                                (crate::models::settings::Language::English, false) => {
                                    "no passphrase, secrets stripped"
                                }
                            };
                            let msg = match lang {
                                crate::models::settings::Language::Chinese => {
                                    if report.keys_synced > 0 {
                                        format!(
                                            "同步成功：{} 台主机、{} 个分组、{} 条片段、{} 个私钥（{}）",
                                            report.servers_count,
                                            report.groups_count,
                                            report.snippets_count,
                                            report.keys_synced,
                                            mode_label
                                        )
                                    } else {
                                        format!(
                                            "同步成功：{} 台主机、{} 个分组、{} 条片段（{}）",
                                            report.servers_count,
                                            report.groups_count,
                                            report.snippets_count,
                                            mode_label
                                        )
                                    }
                                }
                                crate::models::settings::Language::English => {
                                    if report.keys_synced > 0 {
                                        format!(
                                            "Synced {} hosts, {} groups, {} snippets, {} keys ({})",
                                            report.servers_count,
                                            report.groups_count,
                                            report.snippets_count,
                                            report.keys_synced,
                                            mode_label
                                        )
                                    } else {
                                        format!(
                                            "Synced {} hosts, {} groups, {} snippets ({})",
                                            report.servers_count,
                                            report.groups_count,
                                            report.snippets_count,
                                            mode_label
                                        )
                                    }
                                }
                            };
                            s.sync_notice = Some((true, msg));
                        }
                        Ok(Err(err)) => {
                            s.sync_notice = Some((false, err));
                        }
                        Err(_) => {
                            s.sync_notice = Some((false, "internal channel error".to_string()));
                        }
                    }
                    cx.notify();
                });
            });
            Some(())
        }
    })
    .detach();
}
