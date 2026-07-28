// 终端管理方法

use super::{SessionState, SessionStatus, TerminalInstance, TerminalPtyState};
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
        self.ensure_terminal_focus_handle_created(window, cx);

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
            if !instance.pty_state.can_start() {
                return;
            }
            // 获取现有终端状态（用于重连时保留历史）
            (active_id.clone(), instance.terminal.clone())
        };

        info!(
            "[Terminal] Initializing PTY for tab {} terminal {}{}",
            tab_id,
            terminal_instance_id,
            if existing_terminal.is_some() {
                " (reconnecting, preserving history)"
            } else {
                ""
            }
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
        let is_new_terminal_state = existing_terminal.is_none();
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
        let content_width =
            crate::terminal::terminal_content_width(area_width, settings.padding, cell_width);
        terminal_state.update(cx, |t, _| {
            t.apply_settings(settings.clone());
            t.resize(content_width, area_height, cell_width, line_height);
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
                instance.pty_state = TerminalPtyState::Opening;
                instance.pty_channel = None;
                instance.last_sent_pty_size = Some((cols, rows));
            }
        }

        // 每个新终端只启动一个可终止的光标任务；间隔与开关从实时设置读取。
        if is_new_terminal_state {
            let terminal_for_blink = terminal_state.clone();
            let session_for_blink = cx.entity().clone();
            let tab_for_blink = tab_id_owned.clone();
            let terminal_id_for_blink = terminal_instance_id.clone();
            cx.to_async()
                .spawn(async move |async_cx| {
                    loop {
                        let blink_state = async_cx
                            .update(|cx| {
                                let still_present = session_for_blink
                                    .read(cx)
                                    .tabs
                                    .iter()
                                    .find(|tab| tab.id == tab_for_blink)
                                    .map(|tab| {
                                        tab.terminals
                                            .iter()
                                            .any(|terminal| terminal.id == terminal_id_for_blink)
                                    })
                                    .unwrap_or(false);
                                still_present.then(|| {
                                    let terminal = terminal_for_blink.read(cx);
                                    (
                                        terminal.cursor_should_blink(),
                                        terminal.cursor_blink_interval_ms(),
                                    )
                                })
                            })
                            .ok()
                            .flatten();
                        let Some((should_blink, interval_ms)) = blink_state else {
                            break;
                        };

                        async_cx
                            .background_executor()
                            .timer(std::time::Duration::from_millis(interval_ms as u64))
                            .await;

                        if !should_blink {
                            continue;
                        }

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
        }

        // 创建 PTY 请求（使用已计算的 cols/rows）
        let pty_request =
            crate::terminal::create_pty_request(cols, rows, cell_width, line_height, &settings);

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
                            let session_id_for_err = session_id.clone();
                            let terminal_id_for_err = terminal_id_for_task.clone();
                            let _ = async_cx.update(|cx| {
                                session_state_for_task.update(cx, |state, cx| {
                                    if let Some(tab) =
                                        state.tabs.iter_mut().find(|t| t.id == session_id_for_err)
                                    {
                                        if let Some(instance) = tab
                                            .terminals
                                            .iter_mut()
                                            .find(|t| t.id == terminal_id_for_err)
                                        {
                                            instance.pty_state = TerminalPtyState::Failed(
                                                "No SSH session found".to_string(),
                                            );
                                            instance.pty_channel = None;
                                        }
                                    }
                                    cx.notify();
                                });
                            });
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
                        let terminal_still_present = async_cx
                            .update(|cx| {
                                let mut terminal_still_present = false;
                                let mut should_start_services = false;
                                session_state_for_task.update(cx, |state, cx| {
                                    if let Some(tab) = state
                                        .tabs
                                        .iter_mut()
                                        .find(|t| t.id == session_id_for_state)
                                    {
                                        if let Some(instance) = tab
                                            .terminals
                                            .iter_mut()
                                            .find(|t| t.id == terminal_id_for_state)
                                        {
                                            instance.pty_channel = Some(channel_for_state.clone());
                                            instance.pty_state = TerminalPtyState::Ready;
                                            terminal_still_present = true;
                                        }

                                        // 只有真实存在的终端 PTY 创建成功时才启动 Monitor 和 SFTP 服务。
                                        if terminal_still_present && !tab.services_started {
                                            tab.services_started = true;
                                            should_start_services = true;
                                        }
                                    }

                                    if should_start_services {
                                        state.start_monitor_service(session_id_for_state.clone(), cx);
                                        state.start_sftp_service(session_id_for_state.clone(), cx);
                                    }

                                    cx.notify();
                                });
                                terminal_still_present
                            })
                            .unwrap_or(false);

                        if !terminal_still_present {
                            debug!(
                                "[Terminal] PTY opened after terminal {} was closed; closing channel",
                                terminal_id_for_task
                            );
                            if let Err(e) = channel.close().await {
                                error!("[Terminal] Failed to close orphan PTY channel: {:?}", e);
                            }
                            return;
                        }

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
                            session_state_for_task.update(cx, |state, cx| {
                                if let Some(tab) =
                                    state.tabs.iter_mut().find(|t| t.id == session_id_for_err)
                                {
                                    if let Some(instance) = tab
                                        .terminals
                                        .iter_mut()
                                        .find(|t| t.id == terminal_id_for_err)
                                    {
                                        instance.pty_state = TerminalPtyState::Failed(error_msg);
                                        instance.pty_channel = None;
                                    }
                                }
                                cx.notify();
                            });
                        });
                    }
                }
            })
            .detach();

        cx.notify();
    }

    /// 根据当前活动终端模式，上报 xterm focus in/out 事件。
    pub fn send_terminal_focus_report(&self, focused: bool, cx: &mut gpui::Context<Self>) {
        let Some(tab_id) = self.active_tab_id.as_ref() else {
            return;
        };
        let Some(tab) = self.tabs.iter().find(|t| &t.id == tab_id) else {
            return;
        };
        let Some(active_terminal_id) = tab.active_terminal_id.as_ref() else {
            return;
        };
        let Some(instance) = tab.terminals.iter().find(|t| &t.id == active_terminal_id) else {
            return;
        };
        let (Some(terminal), Some(channel)) =
            (instance.terminal.clone(), instance.pty_channel.clone())
        else {
            return;
        };

        if !terminal
            .read(cx)
            .term_mode()
            .contains(alacritty_terminal::term::TermMode::FOCUS_IN_OUT)
        {
            return;
        }

        let bytes: &'static [u8] = if focused { b"\x1b[I" } else { b"\x1b[O" };
        if let Err(e) = channel.queue_write(bytes.to_vec()) {
            error!("[Terminal] Failed to send focus report: {:?}", e);
        }
    }

    /// 将本地终端尺寸与远端 PTY 尺寸同步到给定像素区域（用于窗口/布局变化时的自动 resize）
    /// 只同步当前激活的终端实例
    /// 注意：初始化由单独的机制触发，此方法仅处理 resize
    pub fn sync_or_initialize_terminal_size(
        &mut self,
        tab_id: &str,
        area_width: f32,
        area_height: f32,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if area_width <= 0.0 || area_height <= 0.0 {
            return;
        }

        let should_initialize = self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .filter(|tab| tab.status == SessionStatus::Connected)
            .and_then(|tab| {
                tab.active_terminal_id
                    .as_ref()
                    .and_then(|id| tab.terminals.iter().find(|t| &t.id == id))
            })
            .map(|instance| instance.pty_state.can_start())
            .unwrap_or(false);

        if should_initialize {
            self.initialize_terminal(tab_id, area_width, area_height, window, cx);
        } else {
            self.sync_terminal_size(tab_id, area_width, area_height, window, cx);
        }
    }

    pub fn sync_terminal_size(
        &mut self,
        tab_id: &str,
        area_width: f32,
        area_height: f32,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
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

        // NotStarted 由 sync_or_initialize_terminal_size 负责启动；Failed 保持错误展示，
        // 等待重连流程显式重置为 NotStarted。
        if matches!(
            instance.pty_state,
            TerminalPtyState::NotStarted | TerminalPtyState::Failed(_)
        ) {
            return;
        }

        let (Some(terminal), channel, last_sent) = (
            instance.terminal.clone(),
            instance.pty_channel.clone(),
            instance.last_sent_pty_size,
        ) else {
            return;
        };

        // 若字体/字号/行高设置发生变化，重新测量 cell metrics，避免用旧网格绘制新字体
        // 导致光标、选择、远端 PTY 行列数错位。
        let settings = crate::services::storage::load_settings()
            .unwrap_or_default()
            .terminal;
        let (font_changed, settings_changed) = {
            let terminal_read = terminal.read(cx);
            (
                terminal_read.font_settings_changed(&settings),
                terminal_read.settings_changed(&settings),
            )
        };
        let (cell_width, line_height) = if font_changed {
            let (_, _, cell_width, line_height) = crate::terminal::calculate_terminal_size(
                area_width,
                area_height,
                &settings,
                window,
                cx,
            );
            (cell_width, line_height)
        } else {
            let size = terminal.read(cx).size();
            (size.cell_width, size.line_height)
        };

        let content_width =
            crate::terminal::terminal_content_width(area_width, settings.padding, cell_width);
        let new_size = crate::terminal::TerminalSize::from_pixels(
            content_width,
            area_height,
            cell_width,
            line_height,
        );
        let cols = new_size.columns as u32;
        let rows = new_size.lines as u32;

        terminal.update(cx, |t, _| {
            if settings_changed {
                t.apply_settings(settings.clone());
            }
            t.resize(content_width, area_height, cell_width, line_height);
        });

        let Some(channel) = channel else {
            return;
        };

        if !instance.pty_state.is_ready() {
            return;
        }

        if last_sent == Some((cols, rows)) {
            return;
        }

        instance.last_sent_pty_size = Some((cols, rows));
        let pix_width = (cols as f32 * cell_width) as u32;
        let pix_height = (rows as f32 * line_height) as u32;
        if let Err(e) = channel.resize(cols, rows, pix_width, pix_height) {
            error!("[Terminal] Failed to queue PTY resize: {:?}", e);
        }
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
            pty_state: TerminalPtyState::NotStarted,
            last_sent_pty_size: None,
            title: None,
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
    pub fn close_terminal_instance(
        &mut self,
        tab_id: &str,
        terminal_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };

        // 不允许关闭最后一个终端实例
        if tab.terminals.len() <= 1 {
            return;
        }

        if let Some(pos) = tab.terminals.iter().position(|t| t.id == terminal_id) {
            let instance = tab.terminals.remove(pos);

            // 如果关闭的是当前激活的终端，切换到第一个
            if tab.active_terminal_id.as_deref() == Some(terminal_id) {
                tab.active_terminal_id = tab.terminals.first().map(|t| t.id.clone());
            }

            info!(
                "[Terminal] Closed terminal instance {} from tab {}",
                terminal_id, tab_id
            );

            if let Some(channel) = instance.pty_channel {
                cx.to_async()
                    .spawn(async move |_async_cx| {
                        if let Err(e) = channel.close().await {
                            error!("[Terminal] Failed to close PTY channel: {:?}", e);
                        }
                    })
                    .detach();
            }
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

    /// 重新启动一个已结束或失败的远端 shell，保留当前终端历史。
    pub fn restart_terminal_instance(&mut self, tab_id: &str, terminal_id: &str) {
        let Some(instance) = self
            .tabs
            .iter_mut()
            .find(|tab| tab.id == tab_id)
            .and_then(|tab| {
                tab.terminals
                    .iter_mut()
                    .find(|terminal| terminal.id == terminal_id)
            })
        else {
            return;
        };

        if matches!(instance.pty_state, TerminalPtyState::Failed(_)) {
            instance.pty_channel = None;
            instance.last_sent_pty_size = None;
            instance.pty_state = TerminalPtyState::NotStarted;
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
