// Session 右侧边栏组件 - 快捷命令树状显示

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_component::scroll::ScrollableElement;
use gpui_component::tooltip::Tooltip;
use gpui_component::{ActiveTheme, StyledExt};
use std::sync::Arc;
use tracing::debug;

use crate::components::common::icon::{render_icon, render_letter_avatar};
use crate::constants::icons;
use crate::models::{ProviderRef, SnippetCommand, SnippetGroup, SnippetsConfig};
use crate::state::{SessionState, SessionTab, SidebarPanel};

/// 渲染会话右侧边栏
pub fn render_session_sidebar(
    _tab: &SessionTab,
    active_panel: SidebarPanel,
    session_state: Entity<SessionState>,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let bg_color = crate::theme::sidebar_color(cx);

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    // 根据不同面板显示不同内容（标题栏已移除，内容占满空间）
    let content = match active_panel {
        SidebarPanel::Snippets => {
            render_snippets_tree(session_state.clone(), cx).into_any_element()
        }
        SidebarPanel::Transfer => {
            render_transfer_panel(session_state.clone(), &lang, cx).into_any_element()
        }
        SidebarPanel::AiChat => {
            render_ai_chat_panel(_tab, session_state.clone(), &lang, window, cx).into_any_element()
        }
    };

    div()
        .size_full()
        .bg(bg_color)
        .flex()
        .flex_col()
        .child(content)
}

/// 渲染传输管理面板
fn render_transfer_panel(
    session_state: Entity<SessionState>,
    lang: &crate::models::settings::Language,
    cx: &App,
) -> impl IntoElement {
    let state = session_state.read(cx);

    // 获取当前活动 tab 的传输列表
    let transfers: Vec<_> = state
        .active_tab_id
        .as_ref()
        .and_then(|tab_id| state.tabs.iter().find(|t| &t.id == tab_id))
        .map(|tab| tab.active_transfers.iter().collect())
        .unwrap_or_default();

    let muted_foreground = cx.theme().muted_foreground;
    let foreground = cx.theme().foreground;
    let primary = cx.theme().primary;
    let destructive: Hsla = gpui::rgb(0xef4444).into();
    let success: Hsla = gpui::rgb(0x22c55e).into();

    if transfers.is_empty() {
        // 空状态
        div()
            .id("transfer-panel-empty")
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_2()
            .pt_8()
            .child(render_icon(icons::CLOUD, muted_foreground.into()))
            .child(
                div()
                    .text_xs()
                    .text_color(muted_foreground)
                    .child(crate::i18n::t(lang, "transfer.empty")),
            )
            .into_any_element()
    } else {
        // 传输列表
        div()
            .id("transfer-list-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_2()
            .children(transfers.iter().enumerate().map(|(idx, transfer)| {
                let progress_percent = transfer.progress.percentage();
                let status_text = transfer.status.display_text();
                let status_color = if transfer.status.is_error() {
                    destructive
                } else if transfer.status.is_complete() {
                    success
                } else {
                    primary
                };

                div()
                    .id(SharedString::from(format!("transfer-{}", idx)))
                    .p_3()
                    .bg(cx.theme().muted)
                    .rounded(px(6.))
                    .flex()
                    .flex_col()
                    .gap_2()
                    // 文件名和状态
                    .child({
                        let transfer_id = transfer.id.clone();
                        let session_state_for_cancel = session_state.clone();
                        let is_active =
                            !transfer.status.is_complete() && !transfer.status.is_error();
                        let cancel_color = cx.theme().muted_foreground;

                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    // 上传/下载方向图标
                                    .child(render_icon(
                                        if transfer.is_upload {
                                            icons::UPLOAD
                                        } else {
                                            icons::DOWNLOAD
                                        },
                                        if transfer.is_upload {
                                            gpui::rgb(0x3b82f6).into() // 蓝色表示上传
                                        } else {
                                            gpui::rgb(0x22c55e).into() // 绿色表示下载
                                        },
                                    ))
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_medium()
                                            .text_color(foreground)
                                            .overflow_hidden()
                                            .max_w(px(110.))
                                            .child(transfer.file_name()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div().text_xs().text_color(status_color).child(status_text),
                                    )
                                    // 暂停/继续按钮（仅在下载中或已暂停时显示）
                                    .when(is_active, |this| {
                                        let is_paused = transfer.status
                                            == crate::models::sftp::TransferStatus::Paused;
                                        let session_state_for_pause = session_state.clone();
                                        let transfer_id_pause = transfer_id.clone();
                                        let button_color = cx.theme().muted_foreground;

                                        this.child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "pause-{}",
                                                    transfer_id_pause
                                                )))
                                                .cursor_pointer()
                                                .rounded(px(2.))
                                                .p(px(2.))
                                                .hover(|s| s.bg(button_color.opacity(0.2)))
                                                .child(if is_paused {
                                                    // 显示播放图标（继续）
                                                    render_icon(icons::PLAY, button_color)
                                                } else {
                                                    // 显示暂停图标
                                                    render_icon(icons::PAUSE, button_color)
                                                })
                                                .on_click({
                                                    move |_, _, cx| {
                                                        session_state_for_pause.update(
                                                            cx,
                                                            |state, cx| {
                                                                if is_paused {
                                                                    state.resume_transfer(
                                                                        &transfer_id_pause,
                                                                        cx,
                                                                    );
                                                                } else {
                                                                    state.pause_transfer(
                                                                        &transfer_id_pause,
                                                                        cx,
                                                                    );
                                                                }
                                                            },
                                                        );
                                                    }
                                                }),
                                        )
                                    })
                                    // 取消按钮（仅在传输中显示）
                                    .when(is_active, |this| {
                                        this.child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "cancel-{}",
                                                    transfer_id
                                                )))
                                                .cursor_pointer()
                                                .rounded(px(2.))
                                                .p(px(2.))
                                                .hover(|s| s.bg(cancel_color.opacity(0.2)))
                                                .child(render_icon(icons::X, cancel_color))
                                                .on_click({
                                                    let transfer_id = transfer_id.clone();
                                                    move |_, _, cx| {
                                                        session_state_for_cancel.update(
                                                            cx,
                                                            |state, cx| {
                                                                state.cancel_transfer(
                                                                    &transfer_id,
                                                                    cx,
                                                                );
                                                            },
                                                        );
                                                    }
                                                }),
                                        )
                                    }),
                            )
                    })
                    // 进度条
                    .child(
                        div()
                            .h(px(4.))
                            .w_full()
                            .bg(cx.theme().muted_foreground.opacity(0.2))
                            .rounded_full()
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(progress_percent as f32 / 100.0))
                                    .bg(status_color)
                                    .rounded_full(),
                            ),
                    )
                    // 进度详情
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().text_xs().text_color(muted_foreground).child(
                                        format!(
                                            "{} / {}",
                                            format_bytes(transfer.progress.bytes_transferred),
                                            format_bytes(transfer.progress.total_bytes)
                                        ),
                                    ))
                                    // 下载速度（仅在下载中显示）
                                    .when(
                                        transfer.progress.speed_bytes_per_sec > 0
                                            && !transfer.status.is_complete()
                                            && !transfer.status.is_error(),
                                        |this| {
                                            this.child(div().text_xs().text_color(primary).child(
                                                format!(
                                                    "{}",
                                                    format_speed(
                                                        transfer.progress.speed_bytes_per_sec
                                                    )
                                                ),
                                            ))
                                        },
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted_foreground)
                                    .child(format!("{:.0}%", progress_percent)),
                            ),
                    )
            }))
            .into_any_element()
    }
}

/// 格式化字节数
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

/// 格式化速度
fn format_speed(bytes_per_sec: u64) -> String {
    if bytes_per_sec < 1024 {
        format!("{} B/s", bytes_per_sec)
    } else if bytes_per_sec < 1024 * 1024 {
        format!("{:.1} KB/s", bytes_per_sec as f64 / 1024.0)
    } else if bytes_per_sec < 1024 * 1024 * 1024 {
        format!("{:.1} MB/s", bytes_per_sec as f64 / (1024.0 * 1024.0))
    } else {
        format!(
            "{:.2} GB/s",
            bytes_per_sec as f64 / (1024.0 * 1024.0 * 1024.0)
        )
    }
}

/// 渲染快捷命令树
fn render_snippets_tree(session_state: Entity<SessionState>, cx: &App) -> impl IntoElement {
    // 获取配置
    let state = session_state.read(cx);
    let config = state.snippets_config.clone().unwrap_or_default();
    let expanded = state.snippets_expanded.clone();

    // 获取根级组和命令
    let root_groups = config.get_child_groups(None);
    let root_commands = config.get_commands_in_group(None);

    let has_items = !root_groups.is_empty() || !root_commands.is_empty();

    div()
        .id("snippets-tree-scroll")
        .flex_1()
        .overflow_y_scroll()
        .p_2()
        .child(if has_items {
            render_tree_nodes(
                session_state.clone(),
                &config,
                &expanded,
                root_groups,
                root_commands,
                0,
                cx,
            )
            .into_any_element()
        } else {
            render_empty_tree(cx).into_any_element()
        })
}

/// 渲染空状态
fn render_empty_tree(cx: &App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_2()
        .pt_8()
        .child(render_icon(icons::CODE, muted.into()))
        .child(
            div()
                .text_xs()
                .text_color(muted)
                .child(crate::i18n::t(&lang, "snippets.empty.title")),
        )
}

/// 递归渲染树节点
fn render_tree_nodes(
    session_state: Entity<SessionState>,
    config: &SnippetsConfig,
    expanded: &std::collections::HashSet<String>,
    groups: Vec<&SnippetGroup>,
    commands: Vec<&SnippetCommand>,
    level: usize,
    cx: &App,
) -> impl IntoElement {
    let indent = px((level * 16) as f32);
    let config_clone = config.clone();

    div()
        .flex()
        .flex_col()
        // 渲染文件夹节点
        .children(groups.into_iter().map(|group| {
            let group_owned = group.clone();
            let is_expanded = expanded.contains(&group.id);
            let child_groups = config_clone.get_child_groups(Some(&group.id));
            let child_commands = config_clone.get_commands_in_group(Some(&group.id));
            let has_children = !child_groups.is_empty() || !child_commands.is_empty();

            render_folder_node(
                session_state.clone(),
                group_owned,
                is_expanded,
                has_children,
                child_groups
                    .into_iter()
                    .map(|g| g.clone())
                    .collect::<Vec<_>>(),
                child_commands
                    .into_iter()
                    .map(|c| c.clone())
                    .collect::<Vec<_>>(),
                level,
                expanded.clone(),
                config_clone.clone(),
                cx,
            )
        }))
        // 渲染命令节点
        .children(commands.into_iter().map({
            let session_state = session_state.clone();
            move |command| {
                let command_owned = command.clone();
                render_command_node(command_owned, session_state.clone(), cx)
            }
        }))
        .ml(indent)
}

/// 渲染文件夹节点
fn render_folder_node(
    session_state: Entity<SessionState>,
    group: SnippetGroup,
    is_expanded: bool,
    has_children: bool,
    child_groups: Vec<SnippetGroup>,
    child_commands: Vec<SnippetCommand>,
    level: usize,
    expanded: std::collections::HashSet<String>,
    config: SnippetsConfig,
    cx: &App,
) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let hover_bg = cx.theme().list_active;
    let primary = cx.theme().primary;

    let group_id = group.id.clone();
    let group_id_for_click = group_id.clone();
    let session_for_click = session_state.clone();

    div()
        .flex()
        .flex_col()
        .child(
            // 文件夹行
            div()
                .id(SharedString::from(format!("folder-{}", group_id)))
                .h(px(28.))
                .px_1()
                .flex()
                .items_center()
                .gap(px(2.))
                .rounded(px(4.))
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    session_for_click.update(cx, |state, _| {
                        state.toggle_snippets_group(&group_id_for_click);
                    });
                })
                // 展开/折叠箭头
                .child(
                    svg()
                        .path(if is_expanded {
                            icons::CHEVRON_DOWN
                        } else {
                            icons::CHEVRON_RIGHT
                        })
                        .size(px(12.))
                        .text_color(if has_children {
                            muted
                        } else {
                            muted.opacity(0.3)
                        }),
                )
                // 文件夹图标
                .child(
                    svg()
                        .path(if is_expanded {
                            icons::FOLDER_OPEN
                        } else {
                            icons::FOLDER
                        })
                        .size(px(14.))
                        .text_color(primary),
                )
                // 文件夹名称
                .child(
                    div()
                        .flex_1()
                        .text_xs()
                        .text_color(foreground)
                        .overflow_hidden()
                        .child(group.name.clone()),
                ),
        )
        // 展开时显示子项
        .when(is_expanded && has_children, |this| {
            this.child(render_tree_nodes_owned(
                session_state.clone(),
                config,
                expanded,
                child_groups,
                child_commands,
                level + 1,
                cx,
            ))
        })
}

/// 渲染树节点（拥有所有权版本）
fn render_tree_nodes_owned(
    session_state: Entity<SessionState>,
    config: SnippetsConfig,
    expanded: std::collections::HashSet<String>,
    groups: Vec<SnippetGroup>,
    commands: Vec<SnippetCommand>,
    level: usize,
    cx: &App,
) -> impl IntoElement {
    let indent = px((level * 16) as f32);

    div()
        .flex()
        .flex_col()
        .ml(indent)
        // 渲染文件夹节点
        .children(groups.into_iter().map(|group| {
            let is_expanded = expanded.contains(&group.id);
            let child_groups: Vec<SnippetGroup> = config
                .get_child_groups(Some(&group.id))
                .into_iter()
                .map(|g| g.clone())
                .collect();
            let child_commands: Vec<SnippetCommand> = config
                .get_commands_in_group(Some(&group.id))
                .into_iter()
                .map(|c| c.clone())
                .collect();
            let has_children = !child_groups.is_empty() || !child_commands.is_empty();

            render_folder_node(
                session_state.clone(),
                group,
                is_expanded,
                has_children,
                child_groups,
                child_commands,
                level,
                expanded.clone(),
                config.clone(),
                cx,
            )
        }))
        // 渲染命令节点
        .children({
            let session_state = session_state.clone();
            commands
                .into_iter()
                .map(move |command| render_command_node(command, session_state.clone(), cx))
        })
}

/// 渲染命令节点
fn render_command_node(
    command: SnippetCommand,
    session_state: Entity<SessionState>,
    cx: &App,
) -> impl IntoElement {
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let hover_bg = cx.theme().list_active;

    // 获取语言设置
    let lang = crate::services::storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or_default();

    let command_id = command.id.clone();
    let command_text = command.command.clone();
    let command_text_for_tooltip = command.command.clone();
    let command_text_for_execute = command.command.clone();
    let command_text_for_edit = command.command.clone();

    // 获取菜单文本
    let execute_label = crate::i18n::t(&lang, "snippets.context_menu.execute");
    let edit_label = crate::i18n::t(&lang, "snippets.context_menu.edit_in_box");

    // 获取 PTY channel 用于执行命令
    let pty_channel: Option<Arc<crate::ssh::session::TerminalChannel>> = session_state
        .read(cx)
        .active_tab()
        .and_then(|tab| {
            tab.active_terminal_id
                .as_ref()
                .and_then(|id| tab.terminals.iter().find(|t| &t.id == id))
        })
        .and_then(|inst| inst.pty_channel.clone());

    div()
        .id(SharedString::from(format!("cmd-{}", command_id)))
        .h(px(28.))
        .px_1()
        .flex()
        .items_center()
        .gap(px(2.))
        .rounded(px(4.))
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
            // 点击复制命令到剪贴板
            cx.write_to_clipboard(ClipboardItem::new_string(command_text.clone()));
        })
        // 添加 tooltip 显示完整命令
        .tooltip(move |window, cx| Tooltip::new(command_text_for_tooltip.clone()).build(window, cx))
        // 添加右键菜单
        .context_menu(move |menu, _window, _cx| {
            let cmd_for_execute = command_text_for_execute.clone();
            let cmd_for_edit = command_text_for_edit.clone();
            let pty_for_menu = pty_channel.clone();
            let session_for_menu = session_state.clone();

            menu
                // 在终端执行
                .item({
                    let execute_label = execute_label.to_string();
                    PopupMenuItem::element(move |_window, cx| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().foreground)
                            .child(execute_label.clone())
                    })
                    .on_click(move |_, _window, _cx| {
                        if let Some(channel) = &pty_for_menu {
                            let cmd = cmd_for_execute.clone();
                            let channel = Arc::clone(channel);
                            debug!("[ContextMenu] Executing command: {}", cmd);
                            let mut cmd_with_newline = cmd.into_bytes();
                            cmd_with_newline.push(0x0d); // CR
                            if let Err(e) = channel.queue_write(cmd_with_newline) {
                                tracing::error!("[ContextMenu] Failed to send command: {:?}", e);
                            }
                        }
                    })
                })
                // 在命令框编辑
                .item({
                    let edit_label = edit_label.to_string();
                    PopupMenuItem::element(move |_window, cx| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().foreground)
                            .child(edit_label.clone())
                    })
                    .on_click(move |_, window, cx| {
                        let cmd = cmd_for_edit.clone();
                        debug!("[ContextMenu] Edit in command box: {}", cmd);
                        session_for_menu.update(cx, |state, cx| {
                            state.set_command_input_text(cmd, window, cx);
                        });
                    })
                })
        })
        // 命令图标（紧贴左侧）
        .child(svg().path(icons::CODE).size(px(14.)).text_color(muted))
        // 命令名称
        .child(
            div()
                .flex_1()
                .text_xs()
                .text_color(foreground)
                .overflow_hidden()
                .child(command.name.clone()),
        )
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

    // 当前可用供应商（内置 + 自定义）与当前生效供应商
    let settings = crate::services::storage::load_settings().unwrap_or_default();
    let verified_refs = settings.ai.verified_provider_refs();
    let has_any = !verified_refs.is_empty();
    let active_ref = session_state.read(cx).resolve_ai_provider_ref(&tab_id);
    let active_resolved = active_ref.as_ref().and_then(|r| settings.ai.resolve(r));
    let has_messages = !chat.messages.is_empty() || chat.pending;

    // 输入框（如果存在）
    let input_entity = session_state.read(cx).get_ai_chat_input(&tab_id);

    // ===== 顶部：Zed 风格的窄面板工具栏 =====
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
            _ => svg()
                .path(icons::SPARKLES)
                .size(px(14.))
                .text_color(muted_foreground)
                .into_any_element(),
        };

        div()
            .h(px(36.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_between()
            .px_2()
            .gap_2()
            .border_b_1()
            .border_color(border)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .min_w_0()
                    .gap(px(7.))
                    .child(provider_icon)
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_sm()
                            .font_medium()
                            .text_color(foreground)
                            .child(header_title),
                    ),
            )
            .child(
                div()
                    .id("ai-chat-new-thread")
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(24.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(|s| s.bg(muted_foreground.opacity(0.12)))
                    .tooltip(move |window, cx| Tooltip::new(new_thread_tooltip).build(window, cx))
                    .child(
                        svg()
                            .path(icons::PLUS)
                            .size(px(14.))
                            .text_color(muted_foreground),
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
