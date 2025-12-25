// 核心方法：标签页、侧边栏、快捷命令管理

use super::{
    MonitorState, SessionState, SessionStatus, SessionTab, SidebarPanel, TerminalInstance,
};
use tracing::info;

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
            pty_initialized: false,
            last_sent_pty_size: None,
            pty_error: None,
        };
        let first_terminal_id = first_terminal.id.clone();

        let tab = SessionTab {
            id: tab_id.clone(),
            server_id,
            server_label,
            status: SessionStatus::Connecting,
            terminals: vec![first_terminal],
            active_terminal_id: Some(first_terminal_id),
            terminal_counter: 1,
            monitor_state: MonitorState::empty(),
            sftp_state: None,
            active_transfers: Vec::new(),
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
            self.tabs.remove(pos);
            // 如果关闭的是当前活动标签，切换到下一个
            if self.active_tab_id.as_deref() == Some(tab_id) {
                self.active_tab_id = self.tabs.first().map(|t| t.id.clone());
            }

            // 停止并移除 MonitorService（Drop 会自动调用 stop）
            if let Ok(mut services) = self.monitor_services.lock() {
                if services.remove(tab_id).is_some() {
                    info!("[Monitor] Service removed for closed tab {}", tab_id);
                }
            }

            // 移除 SFTP 文件列表视图
            if self.sftp_file_list_views.remove(tab_id).is_some() {
                info!("[SFTP] FileListView removed for closed tab {}", tab_id);
            }

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
                info!("[FileWatcher] Destroyed (no more watched files and no active sessions)");
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

    /// 设置当前激活的侧边栏面板
    pub fn set_sidebar_panel(&mut self, panel: SidebarPanel) {
        self.active_sidebar_panel = panel;
        // 如果 sidebar 折叠了，自动展开
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
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
}
