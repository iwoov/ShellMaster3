// 应用自动更新 GPUI 状态管理

use gpui_kit::*;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{info, warn};

use crate::i18n;
use crate::services::storage;
use crate::services::updater::{
    self, InstallOutcome, ReleaseInfo, UpdateStatus, CURRENT_VERSION,
};

enum DownloadEvent {
    Progress { downloaded: u64, total: Option<u64> },
    Done(Result<PathBuf, String>),
}

/// 全局共享的应用更新状态（在 HomePage、侧边栏、设置弹窗之间共享）
#[derive(Default)]
pub struct UpdateState {
    pub status: UpdateStatus,
    pub startup_checked: bool,
}

impl UpdateState {
    /// 当前是否处于进行中状态（检查、下载或安装中）
    pub fn is_busy(&self) -> bool {
        matches!(
            self.status,
            UpdateStatus::Checking
                | UpdateStatus::Downloading { .. }
                | UpdateStatus::Installing { .. }
        )
    }

    /// 推送顶部通知（静默后台流程使用）
    fn push_toast(
        window_hint: Option<AnyWindowHandle>,
        message: String,
        kind: gpui_kit::component::notification::NotificationType,
        cx: &mut App,
    ) {
        let Some(handle) = window_hint.or_else(|| cx.active_window()) else {
            return;
        };
        let _ = cx.update_window(handle, |_, window, cx| {
            use gpui_kit::component::notification::Notification;
            use gpui_kit::component::WindowExt;
            let notification = Notification::new()
                .message(message)
                .with_type(kind)
                .w(px(340.))
                .py_2();
            window.push_notification(notification, cx);
        });
    }

    /// 启动后延迟执行自动检查更新（仅触发一次）
    pub fn schedule_startup_check(
        state: Entity<Self>,
        window: AnyWindowHandle,
        cx: &mut App,
    ) {
        let already = state.read(cx).startup_checked;
        if already {
            return;
        }
        state.update(cx, |s, _| {
            s.startup_checked = true;
        });

        let settings = storage::cached_settings();
        if !settings.system.check_updates {
            return;
        }

        cx.spawn({
            let state = state.clone();
            async move |async_cx| {
                async_cx
                    .background_executor()
                    .timer(Duration::from_secs(2))
                    .await;
                let _ = async_cx.update(|cx| {
                    let latest_settings = storage::cached_settings();
                    if !latest_settings.system.check_updates {
                        return;
                    }
                    Self::check_for_updates(
                        state,
                        latest_settings.system.auto_update,
                        true,
                        Some(window),
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    /// 向 GitHub Releases 检查更新
    ///
    /// - `auto_download`: 发现新版本且匹配到当前平台安装包时是否自动开始下载
    /// - `silent`: true 表示启动后台检查（网络错误不显示在 UI 中，仅打日志，发现更新/下载完成时推送通知）
    pub fn check_for_updates(
        state: Entity<Self>,
        auto_download: bool,
        silent: bool,
        window_hint: Option<AnyWindowHandle>,
        cx: &mut App,
    ) {
        if state.read(cx).is_busy() {
            return;
        }

        state.update(cx, |s, cx| {
            s.status = UpdateStatus::Checking;
            cx.notify();
        });

        let (tx, rx) = tokio::sync::oneshot::channel();
        let runtime = crate::ssh::manager::SshManager::global().runtime();
        runtime.spawn(async move {
            let result = updater::fetch_latest_release().await;
            let _ = tx.send(result);
        });

        cx.spawn({
            let state = state.clone();
            async move |async_cx| {
                let result = rx.await;
                let _ = async_cx.update(|cx| match result {
                    Ok(Ok(release)) => {
                        if updater::is_newer_version(CURRENT_VERSION, &release.version) {
                            info!(
                                "[Updater] 发现新版本: {} (当前 v{})",
                                release.tag_name, CURRENT_VERSION
                            );
                            if auto_download && release.asset.is_some() {
                                // 自动下载模式：后台流式下载完成后提示安装并重启（不强行中断正在进行的 SSH 会话）
                                Self::start_download(
                                    state,
                                    release,
                                    false,
                                    silent,
                                    window_hint,
                                    cx,
                                );
                            } else {
                                let tag = release.tag_name.clone();
                                state.update(cx, |s, cx| {
                                    s.status = UpdateStatus::Available(release);
                                    cx.notify();
                                });
                                if silent {
                                    let lang = storage::cached_settings().theme.language.clone();
                                    let msg = format!(
                                        "{} {}",
                                        i18n::t(&lang, "settings.update.toast_available"),
                                        tag
                                    );
                                    Self::push_toast(
                                        window_hint,
                                        msg,
                                        gpui_kit::component::notification::NotificationType::Info,
                                        cx,
                                    );
                                }
                            }
                        } else {
                            let checked_at = chrono::Local::now().format("%H:%M:%S").to_string();
                            state.update(cx, |s, cx| {
                                s.status = UpdateStatus::UpToDate { checked_at };
                                cx.notify();
                            });
                        }
                    }
                    Ok(Err(err)) => {
                        if silent {
                            warn!("[Updater] 启动后台检查更新失败: {}", err);
                            state.update(cx, |s, cx| {
                                s.status = UpdateStatus::Idle;
                                cx.notify();
                            });
                        } else {
                            state.update(cx, |s, cx| {
                                s.status = UpdateStatus::Error(err);
                                cx.notify();
                            });
                        }
                    }
                    Err(_) => {
                        state.update(cx, |s, cx| {
                            s.status = if silent {
                                UpdateStatus::Idle
                            } else {
                                UpdateStatus::Error("更新检查任务意外中断".to_string())
                            };
                            cx.notify();
                        });
                    }
                });
            }
        })
        .detach();
    }

    /// 流式下载安装包；`install_after_download` 为 true 时下载完成后直接执行安装并重启
    pub fn start_download(
        state: Entity<Self>,
        release: ReleaseInfo,
        install_after_download: bool,
        silent: bool,
        window_hint: Option<AnyWindowHandle>,
        cx: &mut App,
    ) {
        let Some(asset) = release.asset.clone() else {
            state.update(cx, |s, cx| {
                s.status = UpdateStatus::Error(
                    "当前系统或架构暂无对应安装包，请前往 GitHub 发布页手动下载".to_string(),
                );
                cx.notify();
            });
            return;
        };

        let total_hint = (asset.size > 0).then_some(asset.size);
        let release_for_state = release.clone();
        state.update(cx, |s, cx| {
            s.status = UpdateStatus::Downloading {
                release: release_for_state,
                downloaded: 0,
                total: total_hint,
            };
            cx.notify();
        });

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<DownloadEvent>();
        let runtime = crate::ssh::manager::SshManager::global().runtime();
        runtime.spawn(async move {
            let tx_progress = tx.clone();
            let result = updater::download_release_asset(&asset, move |downloaded, total| {
                let _ = tx_progress.send(DownloadEvent::Progress { downloaded, total });
            })
            .await;
            let _ = tx.send(DownloadEvent::Done(result));
        });

        cx.spawn({
            let state = state.clone();
            async move |async_cx| {
                while let Some(ev) = rx.recv().await {
                    let mut finished = false;
                    let _ = async_cx.update(|cx| match ev {
                        DownloadEvent::Progress { downloaded, total } => {
                            state.update(cx, |s, cx| {
                                if let UpdateStatus::Downloading {
                                    downloaded: ref mut d,
                                    total: ref mut t,
                                    ..
                                } = s.status
                                {
                                    *d = downloaded;
                                    *t = total;
                                    cx.notify();
                                }
                            });
                        }
                        DownloadEvent::Done(Ok(file_path)) => {
                            finished = true;
                            info!(
                                "[Updater] 安装包下载完成: {} -> {}",
                                release.tag_name,
                                file_path.display()
                            );
                            if install_after_download {
                                Self::install_downloaded(state.clone(), release.clone(), file_path, cx);
                            } else {
                                let tag = release.tag_name.clone();
                                state.update(cx, |s, cx| {
                                    s.status = UpdateStatus::Downloaded {
                                        release: release.clone(),
                                        file_path,
                                    };
                                    cx.notify();
                                });
                                if silent {
                                    let lang = storage::cached_settings().theme.language.clone();
                                    let msg = format!(
                                        "{} {}",
                                        tag,
                                        i18n::t(&lang, "settings.update.toast_ready")
                                    );
                                    Self::push_toast(
                                        window_hint,
                                        msg,
                                        gpui_kit::component::notification::NotificationType::Success,
                                        cx,
                                    );
                                }
                            }
                        }
                        DownloadEvent::Done(Err(err)) => {
                            finished = true;
                            if silent {
                                warn!("[Updater] 后台自动下载更新失败: {}", err);
                                // 退回到 Available 状态，允许用户在设置中重试
                                state.update(cx, |s, cx| {
                                    s.status = UpdateStatus::Available(release.clone());
                                    cx.notify();
                                });
                            } else {
                                state.update(cx, |s, cx| {
                                    s.status = UpdateStatus::Error(err);
                                    cx.notify();
                                });
                            }
                        }
                    });
                    if finished {
                        break;
                    }
                }
            }
        })
        .detach();
    }

    /// 安装已下载的安装包并在支持无缝替换的平台重启应用
    pub fn install_downloaded(
        state: Entity<Self>,
        release: ReleaseInfo,
        file_path: PathBuf,
        cx: &mut App,
    ) {
        state.update(cx, |s, cx| {
            s.status = UpdateStatus::Installing {
                release: release.clone(),
            };
            cx.notify();
        });

        let (tx, rx) = tokio::sync::oneshot::channel();
        let runtime = crate::ssh::manager::SshManager::global().runtime();
        runtime.spawn(async move {
            let result = tokio::task::spawn_blocking(move || updater::install_update(&file_path))
                .await
                .unwrap_or_else(|e| Err(format!("安装任务执行异常: {}", e)));
            let _ = tx.send(result);
        });

        cx.spawn({
            let state = state.clone();
            async move |async_cx| {
                let result = rx.await;
                let _ = async_cx.update(|cx| match result {
                    Ok(Ok(InstallOutcome::Restarting)) => {
                        info!("[Updater] 更新脚本已就绪，正在退出当前进程以完成重启");
                        cx.quit();
                    }
                    Ok(Ok(InstallOutcome::OpenedInstaller { message })) => {
                        state.update(cx, |s, cx| {
                            s.status = UpdateStatus::InstalledOpened { message };
                            cx.notify();
                        });
                    }
                    Ok(Err(err)) => {
                        state.update(cx, |s, cx| {
                            s.status = UpdateStatus::Error(err);
                            cx.notify();
                        });
                    }
                    Err(_) => {
                        state.update(cx, |s, cx| {
                            s.status = UpdateStatus::Error("安装任务意外中断".to_string());
                            cx.notify();
                        });
                    }
                });
            }
        })
        .detach();
    }
}
