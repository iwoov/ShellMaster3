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

    /// SFTP 删除文件或目录（乐观更新：立即从列表移除，失败时恢复）
    pub fn sftp_delete(&mut self, tab_id: &str, path: String, cx: &mut gpui::Context<Self>) {
        info!("[SFTP] Delete: {} for tab {}", path, tab_id);

        // 获取当前目录路径用于缓存失效
        let current_path = {
            let tab = self.tabs.iter().find(|t| t.id == tab_id);
            tab.and_then(|t| t.sftp_state.as_ref())
                .map(|s| s.current_path.clone())
        };

        // 判断是文件还是目录，并执行乐观更新（立即从列表移除）
        let (is_dir, removed_entry) = {
            let tab = self.tabs.iter_mut().find(|t| t.id == tab_id);
            match tab.and_then(|t| t.sftp_state.as_mut()) {
                Some(state) => {
                    let is_dir = state
                        .file_list
                        .iter()
                        .find(|e| e.path == path)
                        .map(|e| e.is_dir())
                        .unwrap_or(false);
                    // 乐观更新：立即从列表移除
                    let removed = state.remove_file_from_list(&path);
                    (is_dir, removed)
                }
                None => return,
            }
        };
        // 通知 UI 更新（文件已从列表移除）
        cx.notify();

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
                    // 恢复被删除的文件
                    if let Some((index, entry)) = removed_entry {
                        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                            if let Some(ref mut sftp_state) = tab.sftp_state {
                                sftp_state.restore_file_to_list(index, entry);
                            }
                        }
                        cx.notify();
                    }
                    return;
                }
            };
            match guard.get(&tab_id_owned) {
                Some(s) => s.clone(),
                None => {
                    error!("[SFTP] No SFTP service for tab {}", tab_id_owned);
                    // 恢复被删除的文件
                    if let Some((index, entry)) = removed_entry {
                        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                            if let Some(ref mut sftp_state) = tab.sftp_state {
                                sftp_state.restore_file_to_list(index, entry);
                            }
                        }
                        cx.notify();
                    }
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
                            match &result {
                                Ok(()) => {
                                    info!("[SFTP] Successfully deleted: {}", path);
                                    // 使当前目录缓存失效（下次进入时会重新加载）
                                    if let Some(tab) =
                                        state.tabs.iter_mut().find(|t| t.id == tab_id_clone)
                                    {
                                        if let Some(ref mut sftp_state) = tab.sftp_state {
                                            if let Some(ref current) = current_path {
                                                sftp_state.invalidate_cache(current);
                                            }
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!("[SFTP] Failed to delete {}: {}", path, e);
                                    // 删除失败，恢复文件到列表
                                    if let Some((index, entry)) = removed_entry.clone() {
                                        if let Some(tab) =
                                            state.tabs.iter_mut().find(|t| t.id == tab_id_clone)
                                        {
                                            if let Some(ref mut sftp_state) = tab.sftp_state {
                                                sftp_state.restore_file_to_list(index, entry);
                                                sftp_state.set_error(format!("删除失败: {}", e));
                                            }
                                        }
                                    }
                                }
                            }
                            cx.notify();
                        });

                        // 推送失败通知
                        if result.is_err() {
                            if let Some(window) = cx.active_window() {
                                use gpui::AppContext as _;
                                let _ = cx.update_window(window, |_, window, cx| {
                                    use gpui::Styled;
                                    use gpui_component::notification::{
                                        Notification, NotificationType,
                                    };
                                    use gpui_component::WindowExt;

                                    // 加载语言设置
                                    let lang = crate::services::storage::load_settings()
                                        .map(|s| s.theme.language)
                                        .unwrap_or_default();

                                    let notification = Notification::new()
                                        .message(crate::i18n::t(&lang, "sftp.delete.failed"))
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

    /// SFTP 重命名文件或目录
    pub fn sftp_rename(
        &mut self,
        tab_id: &str,
        old_path: String,
        new_name: String,
        cx: &mut gpui::Context<Self>,
    ) {
        info!(
            "[SFTP] Rename: {} -> {} for tab {}",
            old_path, new_name, tab_id
        );

        // 计算新路径
        let new_path = if let Some(parent) = old_path.rsplit_once('/').map(|(p, _)| p) {
            if parent.is_empty() {
                format!("/{}", new_name)
            } else {
                format!("{}/{}", parent, new_name)
            }
        } else {
            new_name.clone()
        };

        // 获取当前目录路径用于刷新
        let current_path = {
            let tab = self.tabs.iter().find(|t| t.id == tab_id);
            tab.and_then(|t| t.sftp_state.as_ref())
                .map(|s| s.current_path.clone())
        };

        let sftp_services = self.sftp_services.clone();
        let session_state = cx.entity().clone();
        let tab_id_owned = tab_id.to_string();
        let old_path_clone = old_path.clone();
        let new_path_clone = new_path.clone();

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

        // 在 tokio 运行时中执行重命名操作
        ssh_manager.runtime().spawn(async move {
            let result = service.rename(&old_path_clone, &new_path_clone).await;
            let _ = tx.send(result);
        });

        // 在 GPUI 异步上下文中处理结果
        let tab_id_for_ui = tab_id.to_string();
        cx.to_async()
            .spawn(async move |async_cx| {
                if let Some(result) = rx.recv().await {
                    let tab_id_clone = tab_id_for_ui.clone();
                    let result_clone = result.clone();
                    let _ = async_cx.update(|cx| {
                        // 先更新 state
                        session_state.update(cx, |state, cx| {
                            match &result_clone {
                                Ok(()) => {
                                    info!(
                                        "[SFTP] Successfully renamed: {} -> {}",
                                        old_path, new_path
                                    );
                                    // 刷新当前目录
                                    if let Some(current) = current_path.clone() {
                                        state.sftp_load_directory(&tab_id_clone, current, cx);
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        "[SFTP] Failed to rename {} to {}: {}",
                                        old_path, new_path, e
                                    );
                                    if let Some(tab) =
                                        state.tabs.iter_mut().find(|t| t.id == tab_id_clone)
                                    {
                                        if let Some(ref mut sftp_state) = tab.sftp_state {
                                            sftp_state.set_error(format!("重命名失败: {}", e));
                                        }
                                    }
                                }
                            }
                            cx.notify();
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
                                        .message(crate::i18n::t(&lang, "sftp.rename.failed"))
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
            // 双击文件 → 打开编辑
            self.sftp_edit_file(tab_id, path, cx);
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

    /// 在终端中打开目录 (cd 到指定路径)
    pub fn sftp_open_in_terminal(
        &mut self,
        tab_id: &str,
        path: String,
        cx: &mut gpui::Context<Self>,
    ) {
        info!("[SFTP] Open in terminal: {} for tab {}", path, tab_id);

        // 获取当前激活的终端实例的 PTY 通道
        let pty_channel = {
            let tab = match self.tabs.iter().find(|t| t.id == tab_id) {
                Some(t) => t,
                None => {
                    error!("[SFTP] No tab found: {}", tab_id);
                    return;
                }
            };

            let terminal_id = match &tab.active_terminal_id {
                Some(id) => id.clone(),
                None => {
                    error!("[SFTP] No active terminal for tab {}", tab_id);
                    return;
                }
            };

            let terminal = match tab.terminals.iter().find(|t| t.id == terminal_id) {
                Some(t) => t,
                None => {
                    error!(
                        "[SFTP] Terminal {} not found in tab {}",
                        terminal_id, tab_id
                    );
                    return;
                }
            };

            match &terminal.pty_channel {
                Some(ch) => ch.clone(),
                None => {
                    error!("[SFTP] No PTY channel for terminal {}", terminal_id);
                    return;
                }
            }
        };

        // 构建 cd 命令（处理路径中的特殊字符）
        let command = format!("cd '{}'\n", path.replace("'", "'\\''"));

        // 在 tokio 运行时中发送命令
        let ssh_manager = crate::ssh::manager::SshManager::global();
        ssh_manager.runtime().spawn(async move {
            match pty_channel.write(command.as_bytes()).await {
                Ok(()) => {
                    info!("[SFTP] cd command sent successfully");
                }
                Err(e) => {
                    error!("[SFTP] Failed to send cd command: {:?}", e);
                }
            }
        });

        cx.notify();
    }

    /// 编辑远程文件（外置编辑器）
    /// 1. 下载文件到本地临时目录
    /// 2. 注册文件监控
    /// 3. 启动外置编辑器
    pub fn sftp_edit_file(
        &mut self,
        tab_id: &str,
        remote_path: String,
        cx: &mut gpui::Context<Self>,
    ) {
        use crate::services::sftp::{
            ensure_temp_dir, open_in_external_editor, temp_file_path, FileWatchEvent, FileWatcher,
            WatchedFile,
        };
        use std::sync::{Arc, Mutex};

        info!("[Editor] Edit file: {} for tab {}", remote_path, tab_id);

        // 加载设置
        let settings = crate::services::storage::load_settings().unwrap_or_default();
        let use_builtin = settings.sftp.use_builtin_editor;
        let editor_path = settings.sftp.external_editor_path.clone();
        let max_size_kb = settings.sftp.max_edit_file_size_kb;

        // 检查是否使用内置编辑器
        if use_builtin {
            info!("[Editor] Using built-in editor (not implemented yet)");
            // TODO: 打开内置编辑器
            return;
        }

        // 获取文件大小
        let file_size = {
            let tab = self.tabs.iter().find(|t| t.id == tab_id);
            match tab.and_then(|t| t.sftp_state.as_ref()) {
                Some(state) => state
                    .file_list
                    .iter()
                    .find(|e| e.path == remote_path)
                    .map(|e| e.size),
                None => return,
            }
        };

        // 检查文件大小限制
        if let Some(size) = file_size {
            let max_size_bytes = (max_size_kb as u64) * 1024;
            if size > max_size_bytes {
                error!(
                    "[Editor] File too large: {} bytes (max: {} bytes)",
                    size, max_size_bytes
                );
                // 显示通知
                if let Some(window) = cx.active_window() {
                    use gpui::AppContext as _;
                    let _ = cx.update_window(window, |_, window, cx| {
                        use gpui::Styled;
                        use gpui_component::notification::{Notification, NotificationType};
                        use gpui_component::WindowExt;

                        let lang = crate::services::storage::load_settings()
                            .map(|s| s.theme.language)
                            .unwrap_or_default();

                        let notification = Notification::new()
                            .message(format!(
                                "{}: {}",
                                crate::i18n::t(&lang, "sftp.edit.file_too_large"),
                                format_file_size(size)
                            ))
                            .with_type(NotificationType::Warning)
                            .w_64()
                            .py_2();
                        window.push_notification(notification, cx);
                    });
                }
                return;
            }
        }

        // 确保临时目录存在
        if let Err(e) = ensure_temp_dir() {
            error!("[Editor] Failed to create temp dir: {}", e);
            return;
        }

        // 生成临时文件路径
        let local_path = temp_file_path(tab_id, &remote_path);
        info!("[Editor] Temp file path: {:?}", local_path);

        // 初始化文件监控器（如果尚未初始化）
        if self.file_watcher.is_none() {
            let (tx, rx) = std::sync::mpsc::channel::<FileWatchEvent>();
            match FileWatcher::new(tx) {
                Ok(watcher) => {
                    self.file_watcher = Some(Arc::new(Mutex::new(watcher)));
                    self.file_watch_receiver = Some(rx);
                    info!("[Editor] FileWatcher initialized");

                    // 启动文件监控事件循环
                    self.start_file_watcher_loop(cx);
                }
                Err(e) => {
                    error!("[Editor] Failed to create FileWatcher: {}", e);
                }
            }
        }

        // 获取 SFTP 服务
        let sftp_services = self.sftp_services.clone();

        let tab_id_owned = tab_id.to_string();
        let remote_path_clone = remote_path.clone();
        let local_path_clone = local_path.clone();
        let file_watcher = self.file_watcher.clone();
        let editor_path_clone = editor_path.clone();

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

        // 在 tokio 运行时中下载文件
        let tab_id_for_download = tab_id_owned.clone();
        ssh_manager.runtime().spawn(async move {
            use tokio::io::AsyncReadExt;

            info!("[Editor] Downloading file: {}", remote_path_clone);

            // 打开远程文件
            let mut remote_file = match service.open(&remote_path_clone).await {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.send(Err(format!("Failed to open remote file: {}", e)));
                    return;
                }
            };

            // 读取文件内容
            let mut content = Vec::new();
            if let Err(e) = remote_file.read_to_end(&mut content).await {
                let _ = tx.send(Err(format!("Failed to read remote file: {}", e)));
                return;
            }

            // 写入本地临时文件
            if let Err(e) = std::fs::write(&local_path_clone, &content) {
                let _ = tx.send(Err(format!("Failed to write temp file: {}", e)));
                return;
            }

            info!(
                "[Editor] Downloaded {} bytes to {:?}",
                content.len(),
                local_path_clone
            );

            // 注册文件监控
            if let Some(watcher) = &file_watcher {
                if let Ok(mut watcher) = watcher.lock() {
                    let watched_file = WatchedFile {
                        local_path: local_path_clone.clone(),
                        remote_path: remote_path_clone.clone(),
                        session_id: tab_id_for_download.clone(),
                        last_modified: std::time::SystemTime::now(),
                    };
                    if let Err(e) = watcher.watch(watched_file) {
                        error!("[Editor] Failed to watch file: {}", e);
                    }
                }
            }

            // 启动外置编辑器
            let editor_path_opt = if editor_path_clone.is_empty() {
                None
            } else {
                Some(editor_path_clone.as_str())
            };

            if let Err(e) = open_in_external_editor(&local_path_clone, editor_path_opt) {
                let _ = tx.send(Err(format!("Failed to open editor: {}", e)));
                return;
            }

            let _ = tx.send(Ok(()));
        });

        // 在 GPUI 异步上下文中处理结果
        cx.to_async()
            .spawn(async move |async_cx| {
                if let Some(result) = rx.recv().await {
                    let _ = async_cx.update(|cx| {
                        if let Err(e) = result {
                            error!("[Editor] Edit file failed: {}", e);
                            // 显示错误通知
                            if let Some(window) = cx.active_window() {
                                use gpui::AppContext as _;
                                let _ = cx.update_window(window, |_, window, cx| {
                                    use gpui::Styled;
                                    use gpui_component::notification::{
                                        Notification, NotificationType,
                                    };
                                    use gpui_component::WindowExt;

                                    let notification = Notification::new()
                                        .message(e)
                                        .with_type(NotificationType::Error)
                                        .w_64()
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

    /// 启动文件监控事件循环
    fn start_file_watcher_loop(&mut self, _cx: &mut gpui::Context<Self>) {
        // 将 receiver 从 Option 中取出
        let receiver = match self.file_watch_receiver.take() {
            Some(r) => r,
            None => return,
        };

        let sftp_services = self.sftp_services.clone();
        let file_watcher = self.file_watcher.clone();

        // 使用 std::thread 而不是 tokio，因为 std::sync::mpsc::Receiver 不是 async 的
        std::thread::spawn(move || {
            info!("[FileWatcher] Event loop started");

            while let Ok(event) = receiver.recv() {
                match event {
                    crate::services::sftp::FileWatchEvent::Modified {
                        session_id,
                        local_path,
                        remote_path,
                    } => {
                        info!(
                            "[FileWatcher] File modified: {:?} -> {}",
                            local_path, remote_path
                        );

                        // 读取本地文件内容
                        let content = match std::fs::read(&local_path) {
                            Ok(c) => c,
                            Err(e) => {
                                error!("[FileWatcher] Failed to read file: {}", e);
                                continue;
                            }
                        };

                        // 获取 SFTP 服务
                        let service = {
                            let guard = match sftp_services.lock() {
                                Ok(g) => g,
                                Err(e) => {
                                    error!("[FileWatcher] Failed to lock sftp_services: {}", e);
                                    continue;
                                }
                            };
                            match guard.get(&session_id) {
                                Some(s) => s.clone(),
                                None => {
                                    error!(
                                        "[FileWatcher] No SFTP service for session {}",
                                        session_id
                                    );
                                    continue;
                                }
                            }
                        };

                        // 获取 SSH manager 的 runtime
                        let ssh_manager = crate::ssh::manager::SshManager::global();
                        let remote_path_clone = remote_path.clone();
                        let local_path_clone = local_path.clone();
                        let file_watcher_clone = file_watcher.clone();

                        // 在 tokio 运行时中上传文件
                        ssh_manager.runtime().spawn(async move {
                            info!("[FileWatcher] Uploading file to {}", remote_path_clone);

                            match service.write_file(&remote_path_clone, &content).await {
                                Ok(()) => {
                                    info!(
                                        "[FileWatcher] Successfully uploaded {} bytes to {}",
                                        content.len(),
                                        remote_path_clone
                                    );

                                    // 更新最后修改时间
                                    if let Some(watcher) = &file_watcher_clone {
                                        if let Ok(mut watcher) = watcher.lock() {
                                            watcher.update_last_modified(&local_path_clone);
                                        }
                                    }
                                }
                                Err(e) => {
                                    error!(
                                        "[FileWatcher] Failed to upload file {}: {}",
                                        remote_path_clone, e
                                    );
                                }
                            }
                        });
                    }
                }
            }

            info!("[FileWatcher] Event loop ended");
        });
    }
}

/// 格式化文件大小（内部辅助函数）
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
