// 终端管理方法

use super::{SessionState, SessionStatus, TerminalInstance};
use gpui::prelude::*;
use tracing::{debug, error, info};

impl SessionState {
    /// 初始化终端（在 UI 挂载并获取尺寸后调用）
    /// 只初始化当前激活的终端实例
    pub fn initialize_terminal(
        &mut self,
        tab_id: &str,
        area_width: f32,
        area_height: f32,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        // 先确保终端焦点句柄已创建（在任何可变借用之前）
        self.ensure_terminal_focus_handle_created(cx);

        // 查找 tab 并检查状态
        let tab_id_owned = tab_id.to_string();

        // 获取需要初始化的终端实例 ID 和现有终端状态（如果有）
        let (terminal_instance_id, existing_terminal) = {
            let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) else {
                return;
            };
            if tab.status != SessionStatus::Connected {
                return;
            }
            let Some(active_id) = &tab.active_terminal_id else {
                return;
            };
            let Some(instance) = tab.terminals.iter().find(|t| &t.id == active_id) else {
                return;
            };
            if instance.pty_initialized {
                return;
            }
            // 获取现有终端状态（用于重连时保留历史）
            (active_id.clone(), instance.terminal.clone())
        };

        info!(
            "[Terminal] Initializing PTY for tab {} terminal {}{}",
            tab_id,
            terminal_instance_id,
            if existing_terminal.is_some() { " (reconnecting, preserving history)" } else { "" }
        );
        debug!(
            "[Terminal] Area size: {}x{} pixels",
            area_width, area_height
        );

        // 创建终端设置
        let settings = crate::services::storage::load_settings()
            .unwrap_or_default()
            .terminal;

        // 重用现有 TerminalState（保留历史）或创建新的
        let terminal_state = if let Some(existing) = existing_terminal {
            info!("[Terminal] Reusing existing terminal state (preserving scrollback history)");
            existing
        } else {
            cx.new(|_cx| crate::terminal::TerminalState::new(settings.clone()))
        };

        // 计算终端尺寸
        let (cols, rows, cell_width, line_height) = crate::terminal::calculate_terminal_size(
            area_width,
            area_height,
            &settings,
            window,
            cx,
        );
        debug!(
            "[Terminal] Calculated size: {}x{} (cols x rows)",
            cols, rows
        );

        // 初始化终端尺寸
        terminal_state.update(cx, |t, _| {
            t.resize(area_width, area_height, cell_width, line_height);
        });

        // 存储终端状态到对应的终端实例
        let terminal_instance_id_for_store = terminal_instance_id.clone();
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id_owned) {
            if let Some(instance) = tab
                .terminals
                .iter_mut()
                .find(|t| t.id == terminal_instance_id_for_store)
            {
                instance.terminal = Some(terminal_state.clone());
                instance.pty_initialized = true;
                instance.last_sent_pty_size = Some((cols, rows));
            }
        }

        // 启动光标闪烁定时器 (500ms 间隔)
        let terminal_for_blink = terminal_state.clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                loop {
                    // 等待 500ms
                    async_cx
                        .background_executor()
                        .timer(std::time::Duration::from_millis(500))
                        .await;

                    // 切换光标可见性
                    let result = async_cx.update(|cx| {
                        terminal_for_blink.update(cx, |t, cx| {
                            t.toggle_cursor_visibility();
                            cx.notify();
                        });
                    });

                    // 如果更新失败（例如终端已关闭），退出循环
                    if result.is_err() {
                        break;
                    }
                }
            })
            .detach();

        // 创建 PTY 请求（使用已计算的 cols/rows）
        let pty_request = crate::terminal::create_pty_request(cols, rows, area_width, area_height);

        // 异步创建 PTY channel (使用 App::spawn)
        let terminal_for_task = terminal_state.clone();
        let session_state_for_task = cx.entity().clone();
        let session_id = tab_id_owned.clone();
        let terminal_id_for_task = terminal_instance_id.clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                // 获取 SSH session
                let session =
                    match crate::ssh::manager::SshManager::global().get_session(&session_id) {
                        Some(s) => s,
                        None => {
                            error!("[Terminal] No SSH session found for {}", session_id);
                            return;
                        }
                    };

                // 打开终端通道
                match session.open_terminal(pty_request).await {
                    Ok(channel) => {
                        let channel = std::sync::Arc::new(channel);
                        info!(
                            "[Terminal] PTY channel created for {} terminal {}",
                            session_id, terminal_id_for_task
                        );

                        // 存储 channel 到终端实例
                        let channel_for_state = channel.clone();
                        let session_id_for_state = session_id.clone();
                        let terminal_id_for_state = terminal_id_for_task.clone();
                        let _ = async_cx.update(|cx| {
                            session_state_for_task.update(cx, |state, _| {
                                if let Some(tab) =
                                    state.tabs.iter_mut().find(|t| t.id == session_id_for_state)
                                {
                                    if let Some(instance) = tab
                                        .terminals
                                        .iter_mut()
                                        .find(|t| t.id == terminal_id_for_state)
                                    {
                                        instance.pty_channel = Some(channel_for_state);
                                    }
                                }
                            });
                        });

                        // 启动 PTY 读取循环
                        let session_state_for_reader = session_state_for_task.clone();
                        let session_id_for_reader = session_id.clone();
                        let terminal_id_for_reader = terminal_id_for_task.clone();
                        let _ = async_cx.update(|cx| {
                            crate::terminal::start_pty_reader(
                                channel,
                                terminal_for_task,
                                session_state_for_reader,
                                session_id_for_reader,
                                terminal_id_for_reader,
                                cx,
                            );
                        });

                        debug!(
                            "[Terminal] PTY reader started for {} terminal {}",
                            session_id, terminal_id_for_task
                        );
                    }
                    Err(e) => {
                        error!("[Terminal] Failed to open PTY: {:?}", e);
                        // 记录错误到终端实例
                        let session_id_for_err = session_id.clone();
                        let terminal_id_for_err = terminal_id_for_task.clone();
                        let error_msg = format!("{:?}", e);
                        let _ = async_cx.update(|cx| {
                            session_state_for_task.update(cx, |state, _| {
                                if let Some(tab) =
                                    state.tabs.iter_mut().find(|t| t.id == session_id_for_err)
                                {
                                    if let Some(instance) = tab
                                        .terminals
                                        .iter_mut()
                                        .find(|t| t.id == terminal_id_for_err)
                                    {
                                        instance.pty_error = Some(error_msg);
                                    }
                                }
                            });
                        });
                    }
                }
            })
            .detach();

        cx.notify();
    }

    /// 将本地终端尺寸与远端 PTY 尺寸同步到给定像素区域（用于窗口/布局变化时的自动 resize）
    /// 只同步当前激活的终端实例
    /// 注意：初始化由单独的机制触发，此方法仅处理 resize
    pub fn sync_terminal_size(
        &mut self,
        tab_id: &str,
        area_width: f32,
        area_height: f32,
        cx: &mut gpui::Context<Self>,
    ) {
        use gpui::prelude::*;

        if area_width <= 0.0 || area_height <= 0.0 {
            return;
        }

        // 获取当前激活的终端实例信息
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };

        if tab.status != SessionStatus::Connected {
            return;
        }

        let Some(active_id) = tab.active_terminal_id.clone() else {
            return;
        };

        let Some(instance) = tab.terminals.iter_mut().find(|t| t.id == active_id) else {
            return;
        };

        // 如果未初始化，跳过（初始化由其他机制触发）
        if !instance.pty_initialized {
            return;
        }

        let (Some(terminal), channel, last_sent) = (
            instance.terminal.clone(),
            instance.pty_channel.clone(),
            instance.last_sent_pty_size,
        ) else {
            return;
        };

        let (cell_width, line_height) = {
            let size = terminal.read(cx).size();
            (size.cell_width, size.line_height)
        };

        let new_size = crate::terminal::TerminalSize::from_pixels(
            area_width,
            area_height,
            cell_width,
            line_height,
        );
        let cols = new_size.columns as u32;
        let rows = new_size.lines as u32;

        terminal.update(cx, |t, _| {
            t.resize(area_width, area_height, cell_width, line_height);
        });

        let Some(channel) = channel else {
            return;
        };

        if last_sent == Some((cols, rows)) {
            return;
        }

        instance.last_sent_pty_size = Some((cols, rows));
        let channel_for_resize = channel.clone();
        cx.to_async()
            .spawn(async move |_async_cx| {
                if let Err(e) = channel_for_resize.resize(cols, rows).await {
                    error!("[Terminal] Failed to resize PTY: {:?}", e);
                }
            })
            .detach();
    }

    /// 添加新的终端实例到指定会话标签
    /// 返回新终端实例的 ID
    pub fn add_terminal_instance(&mut self, tab_id: &str) -> Option<String> {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return None;
        };

        tab.terminal_counter += 1;
        let new_instance = TerminalInstance {
            id: uuid::Uuid::new_v4().to_string(),
            index: tab.terminal_counter,
            terminal: None,
            pty_channel: None,
            pty_initialized: false,
            last_sent_pty_size: None,
            pty_error: None,
        };
        let new_id = new_instance.id.clone();
        tab.terminals.push(new_instance);
        tab.active_terminal_id = Some(new_id.clone());

        info!(
            "[Terminal] Added new terminal instance {} to tab {}",
            new_id, tab_id
        );
        Some(new_id)
    }

    /// 关闭指定的终端实例
    pub fn close_terminal_instance(&mut self, tab_id: &str, terminal_id: &str) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };

        // 不允许关闭最后一个终端实例
        if tab.terminals.len() <= 1 {
            return;
        }

        if let Some(pos) = tab.terminals.iter().position(|t| t.id == terminal_id) {
            tab.terminals.remove(pos);

            // 如果关闭的是当前激活的终端，切换到第一个
            if tab.active_terminal_id.as_deref() == Some(terminal_id) {
                tab.active_terminal_id = tab.terminals.first().map(|t| t.id.clone());
            }

            info!(
                "[Terminal] Closed terminal instance {} from tab {}",
                terminal_id, tab_id
            );
        }
    }

    /// 激活指定的终端实例
    pub fn activate_terminal_instance(&mut self, tab_id: &str, terminal_id: &str) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };

        if tab.terminals.iter().any(|t| t.id == terminal_id) {
            tab.active_terminal_id = Some(terminal_id.to_string());
            debug!(
                "[Terminal] Activated terminal instance {} in tab {}",
                terminal_id, tab_id
            );
        }
    }

    /// 获取当前激活的终端实例
    pub fn active_terminal_instance(&self, tab_id: &str) -> Option<&TerminalInstance> {
        let tab = self.tabs.iter().find(|t| t.id == tab_id)?;
        let active_id = tab.active_terminal_id.as_ref()?;
        tab.terminals.iter().find(|t| &t.id == active_id)
    }

    /// 获取指定标签的所有终端实例
    pub fn get_terminal_instances(&self, tab_id: &str) -> Vec<&TerminalInstance> {
        self.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|tab| tab.terminals.iter().collect())
            .unwrap_or_default()
    }

    /// 获取指定标签的当前激活终端 ID
    pub fn active_terminal_id(&self, tab_id: &str) -> Option<String> {
        self.tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|tab| tab.active_terminal_id.clone())
    }
}
