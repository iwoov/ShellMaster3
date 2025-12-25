// UI 状态管理方法：对话框、输入框、焦点句柄等

use super::SessionState;
use crate::components::monitor::DetailDialogState;
use crate::components::sftp::{FileListView, PathBarEvent, PathBarState};
use crate::services::monitor::{MonitorEvent, MonitorService, MonitorSettings};
use gpui::prelude::*;
use gpui::{Entity, FocusHandle};
use gpui_component::input::InputState;
use tracing::info;

impl SessionState {
    /// 确保 Monitor 详情弹窗状态已创建
    pub fn ensure_monitor_detail_dialog(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<DetailDialogState> {
        if self.monitor_detail_dialog.is_none() {
            self.monitor_detail_dialog = Some(cx.new(|_| DetailDialogState::default()));
        }
        self.monitor_detail_dialog.clone().unwrap()
    }

    /// 确保 SFTP 文件列表视图已创建
    pub fn ensure_sftp_file_list_view(
        &mut self,
        tab_id: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<FileListView> {
        if !self.sftp_file_list_views.contains_key(tab_id) {
            let view = cx.new(|cx| FileListView::new(window, cx));

            // 订阅 TableEvent 以处理双击事件
            let tab_id_for_table = tab_id.to_string();
            let view_for_table = view.clone();
            cx.subscribe_in(
                &view,
                window,
                move |this, _emitter, event: &gpui_component::table::TableEvent, _window, cx| {
                    use gpui_component::table::TableEvent;
                    match event {
                        TableEvent::DoubleClickedRow(row_ix) => {
                            // 获取文件路径并触发打开事件
                            if let Some(path) = view_for_table.read(cx).get_file_path(*row_ix, cx) {
                                let tab_id = tab_id_for_table.clone();
                                // 直接在 this 上操作，避免嵌套 update
                                this.sftp_open(&tab_id, path, cx);
                            }
                        }
                        TableEvent::SelectRow(_row_ix) => {
                            // TODO: 处理选择事件
                        }
                        _ => {}
                    }
                },
            )
            .detach();

            // 订阅 FileListContextMenuEvent 以处理右键菜单事件
            let tab_id_for_context = tab_id.to_string();
            let view_for_context = view.clone();
            cx.subscribe_in(
                &view,
                window,
                move |this,
                      _emitter,
                      event: &crate::components::sftp::FileListContextMenuEvent,
                      window,
                      cx| {
                    use crate::components::sftp::FileListContextMenuEvent;
                    let tab_id = tab_id_for_context.clone();
                    match event {
                        FileListContextMenuEvent::Delete(path) => {
                            this.sftp_delete(&tab_id, path.clone(), cx);
                        }
                        FileListContextMenuEvent::Refresh => {
                            this.sftp_refresh(&tab_id, cx);
                        }
                        FileListContextMenuEvent::OpenFolder(path) => {
                            this.sftp_navigate_to(&tab_id, path.clone(), cx);
                        }
                        FileListContextMenuEvent::Rename(path) => {
                            // 开始内联重命名
                            view_for_context.update(cx, |view, cx| {
                                view.start_rename(path.clone(), window, cx);
                            });
                        }
                        FileListContextMenuEvent::RenameConfirmed { old_path, new_name } => {
                            this.sftp_rename(&tab_id, old_path.clone(), new_name.clone(), cx);
                        }
                        FileListContextMenuEvent::Download(path) => {
                            // 下载单个文件 - 需要获取文件信息
                            if let Some(tab) = this.tabs.iter().find(|t| t.id == tab_id) {
                                if let Some(sftp_state) = &tab.sftp_state {
                                    if let Some(file) =
                                        sftp_state.file_list.iter().find(|e| e.path == *path)
                                    {
                                        this.sftp_download_file(
                                            &tab_id,
                                            file.path.clone(),
                                            file.name.clone(),
                                            file.size,
                                            cx,
                                        );
                                    }
                                }
                            }
                        }
                        FileListContextMenuEvent::DownloadFolder(path) => {
                            // 下载文件夹
                            this.sftp_download_folder_with_picker(&tab_id, path.clone(), cx);
                        }
                        FileListContextMenuEvent::UploadFile => {
                            // 上传单个文件到当前目录
                            if let Some(current_path) = this
                                .tabs
                                .iter()
                                .find(|t| t.id == tab_id)
                                .and_then(|t| t.sftp_state.as_ref())
                                .map(|s| s.current_path.clone())
                            {
                                this.sftp_upload_file(&tab_id, current_path, cx);
                            }
                        }
                        FileListContextMenuEvent::UploadFolder => {
                            // 上传文件夹到当前目录
                            if let Some(current_path) = this
                                .tabs
                                .iter()
                                .find(|t| t.id == tab_id)
                                .and_then(|t| t.sftp_state.as_ref())
                                .map(|s| s.current_path.clone())
                            {
                                this.sftp_upload_folder_with_picker(&tab_id, current_path, cx);
                            }
                        }
                        FileListContextMenuEvent::DropFiles { paths, target_dir } => {
                            // 拖放上传 - 根据路径类型调用不同的上传方法
                            for path in paths {
                                if path.is_dir() {
                                    // 上传文件夹
                                    this.sftp_upload_folder(
                                        &tab_id,
                                        path.clone(),
                                        target_dir.clone(),
                                        cx,
                                    );
                                } else if path.is_file() {
                                    // 上传单个文件
                                    this.sftp_upload_file_direct(
                                        &tab_id,
                                        path.clone(),
                                        target_dir.clone(),
                                        cx,
                                    );
                                }
                            }
                        }
                        FileListContextMenuEvent::NewFolder => {
                            // 新建文件夹
                            this.sftp_open_new_folder_dialog(&tab_id, cx);
                        }
                        FileListContextMenuEvent::NewFile => {
                            // 新建文件
                            this.sftp_open_new_file_dialog(&tab_id, cx);
                        }
                        FileListContextMenuEvent::OpenInTerminal(path) => {
                            // 在终端中打开目录
                            this.sftp_open_in_terminal(&tab_id, path.clone(), cx);
                        }
                        FileListContextMenuEvent::Properties(path) => {
                            // 显示属性对话框
                            this.sftp_open_properties_dialog(&tab_id, path.clone(), cx);
                        }
                        FileListContextMenuEvent::EditFile(path) => {
                            // 编辑文件（外置编辑器）
                            this.sftp_edit_file(&tab_id, path.clone(), cx);
                        }
                        _ => {
                            // 其他事件待实现 (CopyName, CopyPath, etc.)
                        }
                    }
                },
            )
            .detach();

            self.sftp_file_list_views.insert(tab_id.to_string(), view);
        }
        self.sftp_file_list_views.get(tab_id).unwrap().clone()
    }

    /// 获取 SFTP 文件列表视图（如果存在）
    pub fn get_sftp_file_list_view(&self, tab_id: &str) -> Option<Entity<FileListView>> {
        self.sftp_file_list_views.get(tab_id).cloned()
    }

    /// 确保 SFTP 路径栏状态已创建
    pub fn ensure_sftp_path_bar_state(
        &mut self,
        tab_id: &str,
        session_state: Entity<SessionState>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<PathBarState> {
        if !self.sftp_path_bar_states.contains_key(tab_id) {
            let tab_id_for_event = tab_id.to_string();
            let view = cx.new(|cx| {
                PathBarState::new(window, cx, move |event, cx| match event {
                    PathBarEvent::Navigate(path) => {
                        session_state.update(cx, |state, cx| {
                            state.sftp_navigate_to(&tab_id_for_event, path, cx);
                        });
                    }
                })
            });
            self.sftp_path_bar_states.insert(tab_id.to_string(), view);
        }
        self.sftp_path_bar_states.get(tab_id).unwrap().clone()
    }

    /// 获取 SFTP 路径栏状态（如果存在）
    pub fn get_sftp_path_bar_state(&self, tab_id: &str) -> Option<Entity<PathBarState>> {
        self.sftp_path_bar_states.get(tab_id).cloned()
    }

    /// 确保命令输入框已创建，并更新占位符为当前语言
    pub fn ensure_command_input_created(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let lang = crate::services::storage::load_settings()
            .map(|s| s.theme.language)
            .unwrap_or(crate::models::settings::Language::Chinese);
        let placeholder = crate::i18n::t(&lang, "session.terminal.command_placeholder");

        if self.command_input.is_none() {
            // 首次创建
            self.command_input = Some(cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .auto_grow(1, 20) // 1-20 行自动增长，支持多行输入
            }));
        } else {
            // 更新占位符（语言可能已变化）
            if let Some(input) = &self.command_input {
                input.update(cx, |state, cx| {
                    state.set_placeholder(placeholder, window, cx);
                });
            }
        }
    }

    /// 设置命令输入框的文本内容
    pub fn set_command_input_text(
        &self,
        text: String,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(input) = &self.command_input {
            input.update(cx, |state, cx| {
                state.set_value(text, window, cx);
            });
        }
    }

    /// 确保终端焦点句柄已创建
    pub fn ensure_terminal_focus_handle_created(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> FocusHandle {
        if self.terminal_focus_handle.is_none() {
            self.terminal_focus_handle = Some(cx.focus_handle());
        }
        self.terminal_focus_handle.clone().unwrap()
    }

    /// 获取终端焦点句柄（如果存在）
    pub fn get_terminal_focus_handle(&self) -> Option<FocusHandle> {
        self.terminal_focus_handle.clone()
    }

    /// 启动 Monitor 服务
    /// 在 SSH 连接成功后调用，开始收集服务器监控数据
    pub fn start_monitor_service(&self, tab_id: String, cx: &mut gpui::Context<Self>) {
        info!("[Monitor] Starting monitor service for tab {}", tab_id);

        // 获取 SSH session
        let ssh_manager = crate::ssh::manager::SshManager::global();
        let Some(ssh_session) = ssh_manager.get_session(&tab_id) else {
            tracing::error!("[Monitor] No SSH session found for tab {}", tab_id);
            return;
        };

        // 使用默认监控设置
        let settings = MonitorSettings::default();

        // 创建 MonitorService（需要使用 SSH manager 的 runtime）
        let (service, mut receiver) =
            MonitorService::new(tab_id.clone(), ssh_session, settings, ssh_manager.runtime());

        // 存储 service
        if let Ok(mut services) = self.monitor_services.lock() {
            services.insert(tab_id.clone(), service);
        }

        // 在 GPUI 异步上下文中处理事件
        let session_state = cx.entity().clone();
        let tab_id_for_task = tab_id.clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                info!("[Monitor] Event loop started for tab {}", tab_id_for_task);

                // 处理监控事件
                loop {
                    match receiver.recv().await {
                        Some(event) => {
                            let tab_id_clone = tab_id_for_task.clone();
                            let result = async_cx.update(|cx| {
                                session_state.update(cx, |state, cx| {
                                    if let Some(tab) =
                                        state.tabs.iter_mut().find(|t| t.id == tab_id_clone)
                                    {
                                        match event {
                                            MonitorEvent::SystemInfo(info) => {
                                                tab.monitor_state.update_system_info(info);
                                            }
                                            MonitorEvent::LoadInfo(info) => {
                                                tab.monitor_state.update_load_info(info);
                                            }
                                            MonitorEvent::NetworkInfo(info) => {
                                                tab.monitor_state.update_network_info(info);
                                            }
                                            MonitorEvent::DiskInfo(info) => {
                                                tab.monitor_state.update_disk_info(info);
                                            }
                                            MonitorEvent::Error(e) => {
                                                tracing::error!("[Monitor] Error: {}", e);
                                            }
                                        }
                                        cx.notify();
                                    }
                                });
                            });
                            if result.is_err() {
                                // Entity 已销毁，退出循环
                                break;
                            }
                        }
                        None => {
                            // Channel 关闭，退出循环
                            break;
                        }
                    }
                }

                info!("[Monitor] Event loop ended for tab {}", tab_id_for_task);
                Some(())
            })
            .detach();
    }
}
