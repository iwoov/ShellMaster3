// SFTP 导航方法：服务启动、目录导航、刷新等

use super::{convert_sftp_entries, get_path_hierarchy, SessionState, SftpInitResult};
use crate::models::sftp::SftpState;
use crate::services::sftp::SftpService;
use tracing::{error, info};

impl SessionState {
    /// 启动 SFTP 服务
    /// 在 SSH 连接成功后调用，初始化 SFTP 子系统并加载用户主目录
    pub fn start_sftp_service(&mut self, tab_id: String, cx: &mut gpui::Context<Self>) {
        let session_state = cx.entity().clone();

        // 获取 SSH manager 和 session
        let ssh_manager = crate::ssh::manager::SshManager::global();
        let Some(session) = ssh_manager.get_session(&tab_id) else {
            error!("[SFTP] No SSH session found for tab {}", tab_id);
            return;
        };

        info!("[SFTP] Starting SFTP service for tab {}", tab_id);

        // 直接初始化空的 SftpState
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            let mut sftp_state = SftpState::default();
            sftp_state.show_hidden = true;
            tab.sftp_state = Some(sftp_state);
        }

        // 创建 channel 用于从 tokio 运行时发送结果到 GPUI
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<SftpInitResult>();

        // 在 SSH 运行时中启动 SFTP 初始化任务
        let tab_id_for_tokio = tab_id.clone();
        let sftp_services = self.sftp_services.clone();
        ssh_manager.runtime().spawn(async move {
            let sftp_result = SftpService::new(tab_id_for_tokio.clone(), &session).await;

            match sftp_result {
                Ok(service) => {
                    info!(
                        "[SFTP] SFTP service initialized for tab {}",
                        tab_id_for_tokio
                    );
                    let sftp = service.sftp();

                    let tx_dir = tx.clone();
                    let sftp_for_dir = sftp.clone();
                    let tab_id_for_dir = tab_id_for_tokio.clone();
                    let sftp_services_clone = sftp_services.clone();

                    let dir_task = async move {
                        // 阶段1：获取主目录
                        let home_dir = match sftp_for_dir.canonicalize(".").await {
                            Ok(home) => {
                                info!("[SFTP] Home directory: {}", home);
                                home
                            }
                            Err(e) => {
                                error!("[SFTP] Failed to get home directory: {:?}", e);
                                "/".to_string()
                            }
                        };

                        let _ = tx_dir.send(SftpInitResult::HomeReady {
                            home_dir: home_dir.clone(),
                        });
                        info!("[SFTP] HomeReady sent");

                        // 阶段2：读取当前目录
                        let home_entries = match sftp_for_dir.read_dir(&home_dir).await {
                            Ok(entries) => {
                                let entries: Vec<_> = entries.collect();
                                let file_entries = convert_sftp_entries(&home_dir, entries);
                                info!(
                                    "[SFTP] Loaded {} entries from home: {}",
                                    file_entries.len(),
                                    home_dir
                                );
                                file_entries
                            }
                            Err(e) => {
                                error!("[SFTP] Failed to read home directory: {:?}", e);
                                Vec::new()
                            }
                        };

                        let _ = tx_dir.send(SftpInitResult::CurrentDirReady {
                            path: home_dir.clone(),
                            entries: home_entries,
                        });
                        info!("[SFTP] CurrentDirReady sent");

                        // 阶段3：并行读取所有父级目录
                        let path_hierarchy = get_path_hierarchy(&home_dir);
                        let parent_paths: Vec<_> = path_hierarchy
                            .into_iter()
                            .filter(|p| *p != home_dir)
                            .collect();

                        if !parent_paths.is_empty() {
                            let read_futures: Vec<_> = parent_paths
                                .iter()
                                .map(|path| {
                                    let path = path.clone();
                                    let sftp = sftp_for_dir.clone();
                                    async move {
                                        let result = sftp.read_dir(&path).await;
                                        (path, result)
                                    }
                                })
                                .collect();

                            let dir_results = futures::future::join_all(read_futures).await;
                            let mut dir_caches = Vec::new();
                            for (path, result) in dir_results {
                                if let Ok(entries) = result {
                                    let entries: Vec<_> = entries.collect();
                                    let file_entries = convert_sftp_entries(&path, entries);
                                    info!(
                                        "[SFTP] Loaded {} entries from parent: {}",
                                        file_entries.len(),
                                        path
                                    );
                                    dir_caches.push((path, file_entries));
                                }
                            }

                            let _ = tx_dir.send(SftpInitResult::ParentDirsReady { dir_caches });
                            info!("[SFTP] ParentDirsReady sent");
                        }

                        if let Ok(mut services) = sftp_services_clone.lock() {
                            services.insert(tab_id_for_dir, service);
                        }
                    };

                    let tx_ug = tx.clone();
                    let sftp_for_ug = sftp.clone();

                    let user_group_task = async move {
                        use tokio::io::AsyncReadExt;

                        let passwd_future = async {
                            match sftp_for_ug.open("/etc/passwd").await {
                                Ok(mut file) => {
                                    let mut content = String::new();
                                    if file.read_to_string(&mut content).await.is_ok() {
                                        info!(
                                            "[SFTP] Loaded /etc/passwd ({} bytes)",
                                            content.len()
                                        );
                                        return Some(content);
                                    }
                                }
                                Err(_) => {}
                            }
                            None
                        };

                        let group_future = async {
                            match sftp_for_ug.open("/etc/group").await {
                                Ok(mut file) => {
                                    let mut content = String::new();
                                    if file.read_to_string(&mut content).await.is_ok() {
                                        info!("[SFTP] Loaded /etc/group ({} bytes)", content.len());
                                        return Some(content);
                                    }
                                }
                                Err(_) => {}
                            }
                            None
                        };

                        let (passwd_content, group_content) =
                            tokio::join!(passwd_future, group_future);
                        let _ = tx_ug.send(SftpInitResult::UserGroupReady {
                            passwd_content,
                            group_content,
                        });
                    };

                    tokio::join!(dir_task, user_group_task);
                }
                Err(e) => {
                    error!("[SFTP] Failed to initialize SFTP service: {:?}", e);
                    let _ = tx.send(SftpInitResult::Error(format!("SFTP 初始化失败: {}", e)));
                }
            }
        });

        // 在 GPUI 异步上下文中循环接收结果并更新 UI
        let tab_id_for_ui = tab_id.clone();
        cx.to_async()
            .spawn(async move |async_cx| {
                while let Some(result) = rx.recv().await {
                    let tab_id_clone = tab_id_for_ui.clone();
                    let update_result = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_clone) {
                                if let Some(sftp_state) = &mut tab.sftp_state {
                                    match result {
                                        SftpInitResult::HomeReady { home_dir } => {
                                            sftp_state.set_home_dir(home_dir.clone());
                                            sftp_state.navigate_to(home_dir.clone());
                                            sftp_state.expand_to_path(&home_dir);
                                            info!("[SFTP] HomeReady processed: toolbar can render");
                                        }
                                        SftpInitResult::CurrentDirReady { path, entries } => {
                                            sftp_state.update_cache(path.clone(), entries.clone());
                                            sftp_state.update_file_list(entries);
                                            sftp_state.set_loading(false);
                                            info!("[SFTP] CurrentDirReady processed: file list can render");
                                        }
                                        SftpInitResult::ParentDirsReady { dir_caches } => {
                                            for (path, entries) in dir_caches {
                                                sftp_state.update_cache(path, entries);
                                            }
                                            info!("[SFTP] ParentDirsReady processed: folder tree fully loaded");
                                        }
                                        SftpInitResult::UserGroupReady { passwd_content, group_content } => {
                                            if let Some(passwd) = passwd_content {
                                                sftp_state.parse_passwd(&passwd);
                                            }
                                            if let Some(group) = group_content {
                                                sftp_state.parse_group(&group);
                                            }
                                            info!("[SFTP] UserGroupReady processed: user/group names available");
                                        }
                                        SftpInitResult::Error(msg) => {
                                            sftp_state.set_error(msg);
                                            sftp_state.set_loading(false);
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        });
                    });

                    if update_result.is_err() {
                        break;
                    }
                }
                info!("[SFTP] Initialization task ended for tab {}", tab_id_for_ui);
            })
            .detach();
    }

    /// 切换 SFTP 目录展开状态
    pub fn sftp_toggle_expand(&mut self, tab_id: &str, path: String, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Toggle expand: {} for tab {}", path, tab_id);

        let (is_expanded, needs_load) = {
            let tab = self.tabs.iter().find(|t| t.id == tab_id);
            match tab.and_then(|t| t.sftp_state.as_ref()) {
                Some(state) => (state.is_expanded(&path), !state.is_cache_valid(&path)),
                None => return,
            }
        };

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if let Some(ref mut sftp_state) = tab.sftp_state {
                sftp_state.toggle_expand(&path);
            }
        }
        cx.notify();

        if !is_expanded && needs_load {
            self.sftp_load_directory(tab_id, path, cx);
        }
    }

    /// 导航到指定 SFTP 目录
    pub fn sftp_navigate_to(&mut self, tab_id: &str, path: String, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Navigate to: {} for tab {}", path, tab_id);

        let needs_load = {
            let tab = self.tabs.iter().find(|t| t.id == tab_id);
            match tab.and_then(|t| t.sftp_state.as_ref()) {
                Some(state) => !state.is_cache_valid(&path),
                None => return,
            }
        };

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if let Some(ref mut sftp_state) = tab.sftp_state {
                sftp_state.navigate_to(path.clone());
                sftp_state.expand_to_path(&path);

                if let Some(entries) = sftp_state.get_cached_entries(&path) {
                    sftp_state.update_file_list(entries.clone());
                }
            }
        }
        cx.notify();

        if needs_load {
            self.sftp_load_directory(tab_id, path, cx);
        }
    }

    /// SFTP 后退导航
    pub fn sftp_go_back(&mut self, tab_id: &str, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Go back for tab {}", tab_id);

        let new_path = {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                if let Some(ref mut sftp_state) = tab.sftp_state {
                    if sftp_state.go_back() {
                        Some(sftp_state.current_path.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(path) = new_path {
            self.sftp_navigate_to(tab_id, path, cx);
        }
    }

    /// SFTP 前进导航
    pub fn sftp_go_forward(&mut self, tab_id: &str, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Go forward for tab {}", tab_id);

        let new_path = {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                if let Some(ref mut sftp_state) = tab.sftp_state {
                    if sftp_state.go_forward() {
                        Some(sftp_state.current_path.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(path) = new_path {
            self.sftp_navigate_to(tab_id, path, cx);
        }
    }

    /// SFTP 上级目录导航
    pub fn sftp_go_up(&mut self, tab_id: &str, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Go up for tab {}", tab_id);

        let new_path = {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                if let Some(ref mut sftp_state) = tab.sftp_state {
                    if sftp_state.go_up() {
                        Some(sftp_state.current_path.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(path) = new_path {
            self.sftp_navigate_to(tab_id, path, cx);
        }
    }

    /// SFTP 返回主目录
    pub fn sftp_go_home(&mut self, tab_id: &str, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Go home for tab {}", tab_id);

        let home_path = {
            if let Some(tab) = self.tabs.iter().find(|t| t.id == tab_id) {
                tab.sftp_state.as_ref().map(|s| s.home_dir.clone())
            } else {
                None
            }
        };

        if let Some(path) = home_path {
            self.sftp_navigate_to(tab_id, path, cx);
        }
    }

    /// SFTP 刷新当前目录
    pub fn sftp_refresh(&mut self, tab_id: &str, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Refresh for tab {}", tab_id);

        let current_path = {
            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                if let Some(ref mut sftp_state) = tab.sftp_state {
                    sftp_state.refresh();
                    Some(sftp_state.current_path.clone())
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(path) = current_path {
            self.sftp_load_directory(tab_id, path, cx);
        }
    }

    /// SFTP 删除文件或目录
    pub fn sftp_delete(&mut self, tab_id: &str, path: String, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Delete: {} for tab {}", path, tab_id);

        // 获取当前目录路径用于刷新
        let current_path = {
            let tab = self.tabs.iter().find(|t| t.id == tab_id);
            tab.and_then(|t| t.sftp_state.as_ref())
                .map(|s| s.current_path.clone())
        };

        // 判断是文件还是目录
        let is_dir = {
            let tab = self.tabs.iter().find(|t| t.id == tab_id);
            match tab.and_then(|t| t.sftp_state.as_ref()) {
                Some(state) => state
                    .file_list
                    .iter()
                    .find(|e| e.path == path)
                    .map(|e| e.is_dir())
                    .unwrap_or(false),
                None => return,
            }
        };

        let sftp_services = self.sftp_services.clone();
        let session_state = cx.entity().clone();
        let tab_id_owned = tab_id.to_string();
        let path_clone = path.clone();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Result<(), String>>();

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

        // 在 tokio 运行时中执行删除操作
        ssh_manager.runtime().spawn(async move {
            let result = if is_dir {
                service.remove_dir(&path_clone).await
            } else {
                service.remove_file(&path_clone).await
            };
            let _ = tx.send(result);
        });

        // 在 GPUI 异步上下文中处理结果
        let tab_id_for_ui = tab_id.to_string();
        cx.to_async()
            .spawn(async move |async_cx| {
                if let Some(result) = rx.recv().await {
                    let tab_id_clone = tab_id_for_ui.clone();
                    let _ = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            match result {
                                Ok(()) => {
                                    info!("[SFTP] Successfully deleted: {}", path);
                                    // 刷新当前目录
                                    if let Some(current) = current_path {
                                        state.sftp_load_directory(&tab_id_clone, current, cx);
                                    }
                                }
                                Err(e) => {
                                    error!("[SFTP] Failed to delete {}: {}", path, e);
                                    if let Some(tab) =
                                        state.tabs.iter_mut().find(|t| t.id == tab_id_clone)
                                    {
                                        if let Some(ref mut sftp_state) = tab.sftp_state {
                                            sftp_state.set_error(format!("删除失败: {}", e));
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        });
                    });
                }
            })
            .detach();
    }

    /// 切换显示/隐藏隐藏文件
    pub fn sftp_toggle_hidden(&mut self, tab_id: &str, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Toggle hidden for tab {}", tab_id);

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if let Some(ref mut sftp_state) = tab.sftp_state {
                sftp_state.toggle_show_hidden();
            }
        }
        cx.notify();
    }

    /// 打开文件或目录
    pub fn sftp_open(&mut self, tab_id: &str, path: String, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Open: {} for tab {}", path, tab_id);

        let is_dir = {
            let tab = self.tabs.iter().find(|t| t.id == tab_id);
            match tab.and_then(|t| t.sftp_state.as_ref()) {
                Some(state) => state
                    .file_list
                    .iter()
                    .find(|e| e.path == path)
                    .map(|e| e.is_dir())
                    .unwrap_or(false),
                None => return,
            }
        };

        if is_dir {
            self.sftp_navigate_to(tab_id, path, cx);
        } else {
            info!("[SFTP] Opening file: {} (not implemented)", path);
        }
    }

    /// 从 SftpService 加载目录内容
    pub(super) fn sftp_load_directory(
        &mut self,
        tab_id: &str,
        path: String,
        cx: &mut gpui::Context<Self>,
    ) {
        let sftp_services = self.sftp_services.clone();
        let session_state = cx.entity().clone();
        let tab_id_owned = tab_id.to_string();
        let path_clone = path.clone();

        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
            if let Some(ref mut sftp_state) = tab.sftp_state {
                sftp_state.set_loading(true);
            }
        }
        cx.notify();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<
            Result<Vec<crate::models::sftp::FileEntry>, String>,
        >();

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

        ssh_manager.runtime().spawn(async move {
            let result = match service.read_dir(&path_clone).await {
                Ok(entries) => Ok(entries),
                Err(e) => Err(e),
            };
            let _ = tx.send(result);
        });

        let tab_id_for_ui = tab_id.to_string();
        cx.to_async()
            .spawn(async move |async_cx| {
                if let Some(result) = rx.recv().await {
                    let tab_id_clone = tab_id_for_ui.clone();
                    let path_for_update = path.clone();
                    let _ = async_cx.update(|cx| {
                        session_state.update(cx, |state, cx| {
                            if let Some(tab) = state.tabs.iter_mut().find(|t| t.id == tab_id_clone)
                            {
                                if let Some(ref mut sftp_state) = tab.sftp_state {
                                    sftp_state.set_loading(false);

                                    match result {
                                        Ok(entries) => {
                                            info!(
                                                "[SFTP] Loaded {} entries from {}",
                                                entries.len(),
                                                path_for_update
                                            );
                                            sftp_state.update_cache(
                                                path_for_update.clone(),
                                                entries.clone(),
                                            );

                                            if sftp_state.current_path == path_for_update {
                                                sftp_state.update_file_list(entries);
                                            }
                                        }
                                        Err(e) => {
                                            error!(
                                                "[SFTP] Failed to load directory {}: {}",
                                                path_for_update, e
                                            );
                                            sftp_state.set_error(e);
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        });
                    });
                }
            })
            .detach();
    }
}
