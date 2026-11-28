// 核心方法：标签页、侧边栏、快捷命令管理

use super::{
    CenterViewMode, MonitorState, SessionState, SessionStatus, SessionTab, SidebarPanel,
    TerminalInstance, TerminalPtyState,
};
use tracing::{debug, info};

impl SessionState {
    /// 添加新的会话标签（插入到最前面）
    pub fn add_tab(&mut self, server_id: String, server_label: String) -> String {
        let tab_id = uuid::Uuid::new_v4().to_string();

        // 创建第一个终端实例
        let first_terminal = TerminalInstance {
            id: uuid::Uuid::new_v4().to_string(),
            index: 1,
            terminal: None,
            pty_channel: None,
            pty_state: TerminalPtyState::NotStarted,
            last_sent_pty_size: None,
            title: None,
        };
        let first_terminal_id = first_terminal.id.clone();

        let tab = SessionTab {
            id: tab_id.clone(),
            server_id,
            server_label,
            status: SessionStatus::Connecting,
            server_data: None,
            disconnect_reason: None,
            reconnected_at: None,
            center_view: CenterViewMode::Terminal,
            terminals: vec![first_terminal],
            active_terminal_id: Some(first_terminal_id),
            terminal_counter: 1,
            editor_tabs: Vec::new(),
            active_editor_tab_id: None,
            monitor_state: MonitorState::empty(),
            sftp_state: None,
            active_transfers: Vec::new(),
            services_started: false,
            ai_chat: super::AiChatState::default(),
            port_forwards: Vec::new(),
        };
        // 新标签插入到最前面
        self.tabs.insert(0, tab);
        self.active_tab_id = Some(tab_id.clone());
        // 切换到会话视图
        self.show_home = false;
        // 确保默认面板（快捷命令）的数据已加载
        self.load_snippets_config();
        tab_id
    }

    /// 关闭标签
    pub fn close_tab(&mut self, tab_id: &str) {
        if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
            let removed = self.tabs.remove(pos);

            // 释放该会话所有内置编辑器标签的事件订阅
            for ed in &removed.editor_tabs {
                self.sftp_editor_subscriptions.remove(&ed.id);
            }
            if self
                .pending_close_editor_tab
                .as_ref()
                .is_some_and(|(sid, _)| sid == tab_id)
            {
                self.pending_close_editor_tab = None;
            }
            if self
                .pending_reload_editor_tab
                .as_ref()
                .is_some_and(|(sid, _)| sid == tab_id)
            {
                self.pending_reload_editor_tab = None;
            }
            if self.pending_close_session_tab.as_deref() == Some(tab_id) {
                self.pending_close_session_tab = None;
            }
            if self
                .sftp_delete_confirm
                .as_ref()
                .is_some_and(|confirm| confirm.tab_id == tab_id)
            {
                self.sftp_delete_confirm = None;
            }
            if self
                .pending_terminal_paste
                .as_ref()
                .is_some_and(|paste| paste.tab_id == tab_id)
            {
                self.pending_terminal_paste = None;
            }

            // 如果关闭的是当前活动标签，切换到下一个
            if self.active_tab_id.as_deref() == Some(tab_id) {
                self.active_tab_id = self.tabs.first().map(|t| t.id.clone());
            }

            // 停止并移除 MonitorService（Drop 会自动调用 stop）
            if let Ok(mut services) = self.monitor_services.lock() {
                if services.remove(tab_id).is_some() {
                    debug!("[Monitor] Service removed for closed tab {}", tab_id);
                }
            }

            // 移除 SFTP 文件列表视图与路径栏状态
            if self.sftp_file_list_views.remove(tab_id).is_some() {
                debug!("[SFTP] FileListView removed for closed tab {}", tab_id);
            }
            self.sftp_path_bar_states.remove(tab_id);

            // 停止并清理 AI Agent 运行期资源（取消任务、白名单、输入框）
            self.cleanup_ai_agent_for_tab(tab_id);

            // 清理该 session 的文件监控和临时文件
            if let Some(watcher) = &self.file_watcher {
                if let Ok(mut watcher) = watcher.lock() {
                    // 移除该 session 的所有监控文件
                    let _ = watcher.unwatch_session(tab_id);
                }
            }

            // 删除该 session 的临时文件
            crate::services::sftp::cleanup_temp_files_for_session(tab_id);

            // 如果没有更多监控的文件，销毁 FileWatcher 释放资源
            let should_destroy_watcher = self
                .file_watcher
                .as_ref()
                .map(|w| w.lock().map(|w| w.is_empty()).unwrap_or(true))
                .unwrap_or(false);

            if should_destroy_watcher && self.tabs.is_empty() {
                self.file_watcher = None;
                self.file_watch_receiver = None;
                debug!("[FileWatcher] Destroyed (no more watched files and no active sessions)");
            }

            // 关闭 SSH 会话
            let ssh_manager = crate::ssh::manager::SshManager::global();
            ssh_manager.close_session(tab_id);
            info!("[Session] SSH session closed for tab {}", tab_id);
        }
    }

    /// 激活指定标签
    pub fn activate_tab(&mut self, tab_id: &str) {
        if self.tabs.iter().any(|t| t.id == tab_id) {
            self.active_tab_id = Some(tab_id.to_string());
        }
    }

    /// 更新标签状态
    pub fn update_tab_status(&mut self, tab_id: &str, status: SessionStatus) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.status = status;
        }
    }

    /// 获取当前活动标签
    pub fn active_tab(&self) -> Option<&SessionTab> {
        self.active_tab_id
            .as_ref()
            .and_then(|id| self.tabs.iter().find(|t| &t.id == id))
    }

    /// 检查是否有任何会话标签
    pub fn has_sessions(&self) -> bool {
        !self.tabs.is_empty()
    }

    /// 切换 Sidebar 折叠状态
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_collapsed = !self.sidebar_collapsed;
    }

    /// 按窗口宽度算出会话视图各栏的实际宽度
    pub fn session_columns(
        &self,
        window_width: gpui_kit::Pixels,
    ) -> crate::pages::session::SessionColumns {
        crate::pages::session::fit_columns(
            window_width,
            self.left_column_width,
            self.sidebar_width,
            !self.sidebar_collapsed,
        )
    }

    /// `key` 对应的条目是否刚被复制过（显示对勾）
    pub fn is_copied(&self, key: &str) -> bool {
        self.copied_key.as_deref() == Some(key)
    }

    /// 复制文字到剪贴板，并让 `key` 对应的复制按钮短暂显示为对勾
    pub fn copy_with_feedback(
        session_state: &gpui_kit::Entity<SessionState>,
        key: String,
        text: String,
        cx: &mut gpui_kit::App,
    ) {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(text));

        let token = session_state.read(cx).copy_feedback_token.wrapping_add(1);
        session_state.update(cx, |state, cx| {
            state.copied_key = Some(key.clone());
            state.copy_feedback_token = token;
            cx.notify();
        });

        let session_state = session_state.clone();
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1200))
                .await;
            let _ = cx.update(|cx| {
                session_state.update(cx, |state, cx| {
                    if state.copy_feedback_token == token && state.is_copied(&key) {
                        state.copied_key = None;
                        cx.notify();
                    }
                });
            });
        })
        .detach();
    }

    /// 设置当前激活的侧边栏面板
    pub fn set_sidebar_panel(&mut self, panel: SidebarPanel) {
        self.active_sidebar_panel = panel;
        // 如果 sidebar 折叠了，自动展开
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
    }

    /// 持久化当前右侧侧边栏的折叠状态与选中面板
    pub fn persist_sidebar_state(&self) {
        let mut ui_state = crate::services::storage::load_ui_state().unwrap_or_default();
        ui_state.session_sidebar_collapsed = self.sidebar_collapsed;
        ui_state.session_sidebar_panel = Some(self.active_sidebar_panel.as_key().to_string());
        if let Err(error) = crate::services::storage::save_ui_state(&ui_state) {
            tracing::warn!("[UI] Failed to persist sidebar state: {}", error);
        }
    }

    /// 切换快捷命令组的展开状态
    pub fn toggle_snippets_group(&mut self, group_id: &str) {
        if self.snippets_expanded.contains(group_id) {
            self.snippets_expanded.remove(group_id);
        } else {
            self.snippets_expanded.insert(group_id.to_string());
        }
    }

    /// 加载快捷命令配置（如果尚未加载）
    pub fn load_snippets_config(&mut self) {
        if self.snippets_config.is_none() {
            self.snippets_config = crate::services::storage::load_snippets().ok();
        }
    }

    /// 刷新快捷命令配置
    pub fn refresh_snippets_config(&mut self) {
        self.snippets_config = crate::services::storage::load_snippets().ok();
    }

    /// 确保端口转发面板的表单输入框已创建
    pub fn ensure_port_forward_inputs(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        use gpui_kit::component::input::InputState;
        use gpui_kit::AppContext;

        if self.forward_bind_host_input.is_none() {
            self.forward_bind_host_input = Some(cx.new(|cx| {
                let mut input = InputState::new(window, cx).placeholder("127.0.0.1");
                input.set_value("127.0.0.1", window, cx);
                input
            }));
        }
        if self.forward_bind_port_input.is_none() {
            self.forward_bind_port_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("5432")));
        }
        if self.forward_target_host_input.is_none() {
            self.forward_target_host_input = Some(cx.new(|cx| {
                let mut input = InputState::new(window, cx).placeholder("127.0.0.1");
                input.set_value("127.0.0.1", window, cx);
                input
            }));
        }
        if self.forward_target_port_input.is_none() {
            self.forward_target_port_input =
                Some(cx.new(|cx| InputState::new(window, cx).placeholder("5432")));
        }
    }

    /// 按当前表单参数启动一条 SSH 端口转发隧道
    pub fn start_port_forward_from_form(
        &mut self,
        tab_id: &str,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        use crate::ssh::forwarding::{ForwardingKind, PortForwardRule};

        let lang = crate::services::storage::cached_settings()
            .theme
            .language
            .clone();
        let read_input = |slot: &Option<gpui_kit::Entity<gpui_kit::component::input::InputState>>| {
            slot.as_ref()
                .map(|i| i.read(cx).value().trim().to_string())
                .unwrap_or_default()
        };

        let kind = self.forward_kind;
        let bind_host_raw = read_input(&self.forward_bind_host_input);
        let bind_host = if bind_host_raw.is_empty() {
            "127.0.0.1".to_string()
        } else {
            bind_host_raw
        };
        let bind_port: u16 = match read_input(&self.forward_bind_port_input).parse::<u16>() {
            Ok(p) if p > 0 => p,
            _ => {
                self.forward_error =
                    Some(crate::i18n::t(&lang, "port_forward.invalid_port").to_string());
                cx.notify();
                return;
            }
        };

        let (target_host, target_port) = if kind == ForwardingKind::Dynamic {
            (String::new(), 0u16)
        } else {
            let th_raw = read_input(&self.forward_target_host_input);
            let th = if th_raw.is_empty() {
                "127.0.0.1".to_string()
            } else {
                th_raw
            };
            let tp: u16 = match read_input(&self.forward_target_port_input).parse::<u16>() {
                Ok(p) if p > 0 => p,
                _ => {
                    self.forward_error =
                        Some(crate::i18n::t(&lang, "port_forward.invalid_target").to_string());
                    cx.notify();
                    return;
                }
            };
            (th, tp)
        };

        let ssh_manager = crate::ssh::manager::SshManager::global();
        let Some(ssh_session) = ssh_manager.get_session(tab_id) else {
            self.forward_error =
                Some(crate::i18n::t(&lang, "port_forward.not_connected").to_string());
            cx.notify();
            return;
        };

        self.forward_error = None;
        let rule = PortForwardRule {
            id: uuid::Uuid::new_v4().to_string(),
            kind,
            bind_host,
            bind_port,
            target_host,
            target_port,
        };

        let (tx, rx) = tokio::sync::oneshot::channel();
        ssh_manager.runtime().spawn(async move {
            let res = ssh_session.start_port_forward(rule).await;
            let _ = tx.send(res);
        });

        let session_state = cx.entity().clone();
        let tab_id_owned = tab_id.to_string();
        cx.to_async()
            .spawn(async move |async_cx| {
                if let Ok(result) = rx.await {
                    let _ = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            match result {
                                Ok(effective_rule) => {
                                    state.forward_error = None;
                                    if let Some(tab) =
                                        state.tabs.iter_mut().find(|t| t.id == tab_id_owned)
                                    {
                                        tab.port_forwards.push(effective_rule);
                                    }
                                }
                                Err(err) => {
                                    state.forward_error = Some(err);
                                }
                            }
                            cx.notify();
                        });
                    });
                }
                Some(())
            })
            .detach();
    }

    /// 请求多行/大段粘贴二次确认
    pub fn request_terminal_paste_confirm(
        &mut self,
        tab_id: &str,
        text: String,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let line_count = text.lines().count().max(1);
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        self.pending_terminal_paste = Some(super::PendingTerminalPaste {
            tab_id: tab_id.to_string(),
            text,
            line_count,
            focus_handle,
        });
        cx.notify();
    }

    /// 确认执行待粘贴的多行文本到当前活动终端
    pub fn confirm_terminal_paste(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let Some(pending) = self.pending_terminal_paste.take() else {
            return;
        };
        if let Some(tab) = self.tabs.iter().find(|t| t.id == pending.tab_id) {
            if let Some(active_id) = &tab.active_terminal_id {
                if let Some(inst) = tab.terminals.iter().find(|t| &t.id == active_id) {
                    if let Some(channel) = &inst.pty_channel {
                        let mode = inst
                            .terminal
                            .as_ref()
                            .map(|t| t.read(cx).term_mode())
                            .unwrap_or_else(alacritty_terminal::term::TermMode::empty);
                        let bytes = crate::terminal::paste_to_bytes(&pending.text, mode);
                        if let Some(terminal) = &inst.terminal {
                            terminal.update(cx, |t, _| t.show_cursor());
                        }
                        let _ = channel.queue_write(bytes);
                    }
                }
            }
        }
        cx.notify();
    }

    /// 取消待确认的多行粘贴
    pub fn cancel_terminal_paste(&mut self, cx: &mut gpui_kit::Context<Self>) {
        if self.pending_terminal_paste.take().is_some() {
            cx.notify();
        }
    }

    /// 停止并移除指定 SSH 会话中的一条端口转发隧道
    pub fn stop_port_forward(
        &mut self,
        tab_id: &str,
        rule_id: &str,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            tab.port_forwards.retain(|r| r.id != rule_id);
        }
        cx.notify();

        let ssh_manager = crate::ssh::manager::SshManager::global();
        if let Some(ssh_session) = ssh_manager.get_session(tab_id) {
            let id_owned = rule_id.to_string();
            ssh_manager.runtime().spawn(async move {
                ssh_session.stop_port_forward(&id_owned).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SessionState, SidebarPanel};
    use gpui_kit::px;

    #[test]
    fn right_sidebar_is_collapsed_by_default() {
        let state = SessionState::default();
        assert!(state.sidebar_collapsed);
        let columns = state.session_columns(px(1200.));
        assert_eq!(columns.sidebar, px(0.));
    }

    #[test]
    fn sidebar_toggle_and_panel_switch_update_collapsed_state() {
        let mut state = SessionState::default();
        assert!(state.sidebar_collapsed);

        state.set_sidebar_panel(SidebarPanel::Snippets);
        assert!(!state.sidebar_collapsed);
        assert_eq!(state.active_sidebar_panel, SidebarPanel::Snippets);

        state.toggle_sidebar();
        assert!(state.sidebar_collapsed);
    }

    #[test]
    fn from_ui_state_restores_sidebar_collapsed_and_panel() {
        let ui_state = crate::services::storage::UiState {
            session_sidebar_collapsed: false,
            session_sidebar_panel: Some("snippets".to_string()),
        };
        let state = SessionState::from_ui_state(&ui_state);
        assert!(!state.sidebar_collapsed);
        assert_eq!(state.active_sidebar_panel, SidebarPanel::Snippets);

        let unknown_panel = crate::services::storage::UiState {
            session_sidebar_collapsed: true,
            session_sidebar_panel: Some("unknown".to_string()),
        };
        let fallback = SessionState::from_ui_state(&unknown_panel);
        assert!(fallback.sidebar_collapsed);
        assert_eq!(fallback.active_sidebar_panel, SidebarPanel::AiChat);
    }
}
