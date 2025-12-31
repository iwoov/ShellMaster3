// SFTP 状态管理

use std::collections::{HashMap, HashSet};

use super::types::{CachedDir, DirCache, FileEntry, NavigationHistory};

/// SFTP 状态（每个 SessionTab 独立）
#[derive(Debug, Clone, Default)]
pub struct SftpState {
    /// 当前路径（唯一数据源）
    pub current_path: String,

    /// 当前目录的文件列表
    pub file_list: Vec<FileEntry>,
    /// 文件列表版本号（用于 UI 增量同步）
    pub file_list_revision: u64,

    /// 文件夹树展开的目录集合
    pub expanded_dirs: HashSet<String>,
    /// 文件夹树展开状态版本号（用于 UI 增量同步）
    pub expanded_dirs_revision: u64,

    /// 目录内容缓存
    pub dir_cache: DirCache,
    /// 目录缓存版本号（用于文件夹树增量同步）
    pub dir_cache_revision: u64,

    /// 导航历史
    pub history: NavigationHistory,

    /// 是否正在加载
    pub loading: bool,

    /// 错误信息
    pub error: Option<String>,

    /// 是否显示隐藏文件
    pub show_hidden: bool,

    /// 用户主目录路径
    pub home_dir: String,

    /// uid -> username 缓存
    pub user_cache: HashMap<u32, String>,

    /// gid -> groupname 缓存
    pub group_cache: HashMap<u32, String>,
    /// 用户缓存版本号（用于 UI 增量同步）
    pub user_cache_revision: u64,
    /// 组缓存版本号（用于 UI 增量同步）
    pub group_cache_revision: u64,
}

impl SftpState {
    /// 创建新的 SFTP 状态
    pub fn new(home_dir: String) -> Self {
        Self {
            current_path: home_dir.clone(),
            home_dir,
            show_hidden: true, // 默认显示隐藏文件
            ..Default::default()
        }
    }

    // ========================================================================
    // 缓存管理
    // ========================================================================

    /// 检查路径的缓存是否有效
    pub fn is_cache_valid(&self, path: &str) -> bool {
        self.dir_cache
            .get(path)
            .map_or(false, |cache| !cache.is_expired())
    }

    /// 从缓存获取目录内容
    pub fn get_cached_entries(&self, path: &str) -> Option<&Vec<FileEntry>> {
        self.dir_cache
            .get(path)
            .filter(|cache| !cache.is_expired())
            .map(|cache| &cache.entries)
    }

    /// 更新目录缓存
    pub fn update_cache(&mut self, path: String, entries: Vec<FileEntry>) {
        self.dir_cache.insert(path, CachedDir::new(entries));
        self.dir_cache_revision = self.dir_cache_revision.wrapping_add(1);
    }

    /// 清除指定路径的缓存
    pub fn invalidate_cache(&mut self, path: &str) {
        if self.dir_cache.remove(path).is_some() {
            self.dir_cache_revision = self.dir_cache_revision.wrapping_add(1);
        }
    }

    /// 清除所有缓存
    pub fn clear_cache(&mut self) {
        if !self.dir_cache.is_empty() {
            self.dir_cache.clear();
            self.dir_cache_revision = self.dir_cache_revision.wrapping_add(1);
        }
    }

    // ========================================================================
    // 导航
    // ========================================================================

    /// 设置当前路径（内部使用，不记录历史）
    fn set_path_internal(&mut self, path: String) {
        self.current_path = path;
        self.error = None;
    }

    /// 导航到指定路径（记录历史）
    pub fn navigate_to(&mut self, path: String) {
        if self.current_path != path {
            self.history.push(self.current_path.clone());
        }
        self.set_path_internal(path);
    }

    /// 后退导航
    pub fn go_back(&mut self) -> bool {
        if let Some(prev) = self.history.go_back(self.current_path.clone()) {
            self.set_path_internal(prev);
            true
        } else {
            false
        }
    }

    /// 前进导航
    pub fn go_forward(&mut self) -> bool {
        if let Some(next) = self.history.go_forward(self.current_path.clone()) {
            self.set_path_internal(next);
            true
        } else {
            false
        }
    }

    /// 导航到上级目录
    pub fn go_up(&mut self) -> bool {
        let parent = get_parent_path(&self.current_path);
        if parent != self.current_path {
            self.navigate_to(parent);
            true
        } else {
            false
        }
    }

    /// 导航到主目录
    pub fn go_home(&mut self) {
        if self.current_path != self.home_dir {
            self.navigate_to(self.home_dir.clone());
        }
    }

    /// 刷新当前目录（清除缓存）
    pub fn refresh(&mut self) {
        let path = self.current_path.clone();
        self.invalidate_cache(&path);
    }

    /// 是否可以后退
    pub fn can_go_back(&self) -> bool {
        self.history.can_go_back()
    }

    /// 是否可以前进
    pub fn can_go_forward(&self) -> bool {
        self.history.can_go_forward()
    }

    /// 是否可以向上导航
    pub fn can_go_up(&self) -> bool {
        self.current_path != "/"
    }

    // ========================================================================
    // 文件列表
    // ========================================================================

    /// 更新文件列表（从缓存或新数据）
    pub fn update_file_list(&mut self, entries: Vec<FileEntry>) {
        // 过滤隐藏文件（如果需要）
        self.file_list = if self.show_hidden {
            entries
        } else {
            entries.into_iter().filter(|e| !e.is_hidden()).collect()
        };
        self.file_list_revision = self.file_list_revision.wrapping_add(1);
    }

    /// 从文件列表中移除指定路径的文件（乐观更新）
    /// 返回被移除的文件条目及其索引（用于失败时恢复）
    pub fn remove_file_from_list(&mut self, path: &str) -> Option<(usize, FileEntry)> {
        if let Some(index) = self.file_list.iter().position(|e| e.path == path) {
            let entry = self.file_list.remove(index);
            self.file_list_revision = self.file_list_revision.wrapping_add(1);
            Some((index, entry))
        } else {
            None
        }
    }

    /// 在指定位置恢复文件条目（删除失败时使用）
    pub fn restore_file_to_list(&mut self, index: usize, entry: FileEntry) {
        let insert_index = index.min(self.file_list.len());
        self.file_list.insert(insert_index, entry);
        self.file_list_revision = self.file_list_revision.wrapping_add(1);
    }

    /// 切换显示隐藏文件
    pub fn toggle_show_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        // 需要重新从缓存加载文件列表
        let path = self.current_path.clone();
        if let Some(entries) = self.get_cached_entries(&path).cloned() {
            self.update_file_list(entries);
        }
    }

    // ========================================================================
    // 文件夹树
    // ========================================================================

    /// 展开目录
    pub fn expand_dir(&mut self, path: &str) {
        if self.expanded_dirs.insert(path.to_string()) {
            self.expanded_dirs_revision = self.expanded_dirs_revision.wrapping_add(1);
        }
    }

    /// 折叠目录
    pub fn collapse_dir(&mut self, path: &str) {
        if self.expanded_dirs.remove(path) {
            self.expanded_dirs_revision = self.expanded_dirs_revision.wrapping_add(1);
        }
    }

    /// 切换目录展开状态
    pub fn toggle_expand(&mut self, path: &str) -> bool {
        if self.expanded_dirs.contains(path) {
            self.collapse_dir(path);
            false
        } else {
            self.expand_dir(path);
            true
        }
    }

    /// 目录是否已展开
    pub fn is_expanded(&self, path: &str) -> bool {
        self.expanded_dirs.contains(path)
    }

    /// 确保路径链上的所有目录都已展开
    pub fn expand_to_path(&mut self, path: &str) {
        // 先展开根目录
        self.expand_dir("/");

        // 然后展开路径中的每个层级
        let mut current = String::new();
        for segment in path.split('/').filter(|s| !s.is_empty()) {
            current.push('/');
            current.push_str(segment);
            self.expand_dir(&current);
        }
    }

    // ========================================================================
    // 状态
    // ========================================================================

    /// 设置加载状态
    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    /// 设置错误信息
    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.loading = false;
    }

    /// 设置主目录
    pub fn set_home_dir(&mut self, home: String) {
        self.home_dir = home;
    }

    /// 清除错误
    pub fn clear_error(&mut self) {
        self.error = None;
    }

    // ========================================================================
    // 用户/组缓存
    // ========================================================================

    /// 解析 /etc/passwd 内容并缓存 uid -> username 映射
    /// 格式: username:x:uid:gid:gecos:home:shell
    pub fn parse_passwd(&mut self, content: &str) {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                if let Ok(uid) = parts[2].parse::<u32>() {
                    self.user_cache.insert(uid, parts[0].to_string());
                }
            }
        }
        self.user_cache_revision = self.user_cache_revision.wrapping_add(1);
    }

    /// 解析 /etc/group 内容并缓存 gid -> groupname 映射
    /// 格式: groupname:x:gid:members
    pub fn parse_group(&mut self, content: &str) {
        for line in content.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                if let Ok(gid) = parts[2].parse::<u32>() {
                    self.group_cache.insert(gid, parts[0].to_string());
                }
            }
        }
        self.group_cache_revision = self.group_cache_revision.wrapping_add(1);
    }

    /// 根据 uid 获取用户名
    pub fn get_username(&self, uid: u32) -> String {
        self.user_cache
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| uid.to_string())
    }

    /// 根据 gid 获取组名
    pub fn get_groupname(&self, gid: u32) -> String {
        self.group_cache
            .get(&gid)
            .cloned()
            .unwrap_or_else(|| gid.to_string())
    }

    /// 格式化 uid:gid 为 username:groupname
    pub fn format_owner(&self, uid: Option<u32>, gid: Option<u32>) -> String {
        let user = uid
            .map(|u| self.get_username(u))
            .unwrap_or_else(|| "-".to_string());
        let group = gid
            .map(|g| self.get_groupname(g))
            .unwrap_or_else(|| "-".to_string());
        format!("{}:{}", user, group)
    }
}

// ============================================================================
// 路径工具函数
// ============================================================================

/// 获取父目录路径
pub fn get_parent_path(path: &str) -> String {
    if path == "/" {
        return "/".to_string();
    }

    let path = path.trim_end_matches('/');
    match path.rfind('/') {
        Some(0) => "/".to_string(),
        Some(pos) => path[..pos].to_string(),
        None => "/".to_string(),
    }
}

/// 连接路径
pub fn join_path(base: &str, name: &str) -> String {
    if base == "/" {
        format!("/{}", name)
    } else {
        format!("{}/{}", base.trim_end_matches('/'), name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_parent_path() {
        assert_eq!(get_parent_path("/"), "/");
        assert_eq!(get_parent_path("/home"), "/");
        assert_eq!(get_parent_path("/home/user"), "/home");
        assert_eq!(get_parent_path("/home/user/"), "/home");
    }

    #[test]
    fn test_join_path() {
        assert_eq!(join_path("/", "home"), "/home");
        assert_eq!(join_path("/home", "user"), "/home/user");
        assert_eq!(join_path("/home/", "user"), "/home/user");
    }
}
