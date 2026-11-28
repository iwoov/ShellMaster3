// SFTP 编辑器服务 - 内置编辑器文本校验/语言识别 & 外置编辑器临时文件管理与监控

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::SystemTime;
use tracing::{debug, error, info};

// ======================== 内置编辑器：文本校验、行尾符与语言识别 ========================

/// 文本行尾符类型（编辑器内部统一使用 LF，保存时按原始类型还原）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::CrLf => "CRLF",
        }
    }
}

/// 内置编辑器解码远程文件失败的原因
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextDecodeError {
    /// 文件包含 NULL 字节或过高比例控制字符，判定为二进制文件
    Binary,
    /// 非合法 UTF-8 编码
    InvalidUtf8,
}

/// 解码后的文本元信息
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedTextFile {
    /// 统一为 LF 且移除前导 BOM 后的文本内容（直接供 EditorState 编辑）
    pub content: String,
    /// 原始行尾符类型
    pub line_ending: LineEnding,
    /// 原文件是否带有 UTF-8 BOM（写回时原样保留）
    pub has_bom: bool,
}

/// 根据路径识别出的语法高亮标识与展示名
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetectedLanguage {
    /// 传给 `EditorState::language(...)` 的 token（由 syntect / two-face 解析）
    pub syntax_token: &'static str,
    /// 状态栏展示名
    pub display_name: &'static str,
}

/// 检测字节流是否为二进制文件（采样前 8KB）
pub fn is_binary_content(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return false;
    }

    let sample_len = bytes.len().min(8192);
    let sample = &bytes[..sample_len];

    // 常见文本文件不会包含 NUL 字节
    if sample.contains(&0) {
        return true;
    }

    // 统计非空白控制字符（允许 TAB/LF/VT/FF/CR/ESC）
    let control_count = sample
        .iter()
        .filter(|&&b| {
            (b < 0x20 && !matches!(b, b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | 0x1b)) || b == 0x7f
        })
        .count();

    control_count * 10 > sample_len
}

/// 严格校验并解码远程文件字节流：先排除二进制文件，再做严格 UTF-8 解码，避免写回损坏原编码
pub fn decode_utf8_text(bytes: &[u8]) -> Result<DecodedTextFile, TextDecodeError> {
    if is_binary_content(bytes) {
        return Err(TextDecodeError::Binary);
    }

    let raw = std::str::from_utf8(bytes).map_err(|_| TextDecodeError::InvalidUtf8)?;
    let (has_bom, without_bom) = match raw.strip_prefix('\u{feff}') {
        Some(stripped) => (true, stripped),
        None => (false, raw),
    };

    let line_ending = detect_line_ending(without_bom);
    let content = normalize_to_lf(without_bom);

    Ok(DecodedTextFile {
        content,
        line_ending,
        has_bom,
    })
}

/// 检测文本主要行尾符
pub fn detect_line_ending(text: &str) -> LineEnding {
    let mut crlf_count = 0usize;
    let mut lf_only_count = 0usize;
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
            crlf_count += 1;
            i += 2;
        } else if bytes[i] == b'\n' {
            lf_only_count += 1;
            i += 1;
        } else {
            i += 1;
        }
    }

    if crlf_count > 0 && crlf_count >= lf_only_count {
        LineEnding::CrLf
    } else {
        LineEnding::Lf
    }
}

/// 将文本中的 CRLF / 孤立 CR 统一规范为 LF 供编辑器展示
pub fn normalize_to_lf(text: &str) -> String {
    if !text.contains('\r') {
        return text.to_string();
    }
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// 保存时还原为原文件的行尾符与 BOM 字节序列
pub fn encode_utf8_text(content: &str, line_ending: LineEnding, has_bom: bool) -> Vec<u8> {
    let normalized = normalize_to_lf(content);
    let final_text = match line_ending {
        LineEnding::Lf => normalized,
        LineEnding::CrLf => normalized.replace('\n', "\r\n"),
    };

    let mut out = Vec::with_capacity(final_text.len() + if has_bom { 3 } else { 0 });
    if has_bom {
        out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    out.extend_from_slice(final_text.as_bytes());
    out
}

/// 根据远程文件路径识别语法高亮语言（兼容常见运维配置文件与无后缀特殊文件名）
pub fn detect_language_from_path(remote_path: &str) -> DetectedLanguage {
    let file_name = Path::new(remote_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let lower_name = file_name.to_ascii_lowercase();

    // 1. 特殊文件名优先匹配
    if lower_name == "dockerfile"
        || lower_name.starts_with("dockerfile.")
        || lower_name == "containerfile"
    {
        return DetectedLanguage {
            syntax_token: "Dockerfile",
            display_name: "Dockerfile",
        };
    }
    if lower_name == "makefile" || lower_name == "gnumakefile" || lower_name.ends_with(".mk") {
        return DetectedLanguage {
            syntax_token: "Makefile",
            display_name: "Makefile",
        };
    }
    if lower_name == "nginx.conf"
        || remote_path.contains("/nginx/") && lower_name.ends_with(".conf")
    {
        return DetectedLanguage {
            syntax_token: "nginx",
            display_name: "Nginx",
        };
    }
    if matches!(
        lower_name.as_str(),
        ".bashrc" | ".bash_profile" | ".bash_logout" | ".zshrc" | ".zprofile" | ".profile" | ".env"
    ) || lower_name.starts_with(".env.")
    {
        return DetectedLanguage {
            syntax_token: "bash",
            display_name: "Shell",
        };
    }
    if matches!(lower_name.as_str(), ".gitignore" | ".dockerignore") {
        return DetectedLanguage {
            syntax_token: "gitignore",
            display_name: "Ignore",
        };
    }
    if lower_name == "cargo.lock" {
        return DetectedLanguage {
            syntax_token: "toml",
            display_name: "TOML",
        };
    }
    if lower_name == "cmakelists.txt" {
        return DetectedLanguage {
            syntax_token: "cmake",
            display_name: "CMake",
        };
    }

    // 2. 按扩展名匹配
    let ext = Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "rs" => DetectedLanguage {
            syntax_token: "rust",
            display_name: "Rust",
        },
        "py" | "pyi" => DetectedLanguage {
            syntax_token: "python",
            display_name: "Python",
        },
        "js" | "mjs" | "cjs" => DetectedLanguage {
            syntax_token: "javascript",
            display_name: "JavaScript",
        },
        "jsx" => DetectedLanguage {
            syntax_token: "jsx",
            display_name: "JSX",
        },
        "ts" | "mts" | "cts" => DetectedLanguage {
            syntax_token: "typescript",
            display_name: "TypeScript",
        },
        "tsx" => DetectedLanguage {
            syntax_token: "tsx",
            display_name: "TSX",
        },
        "json" | "jsonc" => DetectedLanguage {
            syntax_token: "json",
            display_name: "JSON",
        },
        "toml" => DetectedLanguage {
            syntax_token: "toml",
            display_name: "TOML",
        },
        "yml" | "yaml" => DetectedLanguage {
            syntax_token: "yaml",
            display_name: "YAML",
        },
        "xml" | "svg" | "plist" => DetectedLanguage {
            syntax_token: "xml",
            display_name: "XML",
        },
        "html" | "htm" => DetectedLanguage {
            syntax_token: "html",
            display_name: "HTML",
        },
        "css" => DetectedLanguage {
            syntax_token: "css",
            display_name: "CSS",
        },
        "scss" => DetectedLanguage {
            syntax_token: "scss",
            display_name: "SCSS",
        },
        "sh" | "bash" | "zsh" => DetectedLanguage {
            syntax_token: "bash",
            display_name: "Shell",
        },
        "fish" => DetectedLanguage {
            syntax_token: "fish",
            display_name: "Fish",
        },
        "ps1" => DetectedLanguage {
            syntax_token: "powershell",
            display_name: "PowerShell",
        },
        "bat" | "cmd" => DetectedLanguage {
            syntax_token: "bat",
            display_name: "Batch",
        },
        "md" | "markdown" => DetectedLanguage {
            syntax_token: "markdown",
            display_name: "Markdown",
        },
        "sql" => DetectedLanguage {
            syntax_token: "sql",
            display_name: "SQL",
        },
        "go" => DetectedLanguage {
            syntax_token: "go",
            display_name: "Go",
        },
        "c" | "h" => DetectedLanguage {
            syntax_token: "c",
            display_name: "C",
        },
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => DetectedLanguage {
            syntax_token: "cpp",
            display_name: "C++",
        },
        "java" => DetectedLanguage {
            syntax_token: "java",
            display_name: "Java",
        },
        "kt" | "kts" => DetectedLanguage {
            syntax_token: "kotlin",
            display_name: "Kotlin",
        },
        "swift" => DetectedLanguage {
            syntax_token: "swift",
            display_name: "Swift",
        },
        "rb" => DetectedLanguage {
            syntax_token: "ruby",
            display_name: "Ruby",
        },
        "php" => DetectedLanguage {
            syntax_token: "php",
            display_name: "PHP",
        },
        "lua" => DetectedLanguage {
            syntax_token: "lua",
            display_name: "Lua",
        },
        "cs" => DetectedLanguage {
            syntax_token: "csharp",
            display_name: "C#",
        },
        "ini" | "cfg" | "conf" | "service" | "socket" | "timer" | "mount" | "properties" => {
            DetectedLanguage {
                syntax_token: "ini",
                display_name: "INI / Conf",
            }
        }
        "diff" | "patch" => DetectedLanguage {
            syntax_token: "diff",
            display_name: "Diff",
        },
        "log" => DetectedLanguage {
            syntax_token: "log",
            display_name: "Log",
        },
        _ => DetectedLanguage {
            syntax_token: "text",
            display_name: "Plain Text",
        },
    }
}

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
                    debug!("[Editor] Detected macOS app bundle, using 'open -a'");
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
}

impl FileWatcher {
    /// 创建新的文件监控器
    pub fn new(event_sender: mpsc::Sender<FileWatchEvent>) -> anyhow::Result<Self> {
        // 使用 Arc<Mutex> 使 watched_files 在 notify 回调和 watch() 方法间共享
        let watched_files: std::sync::Arc<std::sync::Mutex<HashMap<PathBuf, WatchedFile>>> =
            std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
        let watched_files_for_handler = watched_files.clone();

        let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    debug!("[FileWatcher] Raw event: {:?}", event.kind);

                    // 只处理修改事件
                    if event.kind.is_modify() {
                        for path in &event.paths {
                            debug!("[FileWatcher] Event path: {:?}", path);

                            // 需要使用 mut 锁来更新 last_modified
                            if let Ok(mut files) = watched_files_for_handler.lock() {
                                debug!(
                                    "[FileWatcher] Watched files: {:?}",
                                    files.keys().collect::<Vec<_>>()
                                );

                                if let Some(watched) = files.get_mut(path) {
                                    // 检查文件是否真的被修改了（避免重复事件）
                                    if let Ok(metadata) = std::fs::metadata(path) {
                                        if let Ok(modified) = metadata.modified() {
                                            if modified > watched.last_modified {
                                                debug!("[FileWatcher] File modified: {:?}", path);

                                                // 立即更新 last_modified，防止重复事件
                                                watched.last_modified = modified;

                                                let _ =
                                                    event_sender.send(FileWatchEvent::Modified {
                                                        session_id: watched.session_id.clone(),
                                                        local_path: watched.local_path.clone(),
                                                        remote_path: watched.remote_path.clone(),
                                                    });
                                            }
                                        }
                                    }
                                } else {
                                    debug!("[FileWatcher] Path not in watched list: {:?}", path);
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
        })
    }

    /// 开始监控文件
    pub fn watch(&mut self, file: WatchedFile) -> anyhow::Result<()> {
        let path = file.local_path.clone();
        debug!("[FileWatcher] Watching file: {:?}", path);

        // 添加到共享的监控列表
        if let Ok(mut files) = self.watched_files.lock() {
            files.insert(path.clone(), file);
            debug!(
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

    /// 停止监控单个临时文件（预留能力：用于单独解除某个外置编辑文件的自动同步）
    #[allow(dead_code)]
    pub fn unwatch(&mut self, local_path: &Path) -> anyhow::Result<()> {
        debug!("[FileWatcher] Unwatching file: {:?}", local_path);
        let should_unwatch_parent = if let Ok(mut files) = self.watched_files.lock() {
            files.remove(local_path);
            local_path
                .parent()
                .map(|parent| !files.keys().any(|p| p.parent() == Some(parent)))
                .unwrap_or(false)
        } else {
            false
        };

        if should_unwatch_parent {
            if let Some(parent) = local_path.parent() {
                let _ = self._watcher.unwatch(parent);
            }
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
            }

            // 仅当某父目录下不再有任何受监控文件时，才取消对该父目录的文件系统监控
            for path in &removed_paths {
                if let Some(parent) = path.parent() {
                    if !files.keys().any(|p| p.parent() == Some(parent)) {
                        let _ = self._watcher.unwatch(parent);
                    }
                }
            }

            if !removed_paths.is_empty() {
                debug!(
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
                        debug!("[Editor] Removed temp file: {:?}", path);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_binary_and_utf8_text() {
        assert!(is_binary_content(b"\x7fELF\x02\x01\x01\x00"));
        assert!(!is_binary_content("你好，世界\nline2\tcol\r\n".as_bytes()));
        assert!(!is_binary_content(b""));

        assert_eq!(
            decode_utf8_text(b"abc\x00def"),
            Err(TextDecodeError::Binary)
        );
        assert_eq!(
            decode_utf8_text(&[0xFF, 0xFE, 0xFD]),
            Err(TextDecodeError::InvalidUtf8)
        );
    }

    #[test]
    fn preserves_crlf_and_bom_roundtrip() {
        let original = b"\xEF\xBB\xBFline1\r\nline2\r\n";
        let decoded = decode_utf8_text(original).expect("valid utf-8 with bom");
        assert!(decoded.has_bom);
        assert_eq!(decoded.line_ending, LineEnding::CrLf);
        assert_eq!(decoded.content, "line1\nline2\n");

        let reencoded = encode_utf8_text(&decoded.content, decoded.line_ending, decoded.has_bom);
        assert_eq!(reencoded, original);
    }

    #[test]
    fn detects_common_server_languages() {
        assert_eq!(
            detect_language_from_path("/etc/nginx/nginx.conf").syntax_token,
            "nginx"
        );
        assert_eq!(
            detect_language_from_path("/app/Dockerfile").syntax_token,
            "Dockerfile"
        );
        assert_eq!(
            detect_language_from_path("/root/.bashrc").syntax_token,
            "bash"
        );
        assert_eq!(
            detect_language_from_path("/srv/app/.env.production").syntax_token,
            "bash"
        );
        assert_eq!(
            detect_language_from_path("/etc/systemd/system/app.service").syntax_token,
            "ini"
        );
        assert_eq!(
            detect_language_from_path("/src/main.rs").syntax_token,
            "rust"
        );
        assert_eq!(
            detect_language_from_path("/tmp/unknown").syntax_token,
            "text"
        );
    }
}
