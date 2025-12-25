// 外置编辑器服务 - 临时文件管理和文件监控

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::SystemTime;
use tracing::{error, info};

// ======================== 临时文件管理 ========================

/// 获取临时编辑目录
/// 使用系统临时目录，各平台：
/// - macOS: /private/var/folders/.../T/shellmaster/edit/ (规范化后)
/// - Linux: /tmp/shellmaster/edit/
/// - Windows: C:\Users\<User>\AppData\Local\Temp\shellmaster\edit\
pub fn get_temp_edit_dir() -> PathBuf {
    let base = std::env::temp_dir();
    // macOS 上 /var 是 /private/var 的符号链接
    // notify crate 返回真实路径，所以需要规范化
    let base = base.canonicalize().unwrap_or(base);
    base.join("shellmaster").join("edit")
}

/// 生成临时文件路径
/// 格式: {session_id}_{remote_path_hash}_{filename}
/// 返回规范化的绝对路径（解析符号链接）
pub fn temp_file_path(session_id: &str, remote_path: &str) -> PathBuf {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    remote_path.hash(&mut hasher);
    let hash = hasher.finish();

    let filename = Path::new(remote_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    get_temp_edit_dir().join(format!("{}_{}_{}", session_id, hash, filename))
}

/// 确保临时目录存在
pub fn ensure_temp_dir() -> std::io::Result<()> {
    std::fs::create_dir_all(get_temp_edit_dir())
}

// ======================== 外置编辑器启动 ========================

/// 启动外置编辑器打开文件
pub fn open_in_external_editor(file_path: &Path, editor_path: Option<&str>) -> anyhow::Result<()> {
    use std::process::Command;

    match editor_path {
        Some(path) if !path.is_empty() => {
            // 使用指定的编辑器
            info!("[Editor] Opening with custom editor: {}", path);

            #[cfg(target_os = "macos")]
            {
                // macOS: 如果是 .app 应用程序包，使用 open -a
                if path.ends_with(".app") {
                    info!("[Editor] Detected macOS app bundle, using 'open -a'");
                    Command::new("open")
                        .arg("-a")
                        .arg(path)
                        .arg(file_path)
                        .spawn()?;
                } else {
                    // 普通可执行文件
                    Command::new(path).arg(file_path).spawn()?;
                }
            }

            #[cfg(not(target_os = "macos"))]
            {
                Command::new(path).arg(file_path).spawn()?;
            }
        }
        _ => {
            // 使用系统默认应用
            info!("[Editor] Opening with system default");
            #[cfg(target_os = "macos")]
            {
                Command::new("open").arg("-t").arg(file_path).spawn()?;
            }
            #[cfg(target_os = "linux")]
            {
                Command::new("xdg-open").arg(file_path).spawn()?;
            }
            #[cfg(target_os = "windows")]
            {
                Command::new("cmd")
                    .args(["/C", "start", "", &file_path.to_string_lossy()])
                    .spawn()?;
            }
        }
    }

    Ok(())
}

// ======================== 文件监控器 ========================

/// 被监控的文件信息
#[derive(Clone)]
pub struct WatchedFile {
    pub local_path: PathBuf,
    pub remote_path: String,
    pub session_id: String,
    pub last_modified: SystemTime,
}

/// 文件监控事件
#[derive(Clone, Debug)]
pub enum FileWatchEvent {
    /// 文件被修改
    Modified {
        session_id: String,
        local_path: PathBuf,
        remote_path: String,
    },
}

/// 文件监控器
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    /// 使用 Arc<Mutex> 使 watched_files 在 notify 回调和 watch() 方法间共享
    watched_files: std::sync::Arc<std::sync::Mutex<HashMap<PathBuf, WatchedFile>>>,
    event_sender: mpsc::Sender<FileWatchEvent>,
}

impl FileWatcher {
    /// 创建新的文件监控器
    pub fn new(event_sender: mpsc::Sender<FileWatchEvent>) -> anyhow::Result<Self> {
        // 使用 Arc<Mutex> 使 watched_files 在 notify 回调和 watch() 方法间共享
        let watched_files: std::sync::Arc<std::sync::Mutex<HashMap<PathBuf, WatchedFile>>> =
            std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
        let watched_files_for_handler = watched_files.clone();
        let sender_clone = event_sender.clone();

        let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    // 调试：记录所有收到的事件
                    info!("[FileWatcher] Raw event: {:?}", event.kind);

                    // 只处理修改事件
                    if event.kind.is_modify() {
                        for path in &event.paths {
                            info!("[FileWatcher] Event path: {:?}", path);

                            // 需要使用 mut 锁来更新 last_modified
                            if let Ok(mut files) = watched_files_for_handler.lock() {
                                // 调试：打印当前监控的文件列表
                                info!(
                                    "[FileWatcher] Watched files: {:?}",
                                    files.keys().collect::<Vec<_>>()
                                );

                                if let Some(watched) = files.get_mut(path) {
                                    // 检查文件是否真的被修改了（避免重复事件）
                                    if let Ok(metadata) = std::fs::metadata(path) {
                                        if let Ok(modified) = metadata.modified() {
                                            if modified > watched.last_modified {
                                                info!("[FileWatcher] File modified: {:?}", path);

                                                // 立即更新 last_modified，防止重复事件
                                                watched.last_modified = modified;

                                                let _ =
                                                    sender_clone.send(FileWatchEvent::Modified {
                                                        session_id: watched.session_id.clone(),
                                                        local_path: watched.local_path.clone(),
                                                        remote_path: watched.remote_path.clone(),
                                                    });
                                            }
                                        }
                                    }
                                } else {
                                    info!("[FileWatcher] Path not in watched list: {:?}", path);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("[FileWatcher] Watch error: {}", e);
                }
            }
        })?;

        Ok(Self {
            _watcher: watcher,
            watched_files,
            event_sender,
        })
    }

    /// 开始监控文件
    pub fn watch(&mut self, file: WatchedFile) -> anyhow::Result<()> {
        let path = file.local_path.clone();
        info!("[FileWatcher] Watching file: {:?}", path);

        // 添加到共享的监控列表
        if let Ok(mut files) = self.watched_files.lock() {
            files.insert(path.clone(), file);
            info!(
                "[FileWatcher] Added file to watch list, total: {}",
                files.len()
            );
        }

        // 注册文件监控
        // 注意：notify crate 需要监控父目录才能检测到文件修改
        if let Some(parent) = path.parent() {
            self._watcher.watch(parent, RecursiveMode::NonRecursive)?;
        }

        Ok(())
    }

    /// 停止监控文件
    pub fn unwatch(&mut self, local_path: &Path) -> anyhow::Result<()> {
        info!("[FileWatcher] Unwatching file: {:?}", local_path);
        if let Ok(mut files) = self.watched_files.lock() {
            files.remove(local_path);
        }

        if let Some(parent) = local_path.parent() {
            let _ = self._watcher.unwatch(parent);
        }

        Ok(())
    }

    /// 更新文件的最后修改时间（上传成功后调用）
    pub fn update_last_modified(&mut self, local_path: &Path) {
        if let Ok(mut files) = self.watched_files.lock() {
            if let Some(watched) = files.get_mut(local_path) {
                if let Ok(metadata) = std::fs::metadata(local_path) {
                    if let Ok(modified) = metadata.modified() {
                        watched.last_modified = modified;
                    }
                }
            }
        }
    }

    /// 获取事件发送器的克隆
    pub fn event_sender(&self) -> mpsc::Sender<FileWatchEvent> {
        self.event_sender.clone()
    }

    /// 移除指定 session 的所有监控文件
    /// 返回被移除的文件路径列表（用于删除临时文件）
    pub fn unwatch_session(&mut self, session_id: &str) -> Vec<PathBuf> {
        let mut removed_paths = Vec::new();

        if let Ok(mut files) = self.watched_files.lock() {
            // 收集要移除的文件路径
            let paths_to_remove: Vec<PathBuf> = files
                .iter()
                .filter(|(_, watched)| watched.session_id == session_id)
                .map(|(path, _)| path.clone())
                .collect();

            // 移除这些文件
            for path in &paths_to_remove {
                files.remove(path);
                removed_paths.push(path.clone());

                // 取消文件系统监控
                if let Some(parent) = path.parent() {
                    let _ = self._watcher.unwatch(parent);
                }
            }

            if !removed_paths.is_empty() {
                info!(
                    "[FileWatcher] Removed {} files for session {}, remaining: {}",
                    removed_paths.len(),
                    session_id,
                    files.len()
                );
            }
        }

        removed_paths
    }

    /// 检查是否还有监控的文件
    pub fn is_empty(&self) -> bool {
        if let Ok(files) = self.watched_files.lock() {
            files.is_empty()
        } else {
            true
        }
    }
}

/// 清理指定 session 的临时文件
pub fn cleanup_temp_files_for_session(session_id: &str) {
    let temp_dir = get_temp_edit_dir();

    if !temp_dir.exists() {
        return;
    }

    // 遍历临时目录，删除以 session_id 开头的文件
    if let Ok(entries) = std::fs::read_dir(&temp_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                if filename.starts_with(session_id) {
                    if let Err(e) = std::fs::remove_file(&path) {
                        error!("[Editor] Failed to remove temp file {:?}: {}", path, e);
                    } else {
                        info!("[Editor] Removed temp file: {:?}", path);
                    }
                }
            }
        }
    }
}
