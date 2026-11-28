// SFTP 状态管理

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::types::{CachedDir, DirCache, FileEntry, NavigationHistory};

/// 目录缓存最大条目数，避免长时间浏览大目录导致内存无限增长
const DIR_CACHE_MAX_ENTRIES: usize = 64;

/// SFTP 状态（每个 SessionTab 独立）
#[derive(Debug, Clone, Default)]
pub struct SftpState {
    /// 当前路径（唯一数据源）
    pub current_path: String,

    /// 当前目录的文件列表（使用 Arc 避免同步到 UI 时整表深拷贝）
    pub file_list: Arc<Vec<FileEntry>>,
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

    /// 导航递增序列号（用于丢弃过期异步响应，防止慢请求覆盖新目录）
    pub nav_seq: u64,

    /// 正在请求中的目录集合（防止重复并发 read_dir）
    pub in_flight_dirs: HashSet<String>,

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
    // ========================================================================
    // 缓存管理
    // ========================================================================

    /// 检查路径的缓存是否有效
    pub fn is_cache_valid(&self, path: &str) -> bool {
        self.dir_cache
            .get(path)
            .map_or(false, |cache| !cache.is_expired())
    }

    /// 从缓存获取目录内容（O(1) Arc 引用克隆，并更新 LRU 访问时间）
    pub fn get_cached_entries(&mut self, path: &str) -> Option<Arc<Vec<FileEntry>>> {
        let cache = self.dir_cache.get_mut(path)?;
        if cache.is_expired() {
            return None;
        }
        cache.touch();
        Some(Arc::clone(&cache.entries))
    }

    /// 更新目录缓存（带容量上限与过期/LRU 淘汰）
    pub fn update_cache(&mut self, path: String, entries: Arc<Vec<FileEntry>>) {
        self.dir_cache.insert(path, CachedDir::new(entries));
        self.evict_cache_if_needed();
        self.dir_cache_revision = self.dir_cache_revision.wrapping_add(1);
    }

    /// 超过容量上限时先清理过期项，再按 LRU 淘汰非关键目录
    fn evict_cache_if_needed(&mut self) {
        if self.dir_cache.len() <= DIR_CACHE_MAX_ENTRIES {
            return;
        }

        // 1. 先移除过期缓存（保留当前目录、根目录与已展开目录，避免目录树闪烁）
        let current = self.current_path.clone();
        self.dir_cache.retain(|k, v| {
            !v.is_expired() || k == "/" || k == &current || self.expanded_dirs.contains(k)
        });

        // 2. 若仍超出上限，按最近访问时间从旧到新淘汰非受保护目录
        while self.dir_cache.len() > DIR_CACHE_MAX_ENTRIES {
            let victim = self
                .dir_cache
                .iter()
                .filter(|(k, _)| *k != "/" && *k != &current && !self.expanded_dirs.contains(*k))
                .min_by_key(|(_, v)| v.last_accessed)
                .map(|(k, _)| k.clone());

            match victim {
                Some(key) => {
                    self.dir_cache.remove(&key);
                }
                None => break,
            }
        }
    }

    /// 清除指定路径的缓存
    pub fn invalidate_cache(&mut self, path: &str) {
        if self.dir_cache.remove(path).is_some() {
            self.dir_cache_revision = self.dir_cache_revision.wrapping_add(1);
        }
    }

    /// 递增并返回新的导航序号
    pub fn next_nav_seq(&mut self) -> u64 {
        self.nav_seq = self.nav_seq.wrapping_add(1);
        self.nav_seq
    }

    // ========================================================================
    // 导航
    // ========================================================================

    /// 设置当前路径（内部使用，不记录历史）
    fn set_path_internal(&mut self, path: String) {
        self.current_path = path;
        self.error = None;
        self.next_nav_seq();
    }

    /// 导航到指定路径（记录历史）
    pub fn navigate_to(&mut self, path: String) {
        if self.current_path != path {
            if !self.current_path.is_empty() {
                self.history.push(self.current_path.clone());
            }
            self.set_path_internal(path);
        } else {
            self.error = None;
        }
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

    /// 刷新当前目录（清除缓存）
    pub fn refresh(&mut self) {
        let path = self.current_path.clone();
        self.invalidate_cache(&path);
        self.next_nav_seq();
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

    /// 更新文件列表（显示隐藏文件时直接共享 Arc，零拷贝）
    pub fn update_file_list(&mut self, entries: Arc<Vec<FileEntry>>) {
        self.file_list = if self.show_hidden {
            entries
        } else {
            Arc::new(
                entries
                    .iter()
                    .filter(|e| !e.is_hidden())
                    .cloned()
                    .collect(),
            )
        };
        self.file_list_revision = self.file_list_revision.wrapping_add(1);
    }

    /// 从文件列表中移除指定路径的文件（乐观更新）
    /// 返回被移除的文件条目及其索引（用于失败时恢复）
    pub fn remove_file_from_list(&mut self, path: &str) -> Option<(usize, FileEntry)> {
        let index = self.file_list.iter().position(|e| e.path == path)?;
        let entry = Arc::make_mut(&mut self.file_list).remove(index);
        self.file_list_revision = self.file_list_revision.wrapping_add(1);
        Some((index, entry))
    }

    /// 在指定位置恢复文件条目（删除失败时使用）
    pub fn restore_file_to_list(&mut self, index: usize, entry: FileEntry) {
        let list = Arc::make_mut(&mut self.file_list);
        let insert_index = index.min(list.len());
        list.insert(insert_index, entry);
        self.file_list_revision = self.file_list_revision.wrapping_add(1);
    }

    /// 切换显示隐藏文件
    pub fn toggle_show_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        // 需要重新从缓存加载文件列表
        let path = self.current_path.clone();
        if let Some(entries) = self.get_cached_entries(&path) {
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
    use crate::models::sftp::FileType;

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

    #[test]
    fn dir_cache_evicts_beyond_limit_while_keeping_current() {
        let mut state = SftpState::default();
        state.current_path = "/active".to_string();
        state.update_cache("/".to_string(), Arc::new(Vec::new()));
        state.update_cache("/active".to_string(), Arc::new(Vec::new()));

        for i in 0..80 {
            state.update_cache(format!("/dir_{}", i), Arc::new(Vec::new()));
        }

        assert!(state.dir_cache.len() <= DIR_CACHE_MAX_ENTRIES);
        assert!(state.dir_cache.contains_key("/"));
        assert!(state.dir_cache.contains_key("/active"));
    }

    #[test]
    fn update_file_list_shares_arc_when_showing_hidden() {
        let mut state = SftpState::default();
        state.show_hidden = true;
        let entries = Arc::new(vec![FileEntry::new(
            ".env".to_string(),
            "/.env".to_string(),
            FileType::File,
        )]);
        state.update_file_list(Arc::clone(&entries));
        assert!(Arc::ptr_eq(&state.file_list, &entries));
    }
}
