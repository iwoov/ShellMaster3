//! SFTP file transfer methods for SessionState.
//!
//! This module contains methods for downloading, uploading files, and managing transfer state.

use super::{NewFileDialogState, NewFolderDialogState, PropertiesDialogState, SessionState};
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
                                        let result_clone = result.clone();
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
                                        
                                        // 推送通知
                                        if let Some(window) = cx.active_window() {
                                            use gpui::AppContext as _;
                                            let _ = cx.update_window(window, |_, window, cx| {
                                                use gpui::Styled;
                                                use gpui_component::notification::{
                                                    Notification, NotificationType,
                                                };
                                                use gpui_component::WindowExt;

                                                let lang = crate::services::storage::load_settings()
                                                    .map(|s| s.theme.language)
                                                    .unwrap_or_default();

                                                let notification = match result_clone {
                                                    Ok(()) => Notification::new()
                                                        .message(crate::i18n::t(&lang, "sftp.download.success"))
                                                        .with_type(NotificationType::Success)
                                                        .w_48()
                                                        .py_2(),
                                                    Err(_) => Notification::new()
                                                        .message(crate::i18n::t(&lang, "sftp.download.failed"))
                                                        .with_type(NotificationType::Error)
                                                        .w_48()
                                                        .py_2(),
                                                };
                                                window.push_notification(notification, cx);
                                            });
                                        }
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
                                        let result_clone = result.clone();
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
                                        
                                        // 推送通知
                                        if let Some(window) = cx.active_window() {
                                            use gpui::AppContext as _;
                                            let _ = cx.update_window(window, |_, window, cx| {
                                                use gpui::Styled;
                                                use gpui_component::notification::{
                                                    Notification, NotificationType,
                                                };
                                                use gpui_component::WindowExt;

                                                let lang = crate::services::storage::load_settings()
                                                    .map(|s| s.theme.language)
                                                    .unwrap_or_default();

                                                let notification = match result_clone {
                                                    Ok(()) => Notification::new()
                                                        .message(crate::i18n::t(&lang, "sftp.upload.success"))
                                                        .with_type(NotificationType::Success)
                                                        .w_48()
                                                        .py_2(),
                                                    Err(_) => Notification::new()
                                                        .message(crate::i18n::t(&lang, "sftp.upload.failed"))
                                                        .with_type(NotificationType::Error)
                                                        .w_48()
                                                        .py_2(),
                                                };
                                                window.push_notification(notification, cx);
                                            });
                                        }
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

                // 上传完成后自动刷新目录
                info!("[SFTP] Upload completed, refreshing directory");
                let tab_id_for_refresh = tab_id_owned.clone();
                let _ = async_cx.update(|cx| {
                    session_state.update(cx, |state, cx| {
                        state.sftp_refresh(&tab_id_for_refresh, cx);
                    });
                });
            })
            .detach();
    }

    /// 直接上传单个文件到远程目录（无文件选择器，用于拖放上传）
    pub fn sftp_upload_file_direct(
        &mut self,
        tab_id: &str,
        local_path: std::path::PathBuf,
        remote_dir: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let file_name = local_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
        let file_name = match file_name {
            Some(n) => n,
            None => {
                error!("[SFTP] Invalid file path for upload: {:?}", local_path);
                return;
            }
        };

        let file_size = match std::fs::metadata(&local_path) {
            Ok(m) => m.len(),
            Err(e) => {
                error!("[SFTP] Failed to get file size for {:?}: {}", local_path, e);
                return;
            }
        };

        info!(
            "[SFTP] Direct upload: {:?} -> {}/{} for tab {}",
            local_path, remote_dir, file_name, tab_id
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

        // 使用 GPUI 异步上下文执行上传
        cx.to_async()
            .spawn(async move |async_cx| {
                // 构建远程路径
                let remote_path = if remote_dir == "/" {
                    format!("/{}", file_name)
                } else {
                    format!("{}/{}", remote_dir.trim_end_matches('/'), file_name)
                };

                info!("[SFTP] Uploading {:?} to {}", local_path, remote_path);

                // 创建传输项
                let transfer_item = crate::models::sftp::TransferItem::new_upload(
                    local_path.clone(),
                    remote_path.clone(),
                    file_size,
                );
                let transfer_id = transfer_item.id.clone();

                // 添加传输项到列表
                let tab_id_for_transfer = tab_id_owned.clone();
                let _ = async_cx.update(|cx| {
                    session_state.update(cx, |state, cx| {
                        if let Some(tab) =
                            state.tabs.iter_mut().find(|t| t.id == tab_id_for_transfer)
                        {
                            tab.active_transfers.push(transfer_item);
                        }
                        state.set_sidebar_panel(super::SidebarPanel::Transfer);
                        cx.notify();
                    });
                });

                // 创建 channel 用于进度通知
                enum UploadEvent {
                    Progress(u64, u64, u64),
                    Complete(Result<(), String>),
                }
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UploadEvent>();

                // 在 SSH 运行时中执行上传
                let local_path_clone = local_path.clone();
                let remote_path_clone = remote_path.clone();
                let tx_progress = tx.clone();

                runtime.spawn(async move {
                    let result = service
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
                        .await;

                    let _ = tx.send(UploadEvent::Complete(result.map_err(|e| e.to_string())));
                });

                // 处理上传事件
                let transfer_id_clone = transfer_id.clone();
                let tab_id_for_update = tab_id_owned.clone();
                loop {
                    match rx.recv().await {
                        Some(event) => match event {
                            UploadEvent::Progress(transferred, total, speed) => {
                                let _ = async_cx.update(|cx| {
                                    session_state.update(cx, |state, cx| {
                                        if let Some(tab) = state
                                            .tabs
                                            .iter_mut()
                                            .find(|t| t.id == tab_id_for_update)
                                        {
                                            if let Some(transfer) = tab
                                                .active_transfers
                                                .iter_mut()
                                                .find(|t| t.id == transfer_id_clone)
                                            {
                                                transfer.update_progress(transferred, total, speed);
                                            }
                                        }
                                        cx.notify();
                                    });
                                });
                            }
                            UploadEvent::Complete(result) => {
                                let _ = async_cx.update(|cx| {
                                    let result_clone = result.clone();
                                    session_state.update(cx, |state, cx| {
                                        if let Some(tab) = state
                                            .tabs
                                            .iter_mut()
                                            .find(|t| t.id == tab_id_for_update)
                                        {
                                            if let Some(transfer) = tab
                                                .active_transfers
                                                .iter_mut()
                                                .find(|t| t.id == transfer_id_clone)
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
                                    
                                    // 推送通知
                                    if let Some(window) = cx.active_window() {
                                        use gpui::AppContext as _;
                                        let _ = cx.update_window(window, |_, window, cx| {
                                            use gpui::Styled;
                                            use gpui_component::notification::{
                                                Notification, NotificationType,
                                            };
                                            use gpui_component::WindowExt;

                                            let lang = crate::services::storage::load_settings()
                                                .map(|s| s.theme.language)
                                                .unwrap_or_default();

                                            let notification = match result_clone {
                                                Ok(()) => Notification::new()
                                                    .message(crate::i18n::t(&lang, "sftp.upload.success"))
                                                    .with_type(NotificationType::Success)
                                                    .w_48()
                                                    .py_2(),
                                                Err(_) => Notification::new()
                                                    .message(crate::i18n::t(&lang, "sftp.upload.failed"))
                                                    .with_type(NotificationType::Error)
                                                    .w_48()
                                                    .py_2(),
                                            };
                                            window.push_notification(notification, cx);
                                        });
                                    }
                                });
                                break;
                            }
                        },
                        None => break,
                    }
                }

                // 上传完成后自动刷新目录
                info!("[SFTP] Direct upload completed, refreshing directory");
                let tab_id_for_refresh = tab_id_owned.clone();
                let _ = async_cx.update(|cx| {
                    session_state.update(cx, |state, cx| {
                        state.sftp_refresh(&tab_id_for_refresh, cx);
                    });
                });
            })
            .detach();
    }

    /// 下载远程文件夹到本地（带文件选择器）
    ///
    /// 如果设置了默认下载路径则直接使用，否则打开文件选择器让用户选择保存位置
    pub fn sftp_download_folder_with_picker(
        &mut self,
        tab_id: &str,
        remote_folder: String,
        cx: &mut gpui::Context<Self>,
    ) {
        info!(
            "[SFTP] Download folder with picker: {} for tab {}",
            remote_folder, tab_id
        );

        // 尝试获取默认下载路径
        let default_path = crate::services::storage::load_settings()
            .map(|s| s.sftp.local_default_path.clone())
            .unwrap_or_default();

        let session_state = cx.entity().clone();
        let tab_id_owned = tab_id.to_string();
        let remote_folder_clone = remote_folder.clone();

        // 使用 GPUI 异步上下文执行文件选择
        cx.to_async()
            .spawn(async move |async_cx| {
                // 确定保存路径：优先使用默认路径，否则打开文件选择器
                let local_dir = if !default_path.is_empty() {
                    // 使用默认下载路径
                    let path = std::path::PathBuf::from(&default_path);
                    info!("[SFTP] Using default download path for folder: {:?}", path);
                    path
                } else {
                    // 打开文件夹选择对话框
                    let folder_picker = rfd::AsyncFileDialog::new().set_title("选择下载保存位置");

                    if let Some(folder_handle) = folder_picker.pick_folder().await {
                        folder_handle.path().to_path_buf()
                    } else {
                        info!("[SFTP] Folder download cancelled by user");
                        return;
                    }
                };

                // 在主线程调用下载方法
                let _ = async_cx.update(|cx| {
                    session_state.update(cx, |state, cx| {
                        state.sftp_download_folder(
                            &tab_id_owned,
                            remote_folder_clone,
                            local_dir,
                            cx,
                        );
                    });
                });
            })
            .detach();
    }

    /// 下载远程文件夹到本地（递归）
    ///
    /// 递归遍历远程目录，收集所有文件，然后逐个下载
    pub fn sftp_download_folder(
        &mut self,
        tab_id: &str,
        remote_folder: String,
        local_dir: std::path::PathBuf,
        cx: &mut gpui::Context<Self>,
    ) {
        info!(
            "[SFTP] Download folder: {} -> {:?} for tab {}",
            remote_folder, local_dir, tab_id
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

        // 自动切换到传输面板
        self.set_sidebar_panel(super::SidebarPanel::Transfer);
        cx.notify();

        // 使用 GPUI 异步上下文执行
        cx.to_async()
            .spawn(async move |async_cx| {
                // 1. 递归读取远程目录，获取所有文件
                info!("[SFTP] Collecting files from remote folder: {}", remote_folder);

                // 在 tokio 运行时中执行递归读取
                let (tx_files, mut rx_files) = tokio::sync::mpsc::unbounded_channel();
                let service_for_list = service.clone();
                let remote_folder_clone = remote_folder.clone();

                runtime.spawn(async move {
                    let result = service_for_list.read_dir_recursive(&remote_folder_clone).await;
                    let _ = tx_files.send(result);
                });

                // 等待目录列表结果
                let entries = match rx_files.recv().await {
                    Some(Ok(entries)) => entries,
                    Some(Err(e)) => {
                        error!("[SFTP] Failed to list remote folder: {}", e);
                        return;
                    }
                    None => {
                        error!("[SFTP] Channel closed unexpectedly");
                        return;
                    }
                };

                // 2. 过滤出文件（跳过目录条目）
                let files: Vec<_> = entries
                    .into_iter()
                    .filter(|e| !e.is_dir())
                    .collect();

                if files.is_empty() {
                    info!("[SFTP] No files to download in folder: {}", remote_folder);
                    return;
                }

                info!("[SFTP] Found {} files to download", files.len());

                // 3. 获取远程文件夹的名称作为本地根目录
                let folder_name = remote_folder
                    .rsplit('/')
                    .next()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("download");
                let local_root = local_dir.join(folder_name);

                // 4. 为每个文件创建独立的下载任务
                for file_entry in files {
                    // 计算相对路径
                    let relative_path = file_entry.path
                        .strip_prefix(&remote_folder)
                        .unwrap_or(&file_entry.path)
                        .trim_start_matches('/');

                    let local_file_path = local_root.join(relative_path);

                    // 创建本地父目录
                    if let Some(parent) = local_file_path.parent() {
                        if let Err(e) = std::fs::create_dir_all(parent) {
                            error!("[SFTP] Failed to create local directory {:?}: {}", parent, e);
                            continue;
                        }
                    }

                    // 创建传输项
                    let transfer_item = crate::models::sftp::TransferItem::new_download(
                        file_entry.path.clone(),
                        local_file_path.clone(),
                        file_entry.size,
                    );
                    let transfer_id = transfer_item.id.clone();
                    let cancel_token = transfer_item.cancel_token.clone();

                    // 添加传输项到列表
                    let tab_id_for_add = tab_id_owned.clone();
                    let _ = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_for_add) {
                                tab.active_transfers.push(transfer_item);
                            }
                            cx.notify();
                        });
                    });

                    // 创建事件通道
                    enum DownloadEvent {
                        Progress(u64, u64, u64),
                        Complete(Result<(), String>),
                    }
                    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<DownloadEvent>();

                    // 在 tokio 运行时中启动下载任务
                    let service_for_download = service.clone();
                    let remote_path = file_entry.path.clone();
                    let local_path = local_file_path.clone();
                    let tx_progress = tx.clone();

                    runtime.spawn(async move {
                        let result = service_for_download
                            .download_file(&remote_path, &local_path, move |transferred, total, speed| {
                                let _ = tx_progress.send(DownloadEvent::Progress(transferred, total, speed));
                            })
                            .await;
                        let _ = tx.send(DownloadEvent::Complete(result));
                    });

                    // 处理进度和结果事件 - 这个循环在 GPUI 异步上下文中运行
                    let session_state_for_events = session_state.clone();
                    let tab_id_for_events = tab_id_owned.clone();
                    let transfer_id_for_events = transfer_id.clone();
                    let local_path_for_events = local_file_path.clone();

                    // 由于我们需要并行处理多个文件，我们使用 spawn 来处理每个文件的事件
                    // 注意：这里不能使用 async_cx.clone()，因为它不能跨任务共享
                    // 所以我们在主循环中处理事件
                    loop {
                        tokio::select! {
                            _ = cancel_token.cancelled() => {
                                info!("[SFTP] Download cancelled: {}", transfer_id_for_events);
                                let _ = std::fs::remove_file(&local_path_for_events);
                                let tab_id = tab_id_for_events.clone();
                                let transfer_id = transfer_id_for_events.clone();
                                let _ = async_cx.update(|cx| {
                                    session_state_for_events.update(cx, |state, cx| {
                                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                            if let Some(transfer) = tab.active_transfers.iter_mut().find(|t| t.id == transfer_id) {
                                                transfer.status = crate::models::sftp::TransferStatus::Cancelled;
                                                transfer.error = Some("用户取消".to_string());
                                            }
                                        }
                                        cx.notify();
                                    });
                                });
                                break;
                            }
                            event = rx.recv() => {
                                match event {
                                    Some(DownloadEvent::Progress(transferred, total, speed)) => {
                                        let tab_id = tab_id_for_events.clone();
                                        let transfer_id = transfer_id_for_events.clone();
                                        let _ = async_cx.update(|cx| {
                                            session_state_for_events.update(cx, |state, cx| {
                                                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                                    if let Some(transfer) = tab.active_transfers.iter_mut().find(|t| t.id == transfer_id) {
                                                        transfer.update_progress(transferred, total, speed);
                                                    }
                                                }
                                                cx.notify();
                                            });
                                        });
                                    }
                                    Some(DownloadEvent::Complete(result)) => {
                                        let tab_id = tab_id_for_events.clone();
                                        let transfer_id = transfer_id_for_events.clone();
                                        let local_path = local_path_for_events.clone();
                                        let _ = async_cx.update(|cx| {
                                            let result_clone = result.clone();
                                            session_state_for_events.update(cx, |state, cx| {
                                                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                                    if let Some(transfer) = tab.active_transfers.iter_mut().find(|t| t.id == transfer_id) {
                                                        match &result {
                                                            Ok(()) => {
                                                                transfer.set_completed();
                                                                info!("[SFTP] Download completed: {:?}", local_path);
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
                                            
                                            // 推送通知
                                            if let Some(window) = cx.active_window() {
                                                use gpui::AppContext as _;
                                                let _ = cx.update_window(window, |_, window, cx| {
                                                    use gpui::Styled;
                                                    use gpui_component::notification::{
                                                        Notification, NotificationType,
                                                    };
                                                    use gpui_component::WindowExt;

                                                    let lang = crate::services::storage::load_settings()
                                                        .map(|s| s.theme.language)
                                                        .unwrap_or_default();

                                                    let notification = match result_clone {
                                                        Ok(()) => Notification::new()
                                                            .message(crate::i18n::t(&lang, "sftp.download.success"))
                                                            .with_type(NotificationType::Success)
                                                            .w_48()
                                                            .py_2(),
                                                        Err(_) => Notification::new()
                                                            .message(crate::i18n::t(&lang, "sftp.download.failed"))
                                                            .with_type(NotificationType::Error)
                                                            .w_48()
                                                            .py_2(),
                                                    };
                                                    window.push_notification(notification, cx);
                                                });
                                            }
                                        });
                                        break;
                                    }
                                    None => {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            })
            .detach();
    }

    /// 上传本地文件夹到远程服务器（带文件选择器）
    ///
    /// 打开文件选择器让用户选择要上传的文件夹，然后调用 sftp_upload_folder 执行上传
    pub fn sftp_upload_folder_with_picker(
        &mut self,
        tab_id: &str,
        remote_dir: String,
        cx: &mut gpui::Context<Self>,
    ) {
        info!(
            "[SFTP] Upload folder with picker: remote dir {} for tab {}",
            remote_dir, tab_id
        );

        let session_state = cx.entity().clone();
        let tab_id_owned = tab_id.to_string();
        let remote_dir_clone = remote_dir.clone();

        // 使用 GPUI 异步上下文执行文件选择
        cx.to_async()
            .spawn(async move |async_cx| {
                // 打开文件夹选择对话框
                let folder_picker = rfd::AsyncFileDialog::new().set_title("选择要上传的文件夹");

                if let Some(folder_handle) = folder_picker.pick_folder().await {
                    let local_folder = folder_handle.path().to_path_buf();

                    // 在主线程调用上传方法
                    let _ = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            state.sftp_upload_folder(
                                &tab_id_owned,
                                local_folder,
                                remote_dir_clone,
                                cx,
                            );
                        });
                    });
                } else {
                    info!("[SFTP] Folder upload cancelled by user");
                }
            })
            .detach();
    }

    /// 上传本地文件夹到远程服务器（递归）
    ///
    /// 递归遍历本地目录，收集所有文件，然后逐个上传
    pub fn sftp_upload_folder(
        &mut self,
        tab_id: &str,
        local_folder: std::path::PathBuf,
        remote_dir: String,
        cx: &mut gpui::Context<Self>,
    ) {
        info!(
            "[SFTP] Upload folder: {:?} -> {} for tab {}",
            local_folder, remote_dir, tab_id
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

        // 自动切换到传输面板
        self.set_sidebar_panel(super::SidebarPanel::Transfer);
        cx.notify();

        // 使用 GPUI 异步上下文执行
        cx.to_async()
            .spawn(async move |async_cx| {
                // 1. 递归遍历本地文件夹，收集所有文件
                info!("[SFTP] Collecting files from local folder: {:?}", local_folder);

                fn collect_local_files(
                    dir: &std::path::Path,
                    base: &std::path::Path,
                ) -> std::io::Result<Vec<std::path::PathBuf>> {
                    let mut files = Vec::new();

                    for entry in std::fs::read_dir(dir)? {
                        let entry = entry?;
                        let path = entry.path();

                        if path.is_dir() {
                            files.extend(collect_local_files(&path, base)?);
                        } else if path.is_file() {
                            files.push(path);
                        }
                    }

                    Ok(files)
                }

                let files = match collect_local_files(&local_folder, &local_folder) {
                    Ok(f) => f,
                    Err(e) => {
                        error!("[SFTP] Failed to collect local files: {}", e);
                        return;
                    }
                };

                if files.is_empty() {
                    info!("[SFTP] No files to upload in folder: {:?}", local_folder);
                    return;
                }

                info!("[SFTP] Found {} files to upload", files.len());

                // 2. 获取本地文件夹的名称作为远程根目录
                let folder_name = local_folder
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "upload".to_string());
                let remote_root = if remote_dir == "/" {
                    format!("/{}", folder_name)
                } else {
                    format!("{}/{}", remote_dir.trim_end_matches('/'), folder_name)
                };

                // 3. 收集需要创建的远程目录
                let mut remote_dirs_to_create = std::collections::HashSet::new();
                for file in &files {
                    if let Ok(relative) = file.strip_prefix(&local_folder) {
                        if let Some(parent) = relative.parent() {
                            if !parent.as_os_str().is_empty() {
                                let remote_parent = format!(
                                    "{}/{}",
                                    remote_root,
                                    parent.to_string_lossy().replace("\\", "/")
                                );
                                remote_dirs_to_create.insert(remote_parent);
                            }
                        }
                    }
                }

                // 确保远程根目录存在
                remote_dirs_to_create.insert(remote_root.clone());

                // 4. 在远程创建目录结构
                let mut sorted_dirs: Vec<_> = remote_dirs_to_create.into_iter().collect();
                sorted_dirs.sort_by_key(|p| p.matches('/').count()); // 按深度排序

                for dir in sorted_dirs {
                    let service_clone = service.clone();
                    let dir_clone = dir.clone();
                    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

                    runtime.spawn(async move {
                        let result = service_clone.mkdir_recursive(&dir_clone).await;
                        let _ = tx.send(result);
                    });

                    // 等待目录创建结果
                    if let Some(result) = rx.recv().await {
                        if let Err(e) = result {
                            error!("[SFTP] Failed to create remote directory {}: {}", dir, e);
                            // 继续尝试其他目录
                        }
                    }
                }

                // 5. 为每个文件创建独立的上传任务
                for local_file_path in files {
                    // 计算相对路径和远程路径
                    let relative_path = match local_file_path.strip_prefix(&local_folder) {
                        Ok(r) => r.to_string_lossy().replace("\\", "/"),
                        Err(_) => continue,
                    };

                    let remote_path = format!("{}/{}", remote_root, relative_path);

                    // 获取文件大小
                    let file_size = match std::fs::metadata(&local_file_path) {
                        Ok(m) => m.len(),
                        Err(e) => {
                            error!("[SFTP] Failed to get file size for {:?}: {}", local_file_path, e);
                            continue;
                        }
                    };

                    // 创建传输项
                    let transfer_item = crate::models::sftp::TransferItem::new_upload(
                        local_file_path.clone(),
                        remote_path.clone(),
                        file_size,
                    );
                    let transfer_id = transfer_item.id.clone();
                    let cancel_token = transfer_item.cancel_token.clone();

                    // 添加传输项到列表
                    let tab_id_for_add = tab_id_owned.clone();
                    let _ = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_for_add) {
                                tab.active_transfers.push(transfer_item);
                            }
                            cx.notify();
                        });
                    });

                    // 创建事件通道
                    enum UploadEvent {
                        Progress(u64, u64, u64),
                        Complete(Result<(), String>),
                    }
                    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<UploadEvent>();

                    // 在 tokio 运行时中启动上传任务
                    let service_for_upload = service.clone();
                    let local_path = local_file_path.clone();
                    let remote = remote_path.clone();
                    let tx_progress = tx.clone();

                    runtime.spawn(async move {
                        let result = service_for_upload
                            .upload_file(&local_path, &remote, move |transferred, total, speed| {
                                let _ = tx_progress.send(UploadEvent::Progress(transferred, total, speed));
                            })
                            .await;
                        let _ = tx.send(UploadEvent::Complete(result));
                    });

                    // 处理进度和结果事件 - 这个循环在 GPUI 异步上下文中运行
                    let session_state_for_events = session_state.clone();
                    let tab_id_for_events = tab_id_owned.clone();
                    let transfer_id_for_events = transfer_id.clone();

                    loop {
                        tokio::select! {
                            _ = cancel_token.cancelled() => {
                                info!("[SFTP] Upload cancelled: {}", transfer_id_for_events);
                                let tab_id = tab_id_for_events.clone();
                                let transfer_id = transfer_id_for_events.clone();
                                let _ = async_cx.update(|cx| {
                                    session_state_for_events.update(cx, |state, cx| {
                                        if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                            if let Some(transfer) = tab.active_transfers.iter_mut().find(|t| t.id == transfer_id) {
                                                transfer.status = crate::models::sftp::TransferStatus::Cancelled;
                                                transfer.error = Some("用户取消".to_string());
                                            }
                                        }
                                        cx.notify();
                                    });
                                });
                                break;
                            }
                            event = rx.recv() => {
                                match event {
                                    Some(UploadEvent::Progress(transferred, total, speed)) => {
                                        let tab_id = tab_id_for_events.clone();
                                        let transfer_id = transfer_id_for_events.clone();
                                        let _ = async_cx.update(|cx| {
                                            session_state_for_events.update(cx, |state, cx| {
                                                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                                    if let Some(transfer) = tab.active_transfers.iter_mut().find(|t| t.id == transfer_id) {
                                                        transfer.update_progress(transferred, total, speed);
                                                    }
                                                }
                                                cx.notify();
                                            });
                                        });
                                    }
                                    Some(UploadEvent::Complete(result)) => {
                                        let tab_id = tab_id_for_events.clone();
                                        let transfer_id = transfer_id_for_events.clone();
                                        let _ = async_cx.update(|cx| {
                                            let result_clone = result.clone();
                                            session_state_for_events.update(cx, |state, cx| {
                                                if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id) {
                                                    if let Some(transfer) = tab.active_transfers.iter_mut().find(|t| t.id == transfer_id) {
                                                        match &result {
                                                            Ok(()) => {
                                                                transfer.set_completed();
                                                                info!("[SFTP] Upload completed: {}", remote_path);
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
                                            
                                            // 推送通知
                                            if let Some(window) = cx.active_window() {
                                                use gpui::AppContext as _;
                                                let _ = cx.update_window(window, |_, window, cx| {
                                                    use gpui::Styled;
                                                    use gpui_component::notification::{
                                                        Notification, NotificationType,
                                                    };
                                                    use gpui_component::WindowExt;

                                                    let lang = crate::services::storage::load_settings()
                                                        .map(|s| s.theme.language)
                                                        .unwrap_or_default();

                                                    let notification = match result_clone {
                                                        Ok(()) => Notification::new()
                                                            .message(crate::i18n::t(&lang, "sftp.upload.success"))
                                                            .with_type(NotificationType::Success)
                                                            .w_48()
                                                            .py_2(),
                                                        Err(_) => Notification::new()
                                                            .message(crate::i18n::t(&lang, "sftp.upload.failed"))
                                                            .with_type(NotificationType::Error)
                                                            .w_48()
                                                            .py_2(),
                                                    };
                                                    window.push_notification(notification, cx);
                                                });
                                            }
                                        });
                                        break;
                                    }
                                    None => {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }

                // 所有文件上传完成后自动刷新目录
                info!("[SFTP] All folder uploads completed, refreshing directory");
                let tab_id_for_refresh = tab_id_owned.clone();
                let _ = async_cx.update(|cx| {
                    session_state.update(cx, |state, cx| {
                        state.sftp_refresh(&tab_id_for_refresh, cx);
                    });
                });
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
                    let result_clone = result.clone();
                    let _ = async_cx.update(|cx| {
                        // 先更新 state
                        session_state.update(cx, |state, cx| {
                            match &result_clone {
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
                                            s.set_error(e.clone());
                                        });
                                    }
                                }
                            }
                        });

                        // 推送失败通知（成功时不通知，用户可通过文件列表刷新看到）
                        if result_clone.is_err() {
                            if let Some(window) = cx.active_window() {
                                use gpui::AppContext as _;
                                let _ = cx.update_window(window, |_, window, cx| {
                                    use gpui::Styled;
                                    use gpui_component::notification::{
                                        Notification, NotificationType,
                                    };
                                    use gpui_component::WindowExt;

                                    let lang = crate::services::storage::load_settings()
                                        .map(|s| s.theme.language)
                                        .unwrap_or_default();

                                    let notification = Notification::new()
                                        .message(crate::i18n::t(&lang, "sftp.new_folder.failed"))
                                        .with_type(NotificationType::Error)
                                        .w_48()
                                        .py_2();
                                    window.push_notification(notification, cx);
                                });
                            }
                        }
                    });
                }
            })
            .detach();
    }

    // ============ 新建文件对话框 ============

    /// 确保新建文件对话框状态已创建
    pub fn ensure_sftp_new_file_dialog(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<NewFileDialogState> {
        if self.sftp_new_file_dialog.is_none() {
            self.sftp_new_file_dialog = Some(cx.new(|_| NewFileDialogState::default()));
        }
        self.sftp_new_file_dialog.clone().unwrap()
    }

    /// 获取新建文件对话框状态（如果存在）
    pub fn get_sftp_new_file_dialog(&self) -> Option<Entity<NewFileDialogState>> {
        self.sftp_new_file_dialog.clone()
    }

    /// 打开新建文件对话框
    pub fn sftp_open_new_file_dialog(&mut self, tab_id: &str, cx: &mut gpui::Context<Self>) {
        // 获取当前路径
        let current_path = self
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.sftp_state.as_ref())
            .map(|s| s.current_path.clone())
            .unwrap_or_else(|| "/".to_string());

        let dialog = self.ensure_sftp_new_file_dialog(cx);
        dialog.update(cx, |s, _| {
            s.open(current_path, tab_id.to_string());
        });
        cx.notify();
    }

    /// 创建新文件
    pub fn sftp_create_file(
        &mut self,
        path: String,
        tab_id: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let sftp_services = self.sftp_services.clone();
        let session_state = cx.entity().clone();
        let dialog_state = self.sftp_new_file_dialog.clone();

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

        info!("[SFTP] Creating file: {} for tab {}", path, tab_id);

        // 创建 channel 用于从 tokio 运行时发送结果到 GPUI
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Result<(), String>>();

        // 在 SSH 运行时中执行异步创建
        let ssh_manager = crate::ssh::manager::SshManager::global();
        let path_for_task = path.clone();
        ssh_manager.runtime().spawn(async move {
            let result = service.create_file(&path_for_task).await;
            let _ = tx.send(result);
        });

        // 处理结果
        let tab_id_for_result = tab_id.clone();
        let dialog_for_result = self.sftp_new_file_dialog.clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                if let Some(result) = rx.recv().await {
                    let _ = async_cx.update(|cx| {
                        // 更新对话框状态
                        if let Some(dialog) = dialog_for_result.clone() {
                            dialog.update(cx, |s, _| match &result {
                                Ok(()) => s.close(),
                                Err(e) => s.set_error(e.clone()),
                            });
                        }

                        // 成功后刷新目录
                        if result.is_ok() {
                            session_state.update(cx, |state, cx| {
                                state.sftp_refresh(&tab_id_for_result, cx);
                            });
                        }

                        // 推送失败通知（成功时不通知，用户可通过文件列表刷新看到）
                        if result.is_err() {
                            if let Some(window) = cx.active_window() {
                                use gpui::AppContext as _;
                                let _ = cx.update_window(window, |_, window, cx| {
                                    use gpui::Styled;
                                    use gpui_component::notification::{
                                        Notification, NotificationType,
                                    };
                                    use gpui_component::WindowExt;

                                    let lang = crate::services::storage::load_settings()
                                        .map(|s| s.theme.language)
                                        .unwrap_or_default();

                                    let notification = Notification::new()
                                        .message(crate::i18n::t(&lang, "sftp.new_file.failed"))
                                        .with_type(NotificationType::Error)
                                        .w_48()
                                        .py_2();
                                    window.push_notification(notification, cx);
                                });
                            }
                        }
                    });
                }
            })
            .detach();
    }

    // ============ 属性对话框 ============

    /// 确保属性对话框状态已创建
    pub fn ensure_sftp_properties_dialog(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<PropertiesDialogState> {
        if self.sftp_properties_dialog.is_none() {
            self.sftp_properties_dialog = Some(cx.new(|_| PropertiesDialogState::default()));
        }
        self.sftp_properties_dialog.clone().unwrap()
    }

    /// 获取属性对话框状态
    pub fn get_sftp_properties_dialog(&self) -> Option<Entity<PropertiesDialogState>> {
        self.sftp_properties_dialog.clone()
    }

    /// 打开属性对话框
    pub fn sftp_open_properties_dialog(
        &mut self,
        tab_id: &str,
        path: String,
        cx: &mut gpui::Context<Self>,
    ) {
        info!("[SFTP] Open properties dialog for: {} in tab {}", path, tab_id);

        // 从 file_list 中查找对应的 FileEntry
        let entry = {
            let tab = self.tabs.iter().find(|t| t.id == tab_id);
            if let Some(tab) = tab {
                if let Some(sftp_state) = &tab.sftp_state {
                    sftp_state.file_list.iter().find(|e| e.path == path).cloned()
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(entry) = entry {
            let dialog = self.ensure_sftp_properties_dialog(cx);
            let entry_clone = entry.clone();
            dialog.update(cx, |d, _| {
                d.open(entry_clone, tab_id.to_string());
            });

            // 如果是符号链接，异步获取链接目标
            if entry.file_type == crate::models::sftp::FileType::Symlink {
                self.sftp_fetch_symlink_target(tab_id, &path, dialog.clone(), cx);
            }

            // 如果是文件夹，异步计算总大小（通过 SSH du 命令）
            if entry.is_dir() {
                self.sftp_calculate_folder_size(tab_id, &path, dialog.clone(), cx);
            }

            cx.notify();
        } else {
            error!("[SFTP] File entry not found for path: {}", path);
        }
    }

    /// 获取符号链接目标
    fn sftp_fetch_symlink_target(
        &self,
        tab_id: &str,
        path: &str,
        dialog: Entity<PropertiesDialogState>,
        cx: &mut gpui::Context<Self>,
    ) {
        let sftp_services = self.sftp_services.clone();
        let tab_id_owned = tab_id.to_string();
        let path_owned = path.to_string();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Result<String, String>>();

        let ssh_manager = crate::ssh::manager::SshManager::global();

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

        let path_clone = path_owned.clone();
        ssh_manager.runtime().spawn(async move {
            let result = service.read_link(&path_clone).await;
            let _ = tx.send(result);
        });

        cx.to_async()
            .spawn(async move |async_cx| {
                if let Some(result) = rx.recv().await {
                    let _ = async_cx.update(|cx| {
                        if let Ok(target) = result {
                            dialog.update(cx, |d, _| {
                                d.set_symlink_target(target);
                            });
                        }
                    });
                }
            })
            .detach();
    }

    /// 计算文件夹大小（通过 SSH du 命令）
    pub fn sftp_calculate_folder_size(
        &mut self,
        tab_id: &str,
        path: &str,
        dialog: Entity<PropertiesDialogState>,
        cx: &mut gpui::Context<Self>,
    ) {
        info!("[SFTP] Calculating folder size for: {} in tab {}", path, tab_id);

        // 标记正在计算，并获取取消令牌
        let cancellation_token = dialog.update(cx, |d, _| {
            d.start_calculating_size()
        });

        let tab_id_owned = tab_id.to_string();
        let path_owned = path.to_string();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Result<u64, String>>();

        let ssh_manager = crate::ssh::manager::SshManager::global();

        // 获取 SSH session 来执行命令
        let session = match ssh_manager.get_session(&tab_id_owned) {
            Some(s) => s,
            None => {
                error!("[SFTP] No SSH session for tab {}", tab_id_owned);
                return;
            }
        };

        let path_clone = path_owned.clone();
        let token_for_task = cancellation_token.clone();
        ssh_manager.runtime().spawn(async move {
            // 检查是否已取消
            if token_for_task.is_cancelled() {
                info!("[SFTP] Folder size calculation cancelled before start");
                return;
            }

            // 打开 exec 通道
            let exec_channel = match session.open_exec().await {
                Ok(ch) => ch,
                Err(e) => {
                    let _ = tx.send(Err(format!("Failed to open exec channel: {:?}", e)));
                    return;
                }
            };

            // 再次检查是否已取消
            if token_for_task.is_cancelled() {
                info!("[SFTP] Folder size calculation cancelled");
                return;
            }

            // 执行 du 命令获取文件夹大小（字节）
            // 使用 du -sb 获取总字节数，2>/dev/null 忽略权限错误
            let command = format!("du -sb '{}' 2>/dev/null | cut -f1", path_clone.replace("'", "'\\''"));
            
            match exec_channel.exec(&command).await {
                Ok(output) => {
                    // 检查是否已取消
                    if token_for_task.is_cancelled() {
                        info!("[SFTP] Folder size calculation cancelled after exec");
                        return;
                    }

                    if output.exit_code == 0 {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let size_str = stdout.trim();
                        match size_str.parse::<u64>() {
                            Ok(size) => {
                                info!("[SFTP] Folder size calculated: {} bytes", size);
                                let _ = tx.send(Ok(size));
                            }
                            Err(_) => {
                                let _ = tx.send(Err(format!("Failed to parse size: {}", size_str)));
                            }
                        }
                    } else {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        let _ = tx.send(Err(format!("du command failed: {}", stderr)));
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(format!("Failed to execute du: {:?}", e)));
                }
            }
        });

        let token_for_ui = cancellation_token.clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                // 使用 tokio::select! 来同时监听结果和取消信号
                tokio::select! {
                    _ = token_for_ui.cancelled() => {
                        info!("[SFTP] Folder size UI update cancelled");
                    }
                    result = rx.recv() => {
                        if let Some(result) = result {
                            let _ = async_cx.update(|cx| {
                                match result {
                                    Ok(size) => {
                                        dialog.update(cx, |d, _| {
                                            d.set_folder_size(size);
                                        });
                                    }
                                    Err(e) => {
                                        error!("[SFTP] Folder size calculation failed: {}", e);
                                        // 设置为 0 表示计算失败
                                        dialog.update(cx, |d, _| {
                                            d.set_folder_size(0);
                                        });
                                    }
                                }
                            });
                        }
                    }
                }
            })
            .detach();
    }
}

