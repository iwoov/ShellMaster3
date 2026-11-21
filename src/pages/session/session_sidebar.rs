// Session 右侧边栏：快捷命令 / 传输 / AI 对话三个面板，各带 36px 标题条

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_component::scroll::ScrollableElement;
use gpui_component::ActiveTheme;
use std::sync::Arc;
use tracing::debug;

use crate::components::common::button::{icon_button_sized, text_tooltip};
use crate::components::common::format::{format_bytes, format_speed};
use crate::components::common::icon::{icon_sized, render_icon, render_letter_avatar};
use crate::components::common::panel::{
    meter_bar, panel_header, panel_state, panel_title, StateIcon, LIST_ROW_HEIGHT, PANEL_PADDING,
};
use crate::components::common::text::TextFitter;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::models::sftp::{TransferItem, TransferStatus};
use crate::models::{ProviderRef, SnippetCommand, SnippetsConfig};
use crate::state::{SessionState, SessionTab, SidebarPanel};
use crate::theme::Palette;

/// 渲染会话右侧边栏。`width` 是侧栏宽度（含左边线）
#[allow(clippy::too_many_arguments)]
pub fn render_session_sidebar(
    tab: &SessionTab,
    active_panel: SidebarPanel,
    width: Pixels,
    session_state: Entity<SessionState>,
    lang: &Language,
    fit: &TextFitter,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let p = crate::theme::palette(cx);
    // 内容可用宽度：去掉左边线
    let width = (width - px(1.)).max(px(0.));

    let content = match active_panel {
        SidebarPanel::Snippets => {
            render_snippets_panel(&session_state, width, lang, fit, &p, cx).into_any_element()
        }
        SidebarPanel::Transfer => {
            render_transfer_panel(tab, &session_state, width, lang, fit, &p).into_any_element()
        }
        SidebarPanel::AiChat => {
            render_ai_chat_panel(tab, session_state.clone(), lang, window, cx).into_any_element()
        }
    };

    div()
        .size_full()
        .bg(p.sidebar)
        .flex()
        .flex_col()
        .child(content)
}

/// 面板标题条：标题 + 弱色的数量
fn sidebar_header(title: &'static str, count: Option<usize>, p: &Palette) -> Div {
    panel_header(p)
        .bg(p.sidebar)
        .pl(PANEL_PADDING)
        .pr(px(8.))
        .gap(px(6.))
        .child(panel_title(title, p))
        .children(count.map(|count| {
            div()
                .flex_none()
                .text_size(px(12.))
                .text_color(p.text_faint)
                .child(count.to_string())
        }))
        .child(div().flex_1())
}

// ==================== 传输面板 ====================

/// 传输行里用于联动悬停样式的分组名
const TRANSFER_ROW_GROUP: &str = "transfer-row";
/// 传输方向图标块的边长
const TRANSFER_TILE: f32 = 28.;

/// 渲染传输管理面板（当前标签的上传 / 下载任务）
fn render_transfer_panel(
    tab: &SessionTab,
    session_state: &Entity<SessionState>,
    width: Pixels,
    lang: &Language,
    fit: &TextFitter,
    p: &Palette,
) -> impl IntoElement {
    let transfers = &tab.active_transfers;
    let header = sidebar_header(
        i18n::t(lang, "transfer.title"),
        (!transfers.is_empty()).then_some(transfers.len()),
        p,
    );

    let body = if transfers.is_empty() {
        panel_state(
            "transfer-empty",
            StateIcon::Icon(icons::TRANSFER),
            p.text_faint,
            i18n::t(lang, "transfer.empty"),
            None,
            p,
        )
        .into_any_element()
    } else {
        div()
            .id("transfer-list-scroll")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scrollbar()
            .child(div().flex().flex_col().children(
                transfers.iter().map(|transfer| {
                    render_transfer_row(transfer, session_state, width, lang, fit, p)
                }),
            ))
            .into_any_element()
    };

    div()
        .size_full()
        .flex()
        .flex_col()
        .child(header)
        .child(body)
}

/// 传输状态对应的文字
fn transfer_status_label(transfer: &TransferItem, lang: &Language) -> &'static str {
    let key = match transfer.status {
        TransferStatus::Pending => "transfer.status.pending",
        // 按方向显示：上传不会显示成「下载中」
        TransferStatus::Uploading | TransferStatus::Downloading if transfer.is_upload => {
            "transfer.status.uploading"
        }
        TransferStatus::Uploading | TransferStatus::Downloading => "transfer.status.downloading",
        TransferStatus::Paused => "transfer.status.paused",
        TransferStatus::Completed => "transfer.status.completed",
        TransferStatus::Failed => "transfer.status.failed",
        TransferStatus::Cancelled => "transfer.status.cancelled",
    };
    i18n::t(lang, key)
}

/// 进度（0–1）：已完成的任务一律算满，0 字节文件也是
fn transfer_fraction(transfer: &TransferItem) -> f32 {
    if transfer.status == TransferStatus::Completed {
        1.
    } else {
        (transfer.progress.percentage() / 100.) as f32
    }
}

/// 传输行的第二行文字：进度、速度或状态说明
fn transfer_detail(transfer: &TransferItem, lang: &Language) -> String {
    let progress = &transfer.progress;
    let sizes = format!(
        "{} / {}",
        format_bytes(progress.bytes_transferred),
        format_bytes(progress.total_bytes)
    );
    match transfer.status {
        TransferStatus::Uploading | TransferStatus::Downloading
            if progress.speed_bytes_per_sec > 0 =>
        {
            format!(
                "{sizes} · {}",
                format_speed(progress.speed_bytes_per_sec as f64)
            )
        }
        TransferStatus::Uploading | TransferStatus::Downloading => sizes,
        TransferStatus::Completed => format!(
            "{} · {}",
            transfer_status_label(transfer, lang),
            format_bytes(progress.total_bytes)
        ),
        TransferStatus::Failed => match &transfer.error {
            Some(error) if !error.is_empty() => {
                format!("{}：{}", transfer_status_label(transfer, lang), error)
            }
            _ => transfer_status_label(transfer, lang).to_string(),
        },
        TransferStatus::Paused => format!("{} · {sizes}", transfer_status_label(transfer, lang)),
        TransferStatus::Pending | TransferStatus::Cancelled => {
            transfer_status_label(transfer, lang).to_string()
        }
    }
}

/// 一条传输：方向图标块、文件名、进度说明、百分比（悬停时换成暂停 / 继续、取消按钮）、进度条
fn render_transfer_row(
    transfer: &TransferItem,
    session_state: &Entity<SessionState>,
    width: Pixels,
    lang: &Language,
    fit: &TextFitter,
    p: &Palette,
) -> impl IntoElement {
    let tone = if transfer.is_upload {
        p.primary
    } else {
        p.success
    };
    let (bar_color, detail_color) = match transfer.status {
        TransferStatus::Completed => (p.success, p.text_faint),
        TransferStatus::Failed => (p.danger, p.danger),
        TransferStatus::Paused | TransferStatus::Cancelled | TransferStatus::Pending => {
            (p.text_faint, p.text_faint)
        }
        _ => (p.primary, p.text_faint),
    };
    let is_active = transfer.status.is_active();
    let is_paused = transfer.status == TransferStatus::Paused;
    let percent = format!("{:.0}%", transfer_fraction(transfer) * 100.);

    // 文字可用宽度：去掉左右留白、图标块、间距和右侧百分比 / 按钮
    let text_width = (width - PANEL_PADDING * 2. - px(TRANSFER_TILE + 10. + 8. + 52.)).max(px(40.));
    let name = transfer.file_name();
    let detail = transfer_detail(transfer, lang);

    let actions = is_active.then(|| {
        let pause_state = session_state.clone();
        let cancel_state = session_state.clone();
        let pause_id = transfer.id.clone();
        let cancel_id = transfer.id.clone();
        div()
            .absolute()
            .top_0()
            .right_0()
            .flex()
            .gap(px(2.))
            .invisible()
            .group_hover(TRANSFER_ROW_GROUP, |s| s.visible())
            .child(
                icon_button_sized(
                    SharedString::from(format!("transfer-pause-{}", transfer.id)),
                    if is_paused { icons::PLAY } else { icons::PAUSE },
                    px(24.),
                    12.,
                    i18n::t(
                        lang,
                        if is_paused {
                            "transfer.resume"
                        } else {
                            "transfer.pause"
                        },
                    ),
                    p,
                )
                .on_click(move |_, _, cx| {
                    pause_state.update(cx, |state, cx| {
                        if is_paused {
                            state.resume_transfer(&pause_id, cx);
                        } else {
                            state.pause_transfer(&pause_id, cx);
                        }
                    });
                }),
            )
            .child(
                icon_button_sized(
                    SharedString::from(format!("transfer-cancel-{}", transfer.id)),
                    icons::X,
                    px(24.),
                    12.,
                    i18n::t(lang, "transfer.cancel"),
                    p,
                )
                .on_click(move |_, _, cx| {
                    cancel_state.update(cx, |state, cx| {
                        state.cancel_transfer(&cancel_id, cx);
                    });
                }),
            )
    });

    div()
        .id(SharedString::from(format!("transfer-{}", transfer.id)))
        .group(TRANSFER_ROW_GROUP)
        .px(PANEL_PADDING)
        .py(px(10.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .border_b_1()
        .border_color(p.line)
        .hover(|s| s.bg(p.hover))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(
                    div()
                        .flex_none()
                        .size(px(TRANSFER_TILE))
                        .rounded(px(7.))
                        .bg(tone.opacity(0.14))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon_sized(
                            if transfer.is_upload {
                                icons::UPLOAD
                            } else {
                                icons::DOWNLOAD
                            },
                            14.,
                            tone,
                        )),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            fit.label_with_tooltip(
                                SharedString::from(format!("transfer-name-{}", transfer.id)),
                                &name,
                                text_width,
                                13.,
                                FontWeight::MEDIUM,
                            )
                            .text_color(p.text),
                        )
                        .child(
                            fit.label_with_tooltip(
                                SharedString::from(format!("transfer-detail-{}", transfer.id)),
                                &detail,
                                text_width,
                                11.,
                                FontWeight::NORMAL,
                            )
                            .text_color(detail_color),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .relative()
                        .w(px(52.))
                        .h(px(24.))
                        .flex()
                        .items_center()
                        .justify_end()
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(p.text_muted)
                                .when(is_active, |text| {
                                    text.group_hover(TRANSFER_ROW_GROUP, |s| s.invisible())
                                })
                                .child(percent),
                        )
                        .children(actions),
                ),
        )
        .child(meter_bar(transfer_fraction(transfer), bar_color, 3., p))
}

// ==================== 快捷命令面板 ====================

/// 每一级缩进的宽度
const SNIPPET_INDENT: f32 = 14.;
/// 箭头所在方框、图标的尺寸和它们之间的间距
const SNIPPET_CHEVRON: f32 = 12.;
const SNIPPET_ICON: f32 = 16.;
const SNIPPET_GAP: f32 = 6.;
/// 列表与行的左右留白
const SNIPPET_LIST_PADDING: f32 = 6.;
const SNIPPET_ROW_PADDING: f32 = 6.;
/// 行内用于联动悬停样式的分组名
const SNIPPET_ROW_GROUP: &str = "snippet-row";

/// 快捷命令树展开后的一行
enum SnippetRow {
    Group {
        id: String,
        name: String,
        expanded: bool,
        /// 组内（含子组）的命令数
        count: usize,
        has_children: bool,
        depth: usize,
    },
    Command {
        command: SnippetCommand,
        depth: usize,
    },
}

/// 一个组（含所有子组）里的命令总数
fn count_commands(config: &SnippetsConfig, group_id: &str) -> usize {
    config.get_commands_in_group(Some(group_id)).len()
        + config
            .get_child_groups(Some(group_id))
            .into_iter()
            .map(|child| count_commands(config, &child.id))
            .sum::<usize>()
}

/// 把快捷命令树按显示顺序展开成行：每层先列组，再列命令；只展开已展开的组
fn flatten_snippets(
    config: &SnippetsConfig,
    expanded: &std::collections::HashSet<String>,
    parent: Option<&str>,
    depth: usize,
    rows: &mut Vec<SnippetRow>,
) {
    for group in config.get_child_groups(parent) {
        let is_expanded = expanded.contains(&group.id);
        let has_children = !config.get_child_groups(Some(&group.id)).is_empty()
            || !config.get_commands_in_group(Some(&group.id)).is_empty();
        rows.push(SnippetRow::Group {
            id: group.id.clone(),
            name: group.name.clone(),
            expanded: is_expanded,
            count: count_commands(config, &group.id),
            has_children,
            depth,
        });
        if is_expanded {
            flatten_snippets(config, expanded, Some(&group.id), depth + 1, rows);
        }
    }
    for command in config.get_commands_in_group(parent) {
        rows.push(SnippetRow::Command {
            command: command.clone(),
            depth,
        });
    }
}

/// 某一层的行左侧缩进（不含行内留白）
fn snippet_indent(depth: usize) -> f32 {
    SNIPPET_INDENT * depth as f32
}

/// 渲染快捷命令面板
fn render_snippets_panel(
    session_state: &Entity<SessionState>,
    width: Pixels,
    lang: &Language,
    fit: &TextFitter,
    p: &Palette,
    cx: &App,
) -> impl IntoElement {
    let state = session_state.read(cx);
    let config = state.snippets_config.clone().unwrap_or_default();
    let expanded = state.snippets_expanded.clone();

    let mut rows = Vec::new();
    flatten_snippets(&config, &expanded, None, 0, &mut rows);

    let header = sidebar_header(
        i18n::t(lang, "session.rail.snippets"),
        (!config.commands.is_empty()).then_some(config.commands.len()),
        p,
    );

    let body = if rows.is_empty() {
        panel_state(
            "snippets-empty",
            StateIcon::Icon(icons::CODE),
            p.text_faint,
            i18n::t(lang, "snippets.empty.title"),
            Some(i18n::t(lang, "snippets.empty.session_hint").into()),
            p,
        )
        .into_any_element()
    } else {
        // 当前终端的 PTY：右键菜单「在终端执行」用
        let pty_channel: Option<Arc<crate::ssh::session::TerminalChannel>> = state
            .active_tab()
            .and_then(|tab| {
                tab.active_terminal_id
                    .as_ref()
                    .and_then(|id| tab.terminals.iter().find(|t| &t.id == id))
            })
            .and_then(|inst| inst.pty_channel.clone());

        div()
            .id("snippets-tree-scroll")
            .flex_1()
            .min_h(px(0.))
            .overflow_y_scrollbar()
            .child(
                div()
                    .p(px(SNIPPET_LIST_PADDING))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows.into_iter().map(|row| {
                        match row {
                            SnippetRow::Group {
                                id,
                                name,
                                expanded,
                                count,
                                has_children,
                                depth,
                            } => render_group_row(
                                &id,
                                &name,
                                expanded,
                                count,
                                has_children,
                                depth,
                                session_state,
                                width,
                                fit,
                                p,
                            )
                            .into_any_element(),
                            SnippetRow::Command { command, depth } => render_command_row(
                                command,
                                depth,
                                session_state,
                                pty_channel.clone(),
                                width,
                                lang,
                                fit,
                                p,
                                cx,
                            )
                            .into_any_element(),
                        }
                    })),
            )
            .into_any_element()
    };

    div()
        .size_full()
        .flex()
        .flex_col()
        .child(header)
        .child(body)
}

/// 名称可用宽度：侧栏宽度减去列表和行的留白、缩进、箭头、图标和右侧数量
fn snippet_name_width(width: Pixels, depth: usize, trailing: Pixels) -> Pixels {
    let used = SNIPPET_LIST_PADDING * 2.
        + SNIPPET_ROW_PADDING * 2.
        + snippet_indent(depth)
        + SNIPPET_CHEVRON
        + SNIPPET_GAP
        + SNIPPET_ICON
        + SNIPPET_GAP;
    (width - px(used) - trailing).max(px(24.))
}

/// 组的一行：箭头、文件夹图标、名称和命令数，点击展开 / 收起
#[allow(clippy::too_many_arguments)]
fn render_group_row(
    id: &str,
    name: &str,
    expanded: bool,
    count: usize,
    has_children: bool,
    depth: usize,
    session_state: &Entity<SessionState>,
    width: Pixels,
    fit: &TextFitter,
    p: &Palette,
) -> impl IntoElement {
    let session_state = session_state.clone();
    let group_id = id.to_string();
    let count_text = count.to_string();
    let count_width = fit.width(&count_text, 11., FontWeight::NORMAL) + px(SNIPPET_GAP);
    let (hover_bg, hover_text) = (p.hover, p.text);

    div()
        .id(SharedString::from(format!("snippet-group-{id}")))
        .group(SNIPPET_ROW_GROUP)
        .flex_none()
        .h(LIST_ROW_HEIGHT)
        .pl(px(SNIPPET_ROW_PADDING + snippet_indent(depth)))
        .pr(px(SNIPPET_ROW_PADDING))
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(SNIPPET_GAP))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .on_click(move |_, _, cx| {
            session_state.update(cx, |state, cx| {
                state.toggle_snippets_group(&group_id);
                cx.notify();
            });
        })
        .child(
            icon_sized(
                if expanded {
                    icons::CHEVRON_DOWN
                } else {
                    icons::CHEVRON_RIGHT
                },
                SNIPPET_CHEVRON,
                if has_children {
                    p.text_faint
                } else {
                    p.text_faint.opacity(0.4)
                },
            )
            .group_hover(SNIPPET_ROW_GROUP, move |s| s.text_color(hover_text)),
        )
        .child(icon_sized(
            if expanded {
                icons::FOLDER_OPEN
            } else {
                icons::FOLDER
            },
            SNIPPET_ICON,
            p.text_muted,
        ))
        .child(
            fit.label_with_tooltip(
                SharedString::from(format!("snippet-group-name-{id}")),
                name,
                snippet_name_width(width, depth, count_width),
                13.,
                FontWeight::MEDIUM,
            )
            .flex_none()
            .text_color(p.text),
        )
        .child(div().flex_1())
        .child(
            div()
                .flex_none()
                .text_size(px(11.))
                .text_color(p.text_faint)
                .child(count_text),
        )
}

/// 命令的一行：图标与同级组的文件夹图标对齐，点击复制命令（图标短暂变成对勾），右键可执行或放进命令框
#[allow(clippy::too_many_arguments)]
fn render_command_row(
    command: SnippetCommand,
    depth: usize,
    session_state: &Entity<SessionState>,
    pty_channel: Option<Arc<crate::ssh::session::TerminalChannel>>,
    width: Pixels,
    lang: &Language,
    fit: &TextFitter,
    p: &Palette,
    cx: &App,
) -> impl IntoElement {
    let copy_key = format!("snippet-{}", command.id);
    let copied = session_state.read(cx).is_copied(&copy_key);
    let (hover_bg, hover_text) = (p.hover, p.text);

    let command_text = command.command.clone();
    let command_for_execute = command.command.clone();
    let command_for_edit = command.command.clone();
    let execute_label = i18n::t(lang, "snippets.context_menu.execute");
    let edit_label = i18n::t(lang, "snippets.context_menu.edit_in_box");
    let session_for_copy = session_state.clone();
    let session_for_menu = session_state.clone();

    div()
        .id(SharedString::from(format!("cmd-{}", command.id)))
        .group(SNIPPET_ROW_GROUP)
        .flex_none()
        .h(LIST_ROW_HEIGHT)
        // 命令没有箭头：留出箭头的位置，图标与同级组的文件夹图标对齐
        .pl(px(SNIPPET_ROW_PADDING
            + snippet_indent(depth)
            + SNIPPET_CHEVRON
            + SNIPPET_GAP))
        .pr(px(SNIPPET_ROW_PADDING))
        .rounded(px(6.))
        .flex()
        .items_center()
        .gap(px(SNIPPET_GAP))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .on_click(move |_, _, cx| {
            SessionState::copy_with_feedback(
                &session_for_copy,
                copy_key.clone(),
                command_text.clone(),
                cx,
            );
        })
        // 悬停显示完整命令
        .tooltip(text_tooltip(command.command.clone()))
        .context_menu(move |menu, _window, _cx| {
            let command_for_execute = command_for_execute.clone();
            let command_for_edit = command_for_edit.clone();
            let pty_for_menu = pty_channel.clone();
            let session_for_menu = session_for_menu.clone();
            menu.item(snippet_menu_item(icons::PLAY, execute_label).on_click(
                move |_, _window, _cx| {
                    if let Some(channel) = &pty_for_menu {
                        let channel = Arc::clone(channel);
                        debug!("[ContextMenu] Executing command: {}", command_for_execute);
                        let mut bytes = command_for_execute.clone().into_bytes();
                        bytes.push(0x0d); // CR
                        if let Err(e) = channel.queue_write(bytes) {
                            tracing::error!("[ContextMenu] Failed to send command: {:?}", e);
                        }
                    }
                },
            ))
            .item(
                snippet_menu_item(icons::EDIT, edit_label).on_click(move |_, window, cx| {
                    let command = command_for_edit.clone();
                    debug!("[ContextMenu] Edit in command box: {}", command);
                    session_for_menu.update(cx, |state, cx| {
                        state.set_command_input_text(command, window, cx);
                    });
                }),
            )
        })
        .child(
            div()
                .flex_none()
                .size(px(SNIPPET_ICON))
                .flex()
                .items_center()
                .justify_center()
                .child(if copied {
                    icon_sized(icons::CHECK, 14., p.success)
                } else {
                    icon_sized(icons::CODE, 14., p.text_faint)
                        .group_hover(SNIPPET_ROW_GROUP, move |s| s.text_color(hover_text))
                }),
        )
        .child(
            fit.label(
                &command.name,
                snippet_name_width(width, depth, px(0.)),
                13.,
                FontWeight::NORMAL,
            )
            .flex_none()
            .text_color(p.text_muted)
            .group_hover(SNIPPET_ROW_GROUP, move |s| s.text_color(hover_text)),
        )
}

/// 快捷命令右键菜单的一项：图标 + 文字
fn snippet_menu_item(icon: &'static str, label: &'static str) -> PopupMenuItem {
    PopupMenuItem::element(move |_window, cx| {
        let p = crate::theme::palette(cx);
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(icon_sized(icon, 14., p.text_muted))
            .child(div().text_size(px(13.)).text_color(p.text).child(label))
    })
}

// ==================== AI 对话面板 ====================

fn render_ai_chat_panel(
    tab: &SessionTab,
    session_state: Entity<SessionState>,
    lang: &crate::models::settings::Language,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    use gpui::Corner;
    use gpui_component::button::{Button, ButtonVariants};
    use gpui_component::input::Input;
    use gpui_component::menu::{DropdownMenu, PopupMenuItem};
    use gpui_component::Disableable;
    use gpui_component::Sizable;

    let tab_id = tab.id.clone();
    let chat = &tab.ai_chat;
    let foreground = cx.theme().foreground;
    let muted_foreground = cx.theme().muted_foreground;
    let border = cx.theme().border;
    let p = crate::theme::palette(cx);

    // 当前可用供应商（内置 + 自定义）与当前生效供应商
    let settings = crate::services::storage::cached_settings();
    let verified_refs = settings.ai.verified_provider_refs();
    let has_any = !verified_refs.is_empty();
    let active_ref = session_state.read(cx).resolve_ai_provider_ref(&tab_id);
    let active_resolved = active_ref.as_ref().and_then(|r| settings.ai.resolve(r));
    let has_messages = !chat.messages.is_empty() || chat.pending;

    // 输入框（如果存在）
    let input_entity = session_state.read(cx).get_ai_chat_input(&tab_id);

    // ===== 顶部：与其他面板一致的 36px 标题条 =====
    let header = {
        let session_for_clear = session_state.clone();
        let tab_id_for_clear = tab_id.clone();
        let new_thread_tooltip = crate::i18n::t(lang, "ai_chat.new_thread");
        let header_title: SharedString = if has_messages {
            SharedString::from(tab.server_label.clone())
        } else {
            crate::i18n::t(lang, "ai_chat.new_thread").into()
        };
        let provider_icon: AnyElement = match (&active_ref, &active_resolved) {
            (Some(ProviderRef::Builtin(id)), _) => img(id.icon_path())
                .w(px(15.))
                .h(px(15.))
                .flex_shrink_0()
                .into_any_element(),
            (Some(ProviderRef::Custom(_)), Some(provider)) => {
                render_letter_avatar(ProviderRef::avatar_char(&provider.name), 15.)
                    .into_any_element()
            }
            _ => icon_sized(icons::SPARKLES, 14., p.text_muted).into_any_element(),
        };

        panel_header(&p)
            .bg(p.sidebar)
            .pl(PANEL_PADDING)
            .pr(px(6.))
            .gap(px(8.))
            .child(provider_icon)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text)
                    .child(header_title),
            )
            .child(
                icon_button_sized(
                    "ai-chat-new-thread",
                    icons::PLUS,
                    px(28.),
                    14.,
                    new_thread_tooltip,
                    &p,
                )
                .on_click(move |_, _, cx| {
                    session_for_clear.update(cx, |s, cx| {
                        s.clear_ai_chat(&tab_id_for_clear, cx);
                    });
                }),
            )
    };

    // ===== 中部：用户消息为编辑器卡片，Agent 回复直接铺排 =====
    // Zed 在空 thread 时完全不渲染消息空状态，message editor 会占满这里。
    let messages_view = has_messages.then(|| {
        div()
            .id("ai-chat-messages")
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .py_2()
            .flex()
            .flex_col()
            .gap_1()
            .children(
                chat.messages
                    .iter()
                    .enumerate()
                    .filter(|(_, msg)| {
                        // 跳过流式开始前的空助手占位消息——此时由下方「思考中」指示器代为占位，
                        // 避免出现一个空气泡 + 思考气泡同时显示
                        !(msg.role == crate::services::ai::ChatRole::Assistant
                            && !msg.error
                            && msg.content.is_empty()
                            && msg
                                .reasoning
                                .as_ref()
                                .map(|r| r.trim().is_empty())
                                .unwrap_or(true))
                    })
                    .map(|(idx, msg)| {
                        let is_streaming = chat.pending
                            && idx + 1 == chat.messages.len()
                            && msg.role == crate::services::ai::ChatRole::Assistant;
                        render_ai_chat_message(
                            idx,
                            msg,
                            is_streaming,
                            session_state.clone(),
                            tab_id.clone(),
                            lang,
                            window,
                            cx,
                        )
                    }),
            )
            .when(
                chat.pending
                    && chat
                        .messages
                        .last()
                        .map(|m| {
                            m.role == crate::services::ai::ChatRole::Assistant
                                && m.content.is_empty()
                                && m.reasoning.as_ref().map(|r| r.is_empty()).unwrap_or(true)
                        })
                        .unwrap_or(true),
                |this| {
                    this.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .px_5()
                            .py_1()
                            .child(
                                svg()
                                    .path(icons::SPARKLES)
                                    .size(px(13.))
                                    .text_color(muted_foreground),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted_foreground)
                                    .child(crate::i18n::t(lang, "ai_chat.thinking")),
                            ),
                    )
                },
            )
            .into_any_element()
    });

    // ===== 底部：Zed 风格 message editor + 工具栏 =====
    let input_area = {
        let session_for_send = session_state.clone();
        let tab_id_for_send = tab_id.clone();
        let pending = chat.pending;
        let has_input = input_entity.is_some();

        // 当前供应商的可选模型列表 + 当前生效模型
        let model_list: Vec<String> = active_resolved
            .as_ref()
            .map(|r| r.models.clone())
            .unwrap_or_default();
        let current_model: SharedString = {
            let sel = chat
                .selected_model
                .as_ref()
                .filter(|m| model_list.iter().any(|x| x == *m));
            match sel {
                Some(m) => SharedString::from(m.clone()),
                None => model_list
                    .first()
                    .cloned()
                    .or_else(|| active_resolved.as_ref().map(|r| r.model.clone()))
                    .filter(|m| !m.is_empty())
                    .map(SharedString::from)
                    .unwrap_or_else(|| SharedString::from("—")),
            }
        };

        // 供应商选择器与模型一起归入 message editor 底栏。
        let provider_dropdown = {
            let tab_id_for_pick = tab_id.clone();
            let session_for_pick = session_state.clone();
            let menu_items: Vec<(ProviderRef, String, Option<&'static str>)> = verified_refs
                .iter()
                .map(|provider_ref| match provider_ref {
                    ProviderRef::Builtin(id) => (
                        provider_ref.clone(),
                        id.label().to_string(),
                        Some(id.icon_path()),
                    ),
                    ProviderRef::Custom(_) => (
                        provider_ref.clone(),
                        settings
                            .ai
                            .resolve(provider_ref)
                            .map(|provider| provider.name)
                            .unwrap_or_default(),
                        None,
                    ),
                })
                .collect();
            let trigger_label: SharedString = active_resolved
                .as_ref()
                .map(|provider| SharedString::from(provider.name.clone()))
                .unwrap_or_else(|| crate::i18n::t(lang, "ai_chat.no_provider_short").into());
            let trigger_icon: Option<AnyElement> = match (&active_ref, &active_resolved) {
                (Some(ProviderRef::Builtin(id)), _) => Some(
                    img(id.icon_path())
                        .w(px(14.))
                        .h(px(14.))
                        .flex_shrink_0()
                        .into_any_element(),
                ),
                (Some(ProviderRef::Custom(_)), Some(provider)) => Some(
                    render_letter_avatar(ProviderRef::avatar_char(&provider.name), 14.)
                        .into_any_element(),
                ),
                _ => None,
            };

            Button::new("ai-chat-provider-btn")
                .ghost()
                .xsmall()
                .compact()
                .disabled(!has_any)
                .tooltip(trigger_label)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .min_w_0()
                        .gap(px(3.))
                        .children(trigger_icon)
                        .child(
                            svg()
                                .path(icons::CHEVRON_DOWN)
                                .size(px(11.))
                                .text_color(muted_foreground),
                        ),
                )
                .dropdown_menu_with_anchor(Corner::BottomLeft, move |menu, _, _| {
                    let mut menu = menu.min_w(px(180.));
                    for (provider_ref, name, builtin_icon) in &menu_items {
                        let provider_ref = provider_ref.clone();
                        let label: SharedString = SharedString::from(name.clone());
                        let builtin_icon = *builtin_icon;
                        let avatar_char = ProviderRef::avatar_char(name);
                        let session = session_for_pick.clone();
                        let tab_id = tab_id_for_pick.clone();
                        menu = menu.item(
                            PopupMenuItem::element(move |_window, cx| {
                                let icon: AnyElement = match builtin_icon {
                                    Some(path) => {
                                        img(path).w(px(16.)).h(px(16.)).into_any_element()
                                    }
                                    None => {
                                        render_letter_avatar(avatar_char, 16.).into_any_element()
                                    }
                                };
                                div().flex().items_center().gap(px(8.)).child(icon).child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().foreground)
                                        .child(label.clone()),
                                )
                            })
                            .on_click(move |_, _, cx| {
                                session.update(cx, |state, cx| {
                                    state.set_ai_chat_provider(&tab_id, provider_ref.clone());
                                    cx.notify();
                                });
                            }),
                        );
                    }
                    menu
                })
        };

        // 模型下拉触发器
        let model_dropdown = {
            let models = model_list.clone();
            let session_for_model = session_state.clone();
            let tab_id_for_model = tab_id.clone();
            let label_color = if has_any {
                foreground
            } else {
                muted_foreground
            };
            Button::new("ai-chat-model-btn")
                .ghost()
                .xsmall()
                .compact()
                .disabled(!has_any || model_list.is_empty())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(5.))
                        .min_w_0()
                        .child(
                            div()
                                .text_xs()
                                .text_color(label_color)
                                .max_w(px(76.))
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .child(current_model.clone()),
                        )
                        .child(render_icon(icons::CHEVRON_DOWN, muted_foreground.into())),
                )
                .dropdown_menu_with_anchor(Corner::BottomLeft, move |menu, _, _| {
                    let mut menu = menu.min_w(px(180.));
                    for m in &models {
                        let model_owned = m.clone();
                        let label: SharedString = SharedString::from(m.clone());
                        let session = session_for_model.clone();
                        let tab_id_owned = tab_id_for_model.clone();
                        menu = menu.item(PopupMenuItem::new(label).on_click(move |_, _, cx| {
                            session.update(cx, |s, cx| {
                                s.set_ai_chat_model(&tab_id_owned, model_owned.clone());
                                cx.notify();
                            });
                        }));
                    }
                    menu
                })
        };

        // Zed 使用填充式发送按钮，让主动作在密集的工具栏里仍然明确。
        let send_disabled = pending || !has_any || !has_input;
        let send_icon_color = if send_disabled {
            muted_foreground
        } else {
            cx.theme().primary_foreground
        };
        let send_btn = Button::new("ai-chat-send")
            .primary()
            .xsmall()
            .compact()
            .disabled(send_disabled)
            .child(
                svg()
                    .path(icons::SEND)
                    .size(px(16.))
                    .flex_shrink_0()
                    .text_color(send_icon_color),
            )
            .on_click(move |_, window, cx| {
                session_for_send.update(cx, |s, cx| {
                    s.send_ai_chat_message(&tab_id_for_send, window, cx);
                });
            });

        let context_enabled = settings.ai.context.enabled;
        let context_status = div()
            .flex()
            .items_center()
            .min_w_0()
            .gap(px(5.))
            .text_color(muted_foreground)
            .child(
                svg()
                    .path(icons::TERMINAL)
                    .size(px(13.))
                    .flex_shrink_0()
                    .text_color(muted_foreground),
            )
            .child(
                div()
                    .max_w(px(82.))
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .text_xs()
                    .child(crate::i18n::t(
                        lang,
                        if context_enabled {
                            "ai_chat.terminal_context"
                        } else {
                            "ai_chat.context_off"
                        },
                    )),
            );

        let input_control = input_entity.as_ref().map(|input| {
            let input = Input::new(input).appearance(false);
            if has_messages {
                input
            } else {
                input.h_full()
            }
        });

        div()
            .bg(cx.theme().background)
            .when(has_messages, |this| {
                this.flex_shrink_0().border_t_1().border_color(border)
            })
            .when(!has_messages, |this| this.flex_1().min_h_0())
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .min_h(px(54.))
                            .px_3()
                            .pt_3()
                            .pb_2()
                            .when(!has_messages, |this| this.flex_1().min_h_0())
                            .children(input_control),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .px_2()
                            .pb_2()
                            .child(context_status)
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .min_w_0()
                                    .gap_1()
                                    .child(provider_dropdown)
                                    .child(model_dropdown)
                                    .child(send_btn),
                            ),
                    ),
            )
    };

    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .child(header)
        .children(messages_view)
        .child(input_area)
}

fn render_ai_chat_message(
    idx: usize,
    msg: &crate::state::AiChatMessage,
    is_streaming: bool,
    session_state: Entity<SessionState>,
    tab_id: String,
    lang: &crate::models::settings::Language,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    use crate::services::ai::ChatRole;
    use gpui_component::text::TextView;

    let is_user = msg.role == ChatRole::User;
    let text_color = if msg.error {
        cx.theme().danger
    } else {
        cx.theme().foreground
    };
    let muted_fg = cx.theme().muted_foreground;
    let border = cx.theme().border;

    // gpui-component 的 Markdown TextView 会等待 200ms 无更新后再解析；连续 token
    // 会一直重置计时器。生成期间先直接绘制文本，结束后再切换为完整 Markdown，
    // 这样正文和 reasoning 一样能逐 token 出现，又保留最终的富文本效果。
    let body: AnyElement = if is_user || msg.error || is_streaming {
        div()
            .text_sm()
            .line_height(relative(1.45))
            .text_color(text_color)
            .whitespace_normal()
            .child(SharedString::from(msg.content.clone()))
            .into_any_element()
    } else {
        use gpui::StyleRefinement;
        use gpui_component::text::TextViewStyle;
        // Zed 的 Agent 回复没有外层气泡；代码块自身用编辑器底色承载。
        let code_bg = if cx.theme().mode.is_dark() {
            gpui::black().opacity(0.36)
        } else {
            cx.theme().secondary
        };
        let code_style = StyleRefinement::default().bg(code_bg).rounded(px(5.)).p_2();

        div()
            .text_sm()
            .line_height(relative(1.45))
            .child(
                TextView::markdown(
                    SharedString::from(format!("ai-md-{}", idx)),
                    msg.content.clone(),
                    window,
                    cx,
                )
                .style(
                    TextViewStyle::default()
                        .paragraph_gap(gpui::rems(0.65))
                        .code_block(code_style),
                )
                .selectable(true)
                .code_block_actions(|cb, _window, cx| {
                    let code = cb.code();
                    let copy_color = cx.theme().muted_foreground;
                    div()
                        .id(SharedString::from(format!("md-copy-{}", code.len())))
                        .cursor_pointer()
                        .px_1()
                        .py(px(2.))
                        .rounded(px(3.))
                        .hover(move |s| s.bg(copy_color.opacity(0.15)))
                        .child(
                            svg()
                                .path(crate::constants::icons::COPY)
                                .size(px(12.))
                                .text_color(copy_color),
                        )
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(code.to_string()));
                        })
                        .into_any_element()
                }),
            )
            .into_any_element()
    };

    // 思考过程块（DeepSeek-R1 等会带 reasoning），可折叠。
    // Zed 使用一行低对比度标题，展开内容再以缩进竖线承载。
    let reasoning_block: Option<AnyElement> = msg.reasoning.as_ref().and_then(|r| {
        let r_owned = r.trim().to_string();
        if r_owned.is_empty() {
            return None;
        }
        let collapsed = msg.reasoning_collapsed;
        let session_for_toggle = session_state.clone();
        let tab_id_for_toggle = tab_id.clone();
        Some(
            div()
                .mb_2()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    div()
                        .id(SharedString::from(format!("ai-reason-toggle-{}", idx)))
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .cursor_pointer()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(
                                    svg()
                                        .path(icons::SPARKLES)
                                        .size(px(12.))
                                        .text_color(muted_fg),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(muted_fg)
                                        .child(crate::i18n::t(lang, "ai_chat.reasoning")),
                                ),
                        )
                        .child(
                            svg()
                                .path(if collapsed {
                                    icons::CHEVRON_RIGHT
                                } else {
                                    icons::CHEVRON_DOWN
                                })
                                .size(px(12.))
                                .text_color(muted_fg),
                        )
                        .on_click(move |_, _, cx| {
                            session_for_toggle.update(cx, |s, cx| {
                                s.toggle_ai_reasoning(&tab_id_for_toggle, idx, cx);
                            });
                        }),
                )
                .when(!collapsed, |this| {
                    this.child(
                        div()
                            .ml(px(5.))
                            .pl_3()
                            .py_1()
                            .border_l_1()
                            .border_color(border)
                            .text_xs()
                            .line_height(relative(1.45))
                            .text_color(muted_fg)
                            .whitespace_normal()
                            .child(SharedString::from(r_owned)),
                    )
                })
                .into_any_element(),
        )
    });

    // 与 Zed 一致：用户输入是全宽 editor card，Agent 输出直接落在正文画布上。
    let message = div()
        .id(SharedString::from(format!("ai-msg-{}", idx)))
        .w_full()
        .flex()
        .flex_col()
        .overflow_hidden()
        .when(is_user, |this| {
            this.px_2()
                .py_3()
                .rounded(px(8.))
                .bg(cx.theme().background)
                .border_1()
                .border_color(border)
                .shadow_sm()
        })
        .when(msg.error, |this| {
            this.w_full()
                .px_3()
                .py_2()
                .rounded(px(6.))
                .bg(cx.theme().danger.opacity(0.1))
                .border_1()
                .border_color(cx.theme().danger.opacity(0.3))
        })
        .when(!is_user && !msg.error, |this| this.w_full().px_3().py_2())
        .children(reasoning_block)
        .child(body);

    div()
        .w_full()
        .overflow_hidden()
        .when(is_user, |this| this.px_2())
        .when(!is_user && !msg.error, |this| this.px_2())
        .child(message)
}
