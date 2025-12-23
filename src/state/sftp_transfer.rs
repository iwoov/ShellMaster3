//! SFTP file transfer methods for SessionState.
//!
//! This module contains methods for downloading, uploading files, and managing transfer state.

use super::{NewFolderDialogState, SessionState};
use gpui::prelude::*;
use gpui::Entity;
use tracing::{error, info};

impl SessionState {
    /// 下载文件到本地
    ///
    /// 使用系统文件选择器选择保存位置，然后异步下载文件
    pub fn sftp_download_file(
        &mut self,
        tab_id: &str,
        remote_path: String,
        file_name: String,
        file_size: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        info!(
            "[SFTP] Download file: {} ({} bytes) for tab {}",
            remote_path, file_size, tab_id
        );

        let sftp_services = self.sftp_services.clone();
        let session_state = cx.entity().clone();
        let tab_id_owned = tab_id.to_string();

        // 尝试获取 SFTP 服务
        let service = {
            let guard = match sftp_services.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("[SFTP] Failed to lock sftp_services: {}", e);
                    return;
                }
            };
            match guard.get(&tab_id_owned) {
                Some(s) => s.clone(),
                None => {
                    error!("[SFTP] No SFTP service for tab {}", tab_id_owned);
                    return;
                }
            }
        };

        // 获取 SSH manager 的 runtime
        let ssh_manager = crate::ssh::manager::SshManager::global();
        let runtime = ssh_manager.runtime();

        // 尝试获取默认下载路径
        let default_path = crate::services::storage::load_settings()
            .map(|s| s.sftp.local_default_path.clone())
            .unwrap_or_default();

        let file_name_clone = file_name.clone();

        // 使用 GPUI 异步上下文执行文件选择和下载
        cx.to_async()
            .spawn(async move |async_cx| {
                // 确定保存路径：优先使用默认路径，否则打开文件选择器
                let local_path = if !default_path.is_empty() {
                    // 使用默认下载路径 + 文件名
                    let path = std::path::PathBuf::from(&default_path).join(&file_name_clone);
                    info!("[SFTP] Using default download path: {:?}", path);
                    path
                } else {
                    // 打开系统文件保存对话框
                    let file_picker = rfd::AsyncFileDialog::new()
                        .set_title("保存文件")
                        .set_file_name(&file_name_clone);

                    let save_handle = file_picker.save_file().await;

                    let Some(file_handle) = save_handle else {
                        info!("[SFTP] Download cancelled by user");
                        return;
                    };

                    file_handle.path().to_path_buf()
                };

                info!("[SFTP] Downloading to: {:?}", local_path);

                // 创建传输项并添加到列表
                let transfer_item = crate::models::sftp::TransferItem::new_download(
                    remote_path.clone(),
                    local_path.clone(),
                    file_size,
                );
                // 使用 transfer_item 内部生成的 id
                let transfer_id_clone = transfer_item.id.clone();
                // 克隆取消令牌以便在下载任务中使用
                let cancel_token = transfer_item.cancel_token.clone();
                // 克隆暂停标志以便在下载任务中使用
                let pause_flag = transfer_item.pause_flag.clone();

                // 添加传输项到列表，同时自动切换到传输面板
                let tab_id_for_transfer = tab_id_owned.clone();
                let _ = async_cx.update(|cx| {
                    session_state.update(cx, |state, cx| {
                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_for_transfer) {
                            tab.active_transfers.push(transfer_item);
                        }
                        // 自动切换到传输管理面板
                        state.set_sidebar_panel(super::SidebarPanel::Transfer);
                        cx.notify();
                    });
                });

                // 创建 channel 用于从 tokio 运行时发送进度和结果到 GPUI
                enum DownloadEvent {
                    Progress(u64, u64, u64), // transferred, total, speed
                    Complete(Result<(), String>),
                }
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<DownloadEvent>();

                // 在 SSH 运行时中执行下载
                let remote_path_clone = remote_path.clone();
                let local_path_clone = local_path.clone();
                let tx_progress = tx.clone();
                let tab_id_for_download = tab_id_owned.clone();

                // 多通道下载阈值：10MB
                const MULTI_CHANNEL_THRESHOLD: u64 = 10 * 1024 * 1024;

                // 获取并行通道数设置
                let concurrent_transfers = crate::services::storage::load_settings()
                    .map(|s| s.sftp.concurrent_transfers as usize)
                    .unwrap_or(3);

                // 克隆取消令牌用于下载任务内部
                let cancel_token_for_download = cancel_token.clone();

                runtime.spawn(async move {
                    let result =
                        if file_size >= MULTI_CHANNEL_THRESHOLD && concurrent_transfers > 1 {
                            // 使用多通道下载
                            info!(
                            "[SFTP] Using multi-channel download ({} channels) for {} ({} bytes)",
                            concurrent_transfers, remote_path_clone, file_size
                        );

                            // 获取 SSH session
                            let ssh_manager = crate::ssh::manager::SshManager::global();
                            if let Some(ssh_session) = ssh_manager.get_session(&tab_id_for_download)
                            {
                                let downloader = crate::services::sftp::MultiChannelDownloader::new(
                                    ssh_session,
                                    tab_id_for_download.clone(),
                                    concurrent_transfers,
                                );

                                let tx_progress_clone = tx_progress.clone();
                                downloader
                                    .download_file(
                                        &remote_path_clone,
                                        &local_path_clone,
                                        file_size,
                                        cancel_token_for_download,
                                        pause_flag,
                                        move |transferred, total, speed| {
                                            let _ = tx_progress_clone.send(
                                                DownloadEvent::Progress(transferred, total, speed),
                                            );
                                        },
                                    )
                                    .await
                            } else {
                                Err(format!("SSH session not found: {}", tab_id_for_download))
                            }
                        } else {
                            // 使用单通道下载（小文件或只有1个通道）
                            service
                                .download_file(
                                    &remote_path_clone,
                                    &local_path_clone,
                                    move |transferred, total, speed| {
                                        let _ = tx_progress.send(DownloadEvent::Progress(
                                            transferred,
                                            total,
                                            speed,
                                        ));
                                    },
                                )
                                .await
                        };

                    let _ = tx.send(DownloadEvent::Complete(result));
                });

                // 接收进度和结果，同时监听取消信号
                loop {
                    tokio::select! {
                        // 监听取消信号
                        _ = cancel_token.cancelled() => {
                            info!("[SFTP] Download cancelled by user: {}", transfer_id_clone);
                            // 删除未完成的文件（使用 std::fs 因为不在 tokio 运行时中）
                            let _ = std::fs::remove_file(&local_path);
                            // 更新状态
                            let transfer_id = transfer_id_clone.clone();
                            let tab_id = tab_id_owned.clone();
                            let _ = async_cx.update(|cx| {
                                session_state.update(cx, |state, cx| {
                                    if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                        if let Some(transfer) = tab
                                            .active_transfers
                                            .iter_mut()
                                            .find(|t| t.id == transfer_id)
                                        {
                                            transfer.status = crate::models::sftp::TransferStatus::Cancelled;
                                            transfer.error = Some("用户取消".to_string());
                                        }
                                    }
                                    cx.notify();
                                });
                            });
                            break;
                        }
                        // 接收下载事件
                        event = rx.recv() => {
                            match event {
                                Some(DownloadEvent::Progress(transferred, total, speed)) => {
                                    let transfer_id = transfer_id_clone.clone();
                                    let tab_id = tab_id_owned.clone();
                                    let _ = async_cx.update(|cx| {
                                        session_state.update(cx, |state, cx| {
                                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                                if let Some(transfer) = tab
                                                    .active_transfers
                                                    .iter_mut()
                                                    .find(|t| t.id == transfer_id)
                                                {
                                                    // 使用安全的进度更新方法
                                                    transfer.update_progress(transferred, total, speed);
                                                }
                                            }
                                            cx.notify();
                                        });
                                    });
                                }
                                Some(DownloadEvent::Complete(result)) => {
                                    let transfer_id = transfer_id_clone.clone();
                                    let local_path = local_path.clone();
                                    let tab_id = tab_id_owned.clone();
                                    let _ = async_cx.update(|cx| {
                                        session_state.update(cx, |state, cx| {
                                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                                if let Some(transfer) = tab
                                                    .active_transfers
                                                    .iter_mut()
                                                    .find(|t| t.id == transfer_id)
                                                {
                                                    match &result {
                                                        Ok(()) => {
                                                            transfer.set_completed();
                                                            info!(
                                                                "[SFTP] Download completed: {:?}",
                                                                local_path
                                                            );
                                                        }
                                                        Err(e) => {
                                                            transfer.set_failed(e.clone());
                                                            error!("[SFTP] Download failed: {}", e);
                                                        }
                                                    }
                                                }
                                            }
                                            cx.notify();
                                        });
                                    });
                                    break;
                                }
                                None => {
                                    // Channel closed unexpectedly
                                    break;
                                }
                            }
                        }
                    }
                }
            })
            .detach();
    }

    /// 上传本地文件到远程服务器
    ///
    /// 打开系统文件选择器选择要上传的文件，然后异步上传
    pub fn sftp_upload_file(
        &mut self,
        tab_id: &str,
        remote_dir: String,
        cx: &mut gpui::Context<Self>,
    ) {
        info!(
            "[SFTP] Upload file to remote directory: {} for tab {}",
            remote_dir, tab_id
        );

        let sftp_services = self.sftp_services.clone();
        let session_state = cx.entity().clone();
        let tab_id_owned = tab_id.to_string();

        // 尝试获取 SFTP 服务
        let service = {
            let guard = match sftp_services.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("[SFTP] Failed to lock sftp_services: {}", e);
                    return;
                }
            };
            match guard.get(&tab_id_owned) {
                Some(s) => s.clone(),
                None => {
                    error!("[SFTP] No SFTP service for tab {}", tab_id_owned);
                    return;
                }
            }
        };

        // 获取 SSH manager 的 runtime
        let ssh_manager = crate::ssh::manager::SshManager::global();
        let runtime = ssh_manager.runtime();

        // 使用 GPUI 异步上下文执行文件选择和上传
        cx.to_async()
            .spawn(async move |async_cx| {
                // 打开系统文件选择对话框
                let file_picker = rfd::AsyncFileDialog::new()
                    .set_title("选择要上传的文件");

                let file_handle = file_picker.pick_file().await;

                let Some(file_handle) = file_handle else {
                    info!("[SFTP] Upload cancelled by user");
                    return;
                };

                let local_path = file_handle.path().to_path_buf();
                let file_name = local_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "unknown".to_string());

                // 获取文件大小（使用 std::fs 因为不在 tokio 运行时中）
                let file_size = match std::fs::metadata(&local_path) {
                    Ok(metadata) => metadata.len(),
                    Err(e) => {
                        error!("[SFTP] Failed to get file metadata: {}", e);
                        return;
                    }
                };

                // 构建远程路径
                let remote_path = if remote_dir == "/" {
                    format!("/{}", file_name)
                } else {
                    format!("{}/{}", remote_dir.trim_end_matches('/'), file_name)
                };

                info!("[SFTP] Uploading {:?} to {}", local_path, remote_path);

                // 创建传输项并添加到列表
                let transfer_item = crate::models::sftp::TransferItem::new_upload(
                    local_path.clone(),
                    remote_path.clone(),
                    file_size,
                );
                let transfer_id_clone = transfer_item.id.clone();
                let cancel_token = transfer_item.cancel_token.clone();
                // 克隆暂停标志以便在上传任务中使用（必须在 push 之前克隆）
                let pause_flag = transfer_item.pause_flag.clone();

                // 添加传输项到列表，同时自动切换到传输面板
                let tab_id_for_transfer = tab_id_owned.clone();
                let _ = async_cx.update(|cx| {
                    session_state.update(cx, |state, cx| {
                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_for_transfer) {
                            tab.active_transfers.push(transfer_item);
                        }
                        // 自动切换到传输管理面板
                        state.set_sidebar_panel(super::SidebarPanel::Transfer);
                        cx.notify();
                    });
                });

                // 创建 channel 用于从 tokio 运行时发送进度和结果到 GPUI
                enum UploadEvent {
                    Progress(u64, u64, u64), // transferred, total, speed
                    Complete(Result<(), String>),
                }
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UploadEvent>();

                // 在 SSH 运行时中执行上传
                let local_path_clone = local_path.clone();
                let remote_path_clone = remote_path.clone();
                let tx_progress = tx.clone();
                let tab_id_for_upload = tab_id_owned.clone();

                // 多通道上传阈值：10MB
                const MULTI_CHANNEL_THRESHOLD: u64 = 10 * 1024 * 1024;

                // 获取并行通道数设置
                let concurrent_transfers = crate::services::storage::load_settings()
                    .map(|s| s.sftp.concurrent_transfers as usize)
                    .unwrap_or(3);

                // 克隆取消令牌用于上传任务内部
                let cancel_token_for_upload = cancel_token.clone();

                runtime.spawn(async move {
                    let result =
                        if file_size >= MULTI_CHANNEL_THRESHOLD && concurrent_transfers > 1 {
                            // 使用多通道上传
                            info!(
                                "[SFTP] Using multi-channel upload ({} channels) for {:?} ({} bytes)",
                                concurrent_transfers, local_path_clone, file_size
                            );

                            // 获取 SSH session
                            let ssh_manager = crate::ssh::manager::SshManager::global();
                            if let Some(ssh_session) = ssh_manager.get_session(&tab_id_for_upload)
                            {
                                let uploader = crate::services::sftp::MultiChannelUploader::new(
                                    ssh_session,
                                    tab_id_for_upload.clone(),
                                    concurrent_transfers,
                                );

                                let tx_progress_clone = tx_progress.clone();
                                uploader
                                    .upload_file(
                                        &local_path_clone,
                                        &remote_path_clone,
                                        file_size,
                                        cancel_token_for_upload,
                                        pause_flag,
                                        move |transferred, total, speed| {
                                            let _ = tx_progress_clone.send(
                                                UploadEvent::Progress(transferred, total, speed),
                                            );
                                        },
                                    )
                                    .await
                            } else {
                                Err(format!("SSH session not found: {}", tab_id_for_upload))
                            }
                        } else {
                            // 使用单通道上传（小文件或只有1个通道）
                            service
                                .upload_file(
                                    &local_path_clone,
                                    &remote_path_clone,
                                    move |transferred, total, speed| {
                                        let _ = tx_progress.send(UploadEvent::Progress(
                                            transferred,
                                            total,
                                            speed,
                                        ));
                                    },
                                )
                                .await
                        };

                    let _ = tx.send(UploadEvent::Complete(result));
                });

                // 接收进度和结果，同时监听取消信号
                loop {
                    tokio::select! {
                        // 监听取消信号
                        _ = cancel_token.cancelled() => {
                            info!("[SFTP] Upload cancelled by user: {}", transfer_id_clone);
                            // 更新状态
                            let transfer_id = transfer_id_clone.clone();
                            let tab_id = tab_id_owned.clone();
                            let _ = async_cx.update(|cx| {
                                session_state.update(cx, |state, cx| {
                                    if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                        if let Some(transfer) = tab
                                            .active_transfers
                                            .iter_mut()
                                            .find(|t| t.id == transfer_id)
                                        {
                                            transfer.status = crate::models::sftp::TransferStatus::Cancelled;
                                            transfer.error = Some("用户取消".to_string());
                                        }
                                    }
                                    cx.notify();
                                });
                            });
                            break;
                        }
                        // 接收上传事件
                        event = rx.recv() => {
                            match event {
                                Some(UploadEvent::Progress(transferred, total, speed)) => {
                                    let transfer_id = transfer_id_clone.clone();
                                    let tab_id = tab_id_owned.clone();
                                    let _ = async_cx.update(|cx| {
                                        session_state.update(cx, |state, cx| {
                                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                                if let Some(transfer) = tab
                                                    .active_transfers
                                                    .iter_mut()
                                                    .find(|t| t.id == transfer_id)
                                                {
                                                    transfer.progress.bytes_transferred = transferred;
                                                    transfer.progress.total_bytes = total;
                                                    if transfer.status != crate::models::sftp::TransferStatus::Paused {
                                                        transfer.progress.speed_bytes_per_sec = speed;
                                                        if transfer.status == crate::models::sftp::TransferStatus::Pending {
                                                            transfer.status = crate::models::sftp::TransferStatus::Uploading;
                                                        }
                                                    }
                                                }
                                            }
                                            cx.notify();
                                        });
                                    });
                                }
                                Some(UploadEvent::Complete(result)) => {
                                    let transfer_id = transfer_id_clone.clone();
                                    let remote_path = remote_path.clone();
                                    let tab_id = tab_id_owned.clone();
                                    let _ = async_cx.update(|cx| {
                                        session_state.update(cx, |state, cx| {
                                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                                if let Some(transfer) = tab
                                                    .active_transfers
                                                    .iter_mut()
                                                    .find(|t| t.id == transfer_id)
                                                {
                                                    match &result {
                                                        Ok(()) => {
                                                            transfer.set_completed();
                                                            info!(
                                                                "[SFTP] Upload completed: {}",
                                                                remote_path
                                                            );
                                                        }
                                                        Err(e) => {
                                                            transfer.set_failed(e.clone());
                                                            error!("[SFTP] Upload failed: {}", e);
                                                        }
                                                    }
                                                }
                                            }
                                            cx.notify();
                                        });
                                    });
                                    break;
                                }
                                None => {
                                    // Channel closed unexpectedly
                                    break;
                                }
                            }
                        }
                    }
                }
            })
            .detach();
    }

    /// 取消传输任务
    ///
    /// 标记传输为已取消状态并触发取消令牌
    pub fn cancel_transfer(&mut self, transfer_id: &str, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Cancelling transfer: {}", transfer_id);

        // 遍历所有 tab 查找传输任务
        for tab in self.tabs.iter_mut() {
            if let Some(transfer) = tab
                .active_transfers
                .iter_mut()
                .find(|t| t.id == transfer_id)
            {
                // 触发取消令牌
                transfer.cancel_token.cancel();
                // 更新状态
                transfer.status = crate::models::sftp::TransferStatus::Cancelled;
                transfer.error = Some("用户取消".to_string());

                info!("[SFTP] Transfer cancelled: {}", transfer_id);
                cx.notify();
                return;
            }
        }
    }

    /// 暂停传输任务
    pub fn pause_transfer(&mut self, transfer_id: &str, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Pausing transfer: {}", transfer_id);

        // 遍历所有 tab 查找传输任务
        for tab in self.tabs.iter_mut() {
            if let Some(transfer) = tab
                .active_transfers
                .iter_mut()
                .find(|t| t.id == transfer_id)
            {
                if transfer.pause() {
                    info!("[SFTP] Transfer paused: {}", transfer_id);
                    cx.notify();
                } else {
                    info!(
                        "[SFTP] Cannot pause transfer in current state: {}",
                        transfer_id
                    );
                }
                return;
            }
        }
    }

    /// 恢复传输任务
    pub fn resume_transfer(&mut self, transfer_id: &str, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Resuming transfer: {}", transfer_id);

        // 遍历所有 tab 查找传输任务
        for tab in self.tabs.iter_mut() {
            if let Some(transfer) = tab
                .active_transfers
                .iter_mut()
                .find(|t| t.id == transfer_id)
            {
                if transfer.resume() {
                    info!("[SFTP] Transfer resumed: {}", transfer_id);
                    cx.notify();
                } else {
                    info!(
                        "[SFTP] Cannot resume transfer in current state: {}",
                        transfer_id
                    );
                }
                return;
            }
        }
    }

    /// 确保新建文件夹对话框已创建
    pub fn ensure_sftp_new_folder_dialog(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<NewFolderDialogState> {
        if self.sftp_new_folder_dialog.is_none() {
            self.sftp_new_folder_dialog = Some(cx.new(|_| NewFolderDialogState::default()));
        }
        self.sftp_new_folder_dialog.clone().unwrap()
    }

    /// 获取新建文件夹对话框状态（如果存在）
    pub fn get_sftp_new_folder_dialog(&self) -> Option<Entity<NewFolderDialogState>> {
        self.sftp_new_folder_dialog.clone()
    }

    /// 打开新建文件夹对话框
    pub fn sftp_open_new_folder_dialog(&mut self, tab_id: &str, cx: &mut gpui::Context<Self>) {
        // 获取当前路径
        let current_path = self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.sftp_state.as_ref())
            .map(|s| s.current_path.clone())
            .unwrap_or_else(|| "/".to_string());

        let dialog = self.ensure_sftp_new_folder_dialog(cx);
        dialog.update(cx, |s, _| {
            s.open(current_path, tab_id.to_string());
        });
        cx.notify();
    }

    /// 创建新文件夹
    pub fn sftp_create_folder(
        &mut self,
        path: String,
        tab_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let sftp_services = self.sftp_services.clone();
        let session_state = cx.entity().clone();
        let dialog_state = self.sftp_new_folder_dialog.clone();

        // 尝试获取 SFTP 服务
        let service = {
            let guard = match sftp_services.lock() {
                Ok(g) => g,
                Err(e) => {
                    error!("[SFTP] Failed to lock sftp_services: {}", e);
                    if let Some(dialog) = dialog_state {
                        dialog.update(cx, |s, _| {
                            s.set_error(format!("Internal error: {}", e));
                        });
                    }
                    return;
                }
            };
            match guard.get(&tab_id) {
                Some(s) => s.clone(),
                None => {
                    error!("[SFTP] No SFTP service for tab {}", tab_id);
                    if let Some(dialog) = dialog_state {
                        dialog.update(cx, |s, _| {
                            s.set_error("SFTP service not available".to_string());
                        });
                    }
                    return;
                }
            }
        };

        info!("[SFTP] Creating folder: {} for tab {}", path, tab_id);

        // 创建 channel 用于从 tokio 运行时发送结果到 GPUI
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Result<(), String>>();

        // 在 SSH 运行时中执行异步创建
        let ssh_manager = crate::ssh::manager::SshManager::global();
        let path_for_task = path.clone();
        ssh_manager.runtime().spawn(async move {
            let result = service.mkdir(&path_for_task).await;
            let _ = tx.send(result);
        });

        // 在 GPUI 上下文中处理结果
        let path_for_refresh = path.clone();
        let tab_id_for_refresh = tab_id.clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                while let Some(result) = rx.recv().await {
                    let _ = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            match result {
                                Ok(_) => {
                                    info!(
                                        "[SFTP] Folder created successfully: {}",
                                        path_for_refresh
                                    );
                                    // 关闭对话框
                                    if let Some(dialog) = &state.sftp_new_folder_dialog {
                                        dialog.update(cx, |s, _| s.close());
                                    }
                                    // 刷新当前目录
                                    state.sftp_refresh(&tab_id_for_refresh, cx);
                                }
                                Err(e) => {
                                    error!("[SFTP] Failed to create folder: {}", e);
                                    // 显示错误
                                    if let Some(dialog) = &state.sftp_new_folder_dialog {
                                        dialog.update(cx, |s, _| {
                                            s.set_error(e);
                                        });
                                    }
                                }
                            }
                        });
                    });
                }
            })
            .detach();
    }
}
