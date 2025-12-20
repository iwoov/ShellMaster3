// SFTP 基础数据类型

use std::collections::HashMap;
use std::time::{Instant, SystemTime};

/// 文件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// 普通文件
    File,
    /// 目录
    Directory,
    /// 符号链接
    Symlink,
    /// 其他类型
    Other,
}

impl Default for FileType {
    fn default() -> Self {
        FileType::File
    }
}

/// 文件条目
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// 文件名
    pub name: String,
    /// 完整路径
    pub path: String,
    /// 文件类型
    pub file_type: FileType,
    /// 文件大小（字节）
    pub size: u64,
    /// 修改时间
    pub modified: Option<SystemTime>,
    /// Unix 权限（如 0o755）
    pub permissions: u32,
    /// 所有者用户 ID
    pub uid: Option<u32>,
    /// 所有者组 ID
    pub gid: Option<u32>,
}

impl FileEntry {
    /// 创建新的文件条目
    pub fn new(name: String, path: String, file_type: FileType) -> Self {
        Self {
            name,
            path,
            file_type,
            size: 0,
            modified: None,
            permissions: 0,
            uid: None,
            gid: None,
        }
    }

    /// 是否是目录
    pub fn is_dir(&self) -> bool {
        self.file_type == FileType::Directory
    }

    /// 是否是文件
    pub fn is_file(&self) -> bool {
        self.file_type == FileType::File
    }

    /// 是否是隐藏文件（以 . 开头）
    pub fn is_hidden(&self) -> bool {
        self.name.starts_with('.')
    }

    /// 获取文件扩展名
    pub fn extension(&self) -> Option<&str> {
        if self.is_dir() {
            return None;
        }
        self.name.rsplit('.').next().filter(|ext| *ext != self.name)
    }

    /// 格式化文件大小
    pub fn format_size(&self) -> String {
        if self.is_dir() {
            return "-".to_string();
        }

        let size = self.size as f64;
        if size >= 1_073_741_824.0 {
            format!("{:.1} GB", size / 1_073_741_824.0)
        } else if size >= 1_048_576.0 {
            format!("{:.1} MB", size / 1_048_576.0)
        } else if size >= 1_024.0 {
            format!("{:.1} KB", size / 1_024.0)
        } else {
            format!("{} B", self.size)
        }
    }

    /// 格式化权限字符串（如 rwxr-xr-x）
    pub fn format_permissions(&self) -> String {
        let perms = self.permissions;
        let mut s = String::with_capacity(10);

        // 文件类型标识
        s.push(match self.file_type {
            FileType::Directory => 'd',
            FileType::Symlink => 'l',
            _ => '-',
        });

        // 所有者权限
        s.push(if perms & 0o400 != 0 { 'r' } else { '-' });
        s.push(if perms & 0o200 != 0 { 'w' } else { '-' });
        s.push(if perms & 0o100 != 0 { 'x' } else { '-' });

        // 组权限
        s.push(if perms & 0o040 != 0 { 'r' } else { '-' });
        s.push(if perms & 0o020 != 0 { 'w' } else { '-' });
        s.push(if perms & 0o010 != 0 { 'x' } else { '-' });

        // 其他用户权限
        s.push(if perms & 0o004 != 0 { 'r' } else { '-' });
        s.push(if perms & 0o002 != 0 { 'w' } else { '-' });
        s.push(if perms & 0o001 != 0 { 'x' } else { '-' });

        s
    }
}

/// 缓存的目录内容
#[derive(Debug, Clone)]
pub struct CachedDir {
    /// 目录中的文件条目
    pub entries: Vec<FileEntry>,
    /// 缓存时间
    pub cached_at: Instant,
}

impl CachedDir {
    /// 创建新的缓存目录
    pub fn new(entries: Vec<FileEntry>) -> Self {
        Self {
            entries,
            cached_at: Instant::now(),
        }
    }

    /// 检查缓存是否过期（默认 30 秒）
    pub fn is_expired(&self) -> bool {
        self.cached_at.elapsed().as_secs() > 30
    }
}

/// 导航历史
#[derive(Debug, Clone, Default)]
pub struct NavigationHistory {
    /// 后退栈
    pub back_stack: Vec<String>,
    /// 前进栈
    pub forward_stack: Vec<String>,
}

impl NavigationHistory {
    /// 是否可以后退
    pub fn can_go_back(&self) -> bool {
        !self.back_stack.is_empty()
    }

    /// 是否可以前进
    pub fn can_go_forward(&self) -> bool {
        !self.forward_stack.is_empty()
    }

    /// 记录导航（清除前进栈）
    pub fn push(&mut self, path: String) {
        self.back_stack.push(path);
        self.forward_stack.clear();
    }

    /// 执行后退
    pub fn go_back(&mut self, current_path: String) -> Option<String> {
        if let Some(prev) = self.back_stack.pop() {
            self.forward_stack.push(current_path);
            Some(prev)
        } else {
            None
        }
    }

    /// 执行前进
    pub fn go_forward(&mut self, current_path: String) -> Option<String> {
        if let Some(next) = self.forward_stack.pop() {
            self.back_stack.push(current_path);
            Some(next)
        } else {
            None
        }
    }
}

/// 目录缓存管理器
pub type DirCache = HashMap<String, CachedDir>;
