// SFTP 内置文件编辑器状态与异步读写逻辑

use std::path::Path;
use std::sync::Arc;

use gpui_kit::component::input::{EditorState, InputEvent};
use gpui_kit::component::notification::{Notification, NotificationType};
use gpui_kit::component::WindowExt;
use gpui_kit::{px, App, AppContext as _, Context, Edges, Styled as _, Window};
use tracing::{debug, error, info, warn};

use crate::i18n;
use crate::services::sftp::{
    decode_utf8_text, detect_language_from_path, encode_utf8_text, DecodedTextFile, LineEnding,
    TextDecodeError,
};

use super::{CenterViewMode, EditorBufferStatus, SessionState, SessionStatus, SftpEditorTab};

/// 远程读取结果
enum FetchFileOutcome {
    /// 软链接指向目录，应转为目录导航
    IsDirectory,
    /// 文本内容解码成功
    Text(DecodedTextFile),
    /// 超过最大可编辑大小限制（包含真实字节数）
    TooLarge { size: u64 },
    /// 检测到二进制内容，拒绝打开
    Binary,
    /// 非 UTF-8 编码，拒绝打开（避免写回损坏原编码）
    InvalidUtf8,
    /// SFTP 读取或权限错误
    Error(String),
}

/// 在活动窗口弹出顶部 Toast 通知
fn push_toast(message: String, kind: NotificationType, cx: &mut App) {
    if let Some(window) = cx.active_window() {
        let _ = cx.update_window(window, |_, window, cx| {
            let notification = Notification::new()
                .message(message)
                .with_type(kind)
                .w_72()
                .py_2();
            window.push_notification(notification, cx);
        });
    }
}

/// 格式化字节数为人类可读大小
fn format_file_size(size: u64) -> String {
    let size_f = size as f64;
    if size_f >= 1_073_741_824.0 {
        format!("{:.1} GB", size_f / 1_073_741_824.0)
    } else if size_f >= 1_048_576.0 {
        format!("{:.1} MB", size_f / 1_048_576.0)
    } else if size_f >= 1_024.0 {
        format!("{:.1} KB", size_f / 1_024.0)
    } else {
        format!("{} B", size)
    }
}

impl SessionState {
    /// 打开或聚焦一个内置编辑器远程文件标签
    pub fn sftp_open_builtin_editor(
        &mut self,
        tab_id: &str,
        remote_path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };

        // 如果该路径已经在内置编辑器中打开，直接切换并聚焦
        if let Some(existing) = tab
            .editor_tabs
            .iter()
            .find(|e| e.remote_path == remote_path)
        {
            let existing_id = existing.id.clone();
            tab.active_editor_tab_id = Some(existing_id);
            tab.center_view = CenterViewMode::Editor;
            cx.notify();
            return;
        }

        let detected = detect_language_from_path(&remote_path);
        let file_name = Path::new(&remote_path)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())
            .unwrap_or(&remote_path)
            .to_string();

        let editor_id = uuid::Uuid::new_v4().to_string();
        let new_tab = SftpEditorTab {
            id: editor_id.clone(),
            remote_path: remote_path.clone(),
            file_name,
            syntax_token: detected.syntax_token,
            language_name: detected.display_name,
            line_ending: LineEnding::Lf,
            has_bom: false,
            original_content: Arc::new(String::new()),
            is_modified: false,
            is_saving: false,
            soft_wrap: false,
            pending_reload_content: None,
            status: EditorBufferStatus::Loading,
        };

        tab.editor_tabs.push(new_tab);
        tab.active_editor_tab_id = Some(editor_id.clone());
        tab.center_view = CenterViewMode::Editor;
        cx.notify();

        self.spawn_fetch_remote_file(tab_id.to_string(), editor_id, remote_path, false, cx);
    }

    /// 异步拉取远程文件用于首次打开或重新加载
    fn spawn_fetch_remote_file(
        &mut self,
        tab_id: String,
        editor_tab_id: String,
        remote_path: String,
        is_reload: bool,
        cx: &mut Context<Self>,
    ) {
        let settings = crate::services::storage::cached_settings();
        let max_size_bytes = (settings.sftp.max_edit_file_size_kb as u64).saturating_mul(1024);

        let service = {
            let guard = match self.sftp_services.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("[Editor] Failed to lock sftp_services: {}", e);
                    return;
                }
            };
            guard.get(&tab_id).cloned()
        };
        let Some(service) = service else {
            let lang = settings.theme.language.clone();
            let msg = i18n::t(&lang, "sftp.editor.disconnected").to_string();
            if !is_reload {
                self.remove_editor_tab_internal(&tab_id, &editor_tab_id);
            }
            push_toast(msg, NotificationType::Warning, cx);
            cx.notify();
            return;
        };

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<FetchFileOutcome>();
        let remote_path_for_task = remote_path.clone();

        let ssh_manager = crate::ssh::manager::SshManager::global();
        ssh_manager.runtime().spawn(async move {
            use tokio::io::AsyncReadExt;

            // 先查询 metadata（跟随软链接），确认目标不是目录且大小未超限
            match service.sftp().metadata(&remote_path_for_task).await {
                Ok(meta) => {
                    if meta.is_dir() {
                        let _ = tx.send(FetchFileOutcome::IsDirectory);
                        return;
                    }
                    if let Some(size) = meta.size {
                        if max_size_bytes > 0 && size > max_size_bytes {
                            let _ = tx.send(FetchFileOutcome::TooLarge { size });
                            return;
                        }
                    }
                }
                Err(e) => {
                    debug!(
                        "[Editor] metadata({}) failed (fallback to open): {}",
                        remote_path_for_task, e
                    );
                }
            }

            let mut file = match service.open(&remote_path_for_task).await {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.send(FetchFileOutcome::Error(format!("{}", e)));
                    return;
                }
            };

            let mut bytes = Vec::new();
            if let Err(e) = file.read_to_end(&mut bytes).await {
                let _ = tx.send(FetchFileOutcome::Error(format!("{}", e)));
                return;
            }

            if max_size_bytes > 0 && (bytes.len() as u64) > max_size_bytes {
                let _ = tx.send(FetchFileOutcome::TooLarge {
                    size: bytes.len() as u64,
                });
                return;
            }

            match decode_utf8_text(&bytes) {
                Ok(decoded) => {
                    let _ = tx.send(FetchFileOutcome::Text(decoded));
                }
                Err(TextDecodeError::Binary) => {
                    let _ = tx.send(FetchFileOutcome::Binary);
                }
                Err(TextDecodeError::InvalidUtf8) => {
                    let _ = tx.send(FetchFileOutcome::InvalidUtf8);
                }
            }
        });

        let state_entity = cx.entity().clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                let Some(outcome) = rx.recv().await else {
                    return;
                };

                let _ = async_cx.update(|cx| {
                    let lang = crate::services::storage::cached_settings()
                        .theme
                        .language
                        .clone();

                    state_entity.update(cx, |state, cx| match outcome {
                        FetchFileOutcome::IsDirectory => {
                            if !is_reload {
                                state.remove_editor_tab_internal(&tab_id, &editor_tab_id);
                            }
                            state.sftp_navigate_to(&tab_id, remote_path, cx);
                            cx.notify();
                        }
                        FetchFileOutcome::Text(decoded) => {
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                if let Some(ed) =
                                    tab.editor_tabs.iter_mut().find(|e| e.id == editor_tab_id)
                                {
                                    ed.line_ending = decoded.line_ending;
                                    ed.has_bom = decoded.has_bom;
                                    ed.original_content = Arc::new(decoded.content.clone());
                                    ed.is_modified = false;
                                    match &ed.status {
                                        EditorBufferStatus::Ready { .. } => {
                                            ed.pending_reload_content = Some(decoded.content);
                                        }
                                        _ => {
                                            ed.status = EditorBufferStatus::PendingInit {
                                                content: decoded.content,
                                            };
                                            ed.pending_reload_content = None;
                                        }
                                    }
                                    info!("[Editor] Loaded remote file: {}", ed.remote_path);
                                }
                            }
                            if is_reload {
                                push_toast(
                                    i18n::t(&lang, "sftp.editor.reload_success").to_string(),
                                    NotificationType::Success,
                                    cx,
                                );
                            }
                            cx.notify();
                        }
                        FetchFileOutcome::TooLarge { size } => {
                            warn!(
                                "[Editor] File too large for built-in editor: {} ({} bytes)",
                                remote_path, size
                            );
                            if !is_reload {
                                state.remove_editor_tab_internal(&tab_id, &editor_tab_id);
                            }
                            push_toast(
                                format!(
                                    "{}: {}",
                                    i18n::t(&lang, "sftp.edit.file_too_large"),
                                    format_file_size(size)
                                ),
                                NotificationType::Warning,
                                cx,
                            );
                            cx.notify();
                        }
                        FetchFileOutcome::Binary => {
                            warn!("[Editor] Binary file rejected: {}", remote_path);
                            if !is_reload {
                                state.remove_editor_tab_internal(&tab_id, &editor_tab_id);
                            }
                            push_toast(
                                i18n::t(&lang, "sftp.editor.binary_file").to_string(),
                                NotificationType::Warning,
                                cx,
                            );
                            cx.notify();
                        }
                        FetchFileOutcome::InvalidUtf8 => {
                            warn!("[Editor] Non-UTF-8 file rejected: {}", remote_path);
                            if !is_reload {
                                state.remove_editor_tab_internal(&tab_id, &editor_tab_id);
                            }
                            push_toast(
                                i18n::t(&lang, "sftp.editor.invalid_utf8").to_string(),
                                NotificationType::Warning,
                                cx,
                            );
                            cx.notify();
                        }
                        FetchFileOutcome::Error(err) => {
                            error!("[Editor] Failed to load {}: {}", remote_path, err);
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                if let Some(ed) =
                                    tab.editor_tabs.iter_mut().find(|e| e.id == editor_tab_id)
                                {
                                    if !is_reload {
                                        ed.status = EditorBufferStatus::Error(err.clone());
                                    }
                                }
                            }
                            push_toast(
                                format!("{}: {}", i18n::t(&lang, "sftp.editor.load_failed"), err),
                                NotificationType::Error,
                                cx,
                            );
                            cx.notify();
                        }
                    });
                });
            })
            .detach();
    }

    /// 在渲染阶段（持有 `&mut Window`）为 PendingInit 的编辑器标签创建 `EditorState`，
    /// 并应用已就绪标签的重载内容及 padding 配置
    pub fn ensure_sftp_editor_states(
        &mut self,
        tab_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let settings = crate::services::storage::cached_settings();
        let gutter_pad = px(settings.sftp.editor_gutter_padding as f32);

        // 先收集需要初始化与需要重载的标签信息，避免同时借用 self.tabs 和 cx
        struct InitJob {
            editor_id: String,
            syntax_token: &'static str,
            soft_wrap: bool,
            content: String,
            should_focus: bool,
        }
        struct ReloadJob {
            editor_id: String,
            editor_state: gpui_kit::Entity<EditorState>,
            content: String,
        }

        let mut init_jobs = Vec::new();
        let mut reload_jobs = Vec::new();

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            let active_editor_id = tab.active_editor_tab_id.clone();
            let in_editor_view = tab.center_view == CenterViewMode::Editor;

            for ed in &mut tab.editor_tabs {
                if let EditorBufferStatus::PendingInit { content } = &mut ed.status {
                    let taken = std::mem::take(content);
                    init_jobs.push(InitJob {
                        editor_id: ed.id.clone(),
                        syntax_token: ed.syntax_token,
                        soft_wrap: ed.soft_wrap,
                        content: taken,
                        should_focus: in_editor_view
                            && active_editor_id.as_deref() == Some(ed.id.as_str()),
                    });
                } else if let EditorBufferStatus::Ready { editor_state } = &ed.status {
                    if let Some(new_content) = ed.pending_reload_content.take() {
                        reload_jobs.push(ReloadJob {
                            editor_id: ed.id.clone(),
                            editor_state: editor_state.clone(),
                            content: new_content,
                        });
                    }
                }
            }
        }

        for job in init_jobs {
            let editor_state = cx.new(|cx| {
                let mut state = EditorState::new(window, cx)
                    .language(job.syntax_token)
                    .line_number(true)
                    .folding(true)
                    .searchable(true)
                    .replaceable(true)
                    .indent_guides(true)
                    .auto_close(true)
                    .soft_wrap(job.soft_wrap)
                    .default_value(job.content);
                state.set_editor_paddings(Edges {
                    top: px(8.),
                    right: px(12.),
                    bottom: px(16.),
                    left: gutter_pad,
                });
                state
            });

            let tab_id_sub = tab_id.to_string();
            let editor_id_sub = job.editor_id.clone();
            let sub = cx.subscribe(
                &editor_state,
                move |state, entity, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        let current = entity.read(cx).value();
                        state.update_editor_tab_modified(
                            &tab_id_sub,
                            &editor_id_sub,
                            current.as_str(),
                        );
                        cx.notify();
                    }
                },
            );
            self.sftp_editor_subscriptions
                .insert(job.editor_id.clone(), sub);

            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                if let Some(ed) = tab.editor_tabs.iter_mut().find(|e| e.id == job.editor_id) {
                    ed.status = EditorBufferStatus::Ready {
                        editor_state: editor_state.clone(),
                    };
                }
            }

            if job.should_focus {
                editor_state.update(cx, |s, cx| s.focus(window, cx));
            }
        }

        for job in reload_jobs {
            job.editor_state.update(cx, |s, cx| {
                s.set_value(job.content, window, cx);
            });
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                if let Some(ed) = tab.editor_tabs.iter_mut().find(|e| e.id == job.editor_id) {
                    ed.is_modified = false;
                }
            }
        }
    }

    /// 根据当前文本对比基准文本，更新 `is_modified` 标记
    fn update_editor_tab_modified(&mut self, tab_id: &str, editor_tab_id: &str, current: &str) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if let Some(ed) = tab.editor_tabs.iter_mut().find(|e| e.id == editor_tab_id) {
                ed.is_modified = current != ed.original_content.as_str();
            }
        }
    }

    /// 切换中间主区到指定文件标签
    pub fn sftp_activate_editor_tab(&mut self, tab_id: &str, editor_tab_id: &str) {
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if tab.editor_tabs.iter().any(|e| e.id == editor_tab_id) {
                tab.active_editor_tab_id = Some(editor_tab_id.to_string());
                tab.center_view = CenterViewMode::Editor;
            }
        }
    }

    /// 切换自动换行状态
    pub fn sftp_toggle_editor_soft_wrap(
        &mut self,
        tab_id: &str,
        editor_tab_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut target: Option<(gpui_kit::Entity<EditorState>, bool)> = None;
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if let Some(ed) = tab.editor_tabs.iter_mut().find(|e| e.id == editor_tab_id) {
                ed.soft_wrap = !ed.soft_wrap;
                if let EditorBufferStatus::Ready { editor_state } = &ed.status {
                    target = Some((editor_state.clone(), ed.soft_wrap));
                }
            }
        }
        if let Some((editor_state, wrap)) = target {
            editor_state.update(cx, |s, cx| s.set_soft_wrap(wrap, window, cx));
        }
        cx.notify();
    }

    /// 保存当前或指定编辑器标签内容到远端 SFTP
    pub fn sftp_save_editor_tab(
        &mut self,
        tab_id: &str,
        editor_tab_id: &str,
        close_after_save: bool,
        cx: &mut Context<Self>,
    ) {
        let lang = crate::services::storage::cached_settings()
            .theme
            .language
            .clone();

        // 1. 提取会话状态与待保存内容
        let (remote_path, line_ending, has_bom, editor_state) = {
            let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) else {
                return;
            };
            if !matches!(tab.status, SessionStatus::Connected) {
                push_toast(
                    i18n::t(&lang, "sftp.editor.disconnected").to_string(),
                    NotificationType::Warning,
                    cx,
                );
                return;
            }
            let Some(ed) = tab.editor_tabs.iter().find(|e| e.id == editor_tab_id) else {
                return;
            };
            if ed.is_saving {
                return;
            }
            let EditorBufferStatus::Ready { editor_state } = &ed.status else {
                return;
            };
            (
                ed.remote_path.clone(),
                ed.line_ending,
                ed.has_bom,
                editor_state.clone(),
            )
        };

        let content_lf = editor_state.read(cx).value().to_string();
        let bytes = encode_utf8_text(&content_lf, line_ending, has_bom);

        let service = {
            let guard = match self.sftp_services.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("[Editor] Failed to lock sftp_services: {}", e);
                    return;
                }
            };
            match guard.get(tab_id) {
                Some(s) => s.clone(),
                None => {
                    push_toast(
                        i18n::t(&lang, "sftp.editor.disconnected").to_string(),
                        NotificationType::Warning,
                        cx,
                    );
                    return;
                }
            }
        };

        // 标记保存中
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if let Some(ed) = tab.editor_tabs.iter_mut().find(|e| e.id == editor_tab_id) {
                ed.is_saving = true;
            }
        }
        cx.notify();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Result<(), String>>();
        let remote_path_for_write = remote_path.clone();

        let ssh_manager = crate::ssh::manager::SshManager::global();
        ssh_manager.runtime().spawn(async move {
            let res = service
                .write_file(&remote_path_for_write, &bytes)
                .await
                .map_err(|e| format!("{}", e));
            let _ = tx.send(res);
        });

        let state_entity = cx.entity().clone();
        let tab_id_owned = tab_id.to_string();
        let editor_id_owned = editor_tab_id.to_string();

        cx.to_async()
            .spawn(async move |async_cx| {
                let Some(result) = rx.recv().await else {
                    return;
                };

                let _ = async_cx.update(|cx| {
                    let lang = crate::services::storage::cached_settings()
                        .theme
                        .language
                        .clone();

                    state_entity.update(cx, |state, cx| match result {
                        Ok(()) => {
                            let mut should_refresh_dir = false;
                            let parent_dir = Path::new(&remote_path)
                                .parent()
                                .and_then(|p| p.to_str())
                                .map(|p| if p.is_empty() { "/" } else { p });

                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_owned)
                            {
                                if let Some(ed) =
                                    tab.editor_tabs.iter_mut().find(|e| e.id == editor_id_owned)
                                {
                                    ed.is_saving = false;
                                    ed.original_content = Arc::new(content_lf.clone());
                                    // 对比当前编辑器最新文本（防止保存期间用户又输入了新字符）
                                    let latest_matches = match &ed.status {
                                        EditorBufferStatus::Ready { editor_state } => {
                                            editor_state.read(cx).value().as_str()
                                                == content_lf.as_str()
                                        }
                                        _ => true,
                                    };
                                    ed.is_modified = !latest_matches;
                                }

                                if let (Some(sftp), Some(parent)) = (&tab.sftp_state, parent_dir) {
                                    let norm_current = sftp.current_path.trim_end_matches('/');
                                    let norm_parent = parent.trim_end_matches('/');
                                    if norm_current == norm_parent {
                                        should_refresh_dir = true;
                                    }
                                }
                            }

                            info!("[Editor] Saved remote file: {}", remote_path);
                            push_toast(
                                i18n::t(&lang, "sftp.editor.save_success").to_string(),
                                NotificationType::Success,
                                cx,
                            );

                            if should_refresh_dir {
                                state.sftp_refresh(&tab_id_owned, cx);
                            }

                            if close_after_save {
                                state.sftp_force_close_editor_tab(
                                    &tab_id_owned,
                                    &editor_id_owned,
                                    cx,
                                );
                            }

                            cx.notify();
                        }
                        Err(e) => {
                            error!("[Editor] Failed to save {}: {}", remote_path, e);
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_owned)
                            {
                                if let Some(ed) =
                                    tab.editor_tabs.iter_mut().find(|e| e.id == editor_id_owned)
                                {
                                    ed.is_saving = false;
                                }
                            }
                            push_toast(
                                format!("{}: {}", i18n::t(&lang, "sftp.editor.save_failed"), e),
                                NotificationType::Error,
                                cx,
                            );
                            cx.notify();
                        }
                    });
                });
            })
            .detach();
    }

    /// 重新从远端加载文件；若存在未保存修改且 `force == false`，弹出确认弹窗
    pub fn sftp_reload_editor_tab(
        &mut self,
        tab_id: &str,
        editor_tab_id: &str,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        let Some((remote_path, is_modified, is_error)) = self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.editor_tabs.iter().find(|e| e.id == editor_tab_id))
            .map(|e| {
                (
                    e.remote_path.clone(),
                    e.is_modified,
                    matches!(e.status, EditorBufferStatus::Error(_)),
                )
            })
        else {
            return;
        };

        if !force && is_modified {
            self.pending_reload_editor_tab = Some((tab_id.to_string(), editor_tab_id.to_string()));
            cx.notify();
            return;
        }

        if is_error {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                if let Some(ed) = tab.editor_tabs.iter_mut().find(|e| e.id == editor_tab_id) {
                    ed.status = EditorBufferStatus::Loading;
                }
            }
            cx.notify();
        }

        self.spawn_fetch_remote_file(
            tab_id.to_string(),
            editor_tab_id.to_string(),
            remote_path,
            !is_error,
            cx,
        );
    }

    /// 请求关闭指定编辑器文件标签（有未保存改动时先弹确认弹窗）
    pub fn sftp_request_close_editor_tab(
        &mut self,
        tab_id: &str,
        editor_tab_id: &str,
        cx: &mut Context<Self>,
    ) {
        let is_modified = self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.editor_tabs.iter().find(|e| e.id == editor_tab_id))
            .map(|e| e.is_modified)
            .unwrap_or(false);

        if is_modified {
            self.pending_close_editor_tab = Some((tab_id.to_string(), editor_tab_id.to_string()));
            cx.notify();
        } else {
            self.sftp_force_close_editor_tab(tab_id, editor_tab_id, cx);
        }
    }

    /// 强制关闭指定编辑器文件标签并清理订阅
    pub fn sftp_force_close_editor_tab(
        &mut self,
        tab_id: &str,
        editor_tab_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.remove_editor_tab_internal(tab_id, editor_tab_id);
        if self.pending_close_editor_tab.as_ref()
            == Some(&(tab_id.to_string(), editor_tab_id.to_string()))
        {
            self.pending_close_editor_tab = None;
        }
        if self.pending_reload_editor_tab.as_ref()
            == Some(&(tab_id.to_string(), editor_tab_id.to_string()))
        {
            self.pending_reload_editor_tab = None;
        }
        cx.notify();
    }

    /// 内部移除编辑器标签并维护激活态与订阅清理
    fn remove_editor_tab_internal(&mut self, tab_id: &str, editor_tab_id: &str) {
        self.sftp_editor_subscriptions.remove(editor_tab_id);

        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) else {
            return;
        };
        let Some(pos) = tab.editor_tabs.iter().position(|e| e.id == editor_tab_id) else {
            return;
        };
        tab.editor_tabs.remove(pos);

        if tab.active_editor_tab_id.as_deref() == Some(editor_tab_id) {
            if tab.editor_tabs.is_empty() {
                tab.active_editor_tab_id = None;
                tab.center_view = CenterViewMode::Terminal;
            } else {
                let next_idx = pos.min(tab.editor_tabs.len() - 1);
                tab.active_editor_tab_id = Some(tab.editor_tabs[next_idx].id.clone());
            }
        } else if tab.editor_tabs.is_empty() {
            tab.center_view = CenterViewMode::Terminal;
        }
    }

    /// 检查会话标签是否存在未保存的编辑器文件；有则弹确认弹窗，无则直接关闭会话
    pub fn request_close_tab(&mut self, tab_id: &str, cx: &mut Context<Self>) {
        let has_unsaved = self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.editor_tabs.iter().any(|e| e.is_modified))
            .unwrap_or(false);

        if has_unsaved {
            self.activate_tab(tab_id);
            self.show_home = false;
            self.pending_close_session_tab = Some(tab_id.to_string());
            cx.notify();
        } else {
            self.close_tab(tab_id);
            cx.notify();
        }
    }
}
