// 应用自动更新服务：从 GitHub Releases 检查版本、流式下载安装包并执行平台原生安装/重启

use futures::StreamExt;
use serde::Deserialize;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt;

/// GitHub 仓库信息
pub const GITHUB_OWNER: &str = "iwoov";
pub const GITHUB_REPO: &str = "ShellMaster3";
/// 当前应用版本号（来自 Cargo.toml）
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub Releases 最新版本 API 地址
fn latest_release_api_url() -> String {
    format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        GITHUB_OWNER, GITHUB_REPO
    )
}

/// 匹配当前平台的安装包资产信息
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
}

/// GitHub Release 摘要信息
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInfo {
    /// 去掉前导 `v` 后的规范版本号，如 `0.1.7`
    pub version: String,
    /// Git tag 名称，如 `v0.1.7`
    pub tag_name: String,
    /// Release 标题
    pub name: String,
    /// Release 说明（Markdown 文本）
    pub body: String,
    /// GitHub Release 网页链接
    pub html_url: String,
    /// 发布时间（ISO 8601）
    pub published_at: String,
    /// 匹配当前 OS/架构的下载资产（若仓库尚未上传对应平台包则为 None）
    pub asset: Option<ReleaseAsset>,
}

/// 更新状态机
#[derive(Clone, Debug, Default)]
pub enum UpdateStatus {
    /// 尚未检查
    #[default]
    Idle,
    /// 正在检查 GitHub Releases
    Checking,
    /// 当前已是最新版本
    UpToDate { checked_at: String },
    /// 发现新版本，等待用户或自动流程下载
    Available(ReleaseInfo),
    /// 正在下载安装包
    Downloading {
        release: ReleaseInfo,
        downloaded: u64,
        total: Option<u64>,
    },
    /// 安装包已下载完成，等待安装并重启
    Downloaded {
        release: ReleaseInfo,
        file_path: PathBuf,
    },
    /// 正在解压/替换/启动安装程序
    Installing { release: ReleaseInfo },
    /// 已打开安装包（例如开发环境非 `.app` 运行、无写权限目录或 Linux `.deb`）
    InstalledOpened {
        message: String,
    },
    /// 检查/下载/安装失败
    Error(String),
}

/// 执行安装后的结果
#[derive(Clone, Debug)]
pub enum InstallOutcome {
    /// 已启动独立更新脚本/安装器并准备接管，应用应立即退出以完成替换与重启
    Restarting,
    /// 已通过系统方式打开安装包（如 macOS DMG / Linux deb），无需强制退出
    OpenedInstaller { message: String },
}

/// GitHub Releases API 响应结构
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<GitHubAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

/// 简化的 SemVer 结构（支持 `v0.1.6`、`0.2.0-beta.1`、`1.2.3+build`）
#[derive(Clone, Debug, PartialEq, Eq)]
struct SemVer {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<String>,
}

impl SemVer {
    fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim().trim_start_matches(['v', 'V']);
        // 忽略 +build 元数据
        let without_build = trimmed.split('+').next()?.trim();
        if without_build.is_empty() {
            return None;
        }
        let (core, pre_str) = match without_build.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (without_build, None),
        };
        let mut parts = core.split('.');
        let major = parts.next()?.parse::<u64>().ok()?;
        let minor = parts.next().unwrap_or("0").parse::<u64>().ok()?;
        let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
        if parts.next().is_some() {
            return None;
        }
        let pre = pre_str
            .filter(|s| !s.is_empty())
            .map(|s| s.split('.').map(|id| id.to_string()).collect())
            .unwrap_or_default();
        Some(Self {
            major,
            minor,
            patch,
            pre,
        })
    }
}

impl Ord for SemVer {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then_with(|| self.minor.cmp(&other.minor))
            .then_with(|| self.patch.cmp(&other.patch))
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                // 正式版 > 预发布版
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(other.pre.iter()) {
                        let ord = match (a.parse::<u64>(), b.parse::<u64>()) {
                            (Ok(na), Ok(nb)) => na.cmp(&nb),
                            (Ok(_), Err(_)) => Ordering::Less,
                            (Err(_), Ok(_)) => Ordering::Greater,
                            (Err(_), Err(_)) => a.cmp(b),
                        };
                        if ord != Ordering::Equal {
                            return ord;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            })
    }
}

impl PartialOrd for SemVer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 判断 `candidate` 是否严格高于 `current`
pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    match (SemVer::parse(current), SemVer::parse(candidate)) {
        (Some(cur), Some(cand)) => cand > cur,
        _ => false,
    }
}

/// 按当前运行平台与架构从 GitHub Release 资产列表中挑选最合适的安装包
pub fn select_platform_asset(
    assets: &[GitHubAsset],
    os: &str,
    arch: &str,
    is_appimage: bool,
) -> Option<ReleaseAsset> {
    let to_release_asset = |a: &GitHubAsset| ReleaseAsset {
        name: a.name.clone(),
        download_url: a.browser_download_url.clone(),
        size: a.size,
    };

    let find_matching = |pred: &dyn Fn(&str) -> bool| -> Option<ReleaseAsset> {
        assets
            .iter()
            .find(|a| pred(&a.name.to_ascii_lowercase()))
            .map(to_release_asset)
    };

    match os {
        "macos" => match arch {
            "aarch64" => find_matching(&|n| {
                n.ends_with(".dmg") && (n.contains("aarch64") || n.contains("arm64"))
            })
            .or_else(|| {
                find_matching(&|n| {
                    (n.ends_with(".tar.gz") || n.ends_with(".zip"))
                        && (n.contains("aarch64") || n.contains("arm64"))
                })
            })
            .or_else(|| find_matching(&|n| n.ends_with(".dmg") && n.contains("universal"))),
            "x86_64" => find_matching(&|n| {
                n.ends_with(".dmg") && (n.contains("x64") || n.contains("x86_64"))
            })
            .or_else(|| {
                find_matching(&|n| {
                    (n.ends_with(".tar.gz") || n.ends_with(".zip"))
                        && (n.contains("x64") || n.contains("x86_64"))
                })
            })
            .or_else(|| find_matching(&|n| n.ends_with(".dmg") && n.contains("universal"))),
            _ => None,
        },
        "windows" => match arch {
            "x86_64" => find_matching(&|n| {
                n.ends_with("-setup.exe") && (n.contains("x64") || n.contains("x86_64"))
            })
            .or_else(|| {
                find_matching(&|n| {
                    n.ends_with(".exe") && (n.contains("x64") || n.contains("x86_64"))
                })
            })
            .or_else(|| {
                find_matching(&|n| {
                    n.ends_with(".msi") && (n.contains("x64") || n.contains("x86_64"))
                })
            }),
            "aarch64" => find_matching(&|n| {
                (n.ends_with(".exe") || n.ends_with(".msi"))
                    && (n.contains("arm64") || n.contains("aarch64"))
            }),
            _ => None,
        },
        "linux" => {
            let arch_matches = |n: &str| match arch {
                "x86_64" => n.contains("x86_64") || n.contains("amd64") || n.contains("x64"),
                "aarch64" => n.contains("aarch64") || n.contains("arm64"),
                _ => false,
            };
            if is_appimage {
                find_matching(&|n| n.ends_with(".appimage") && arch_matches(n))
                    .or_else(|| find_matching(&|n| n.ends_with(".deb") && arch_matches(n)))
            } else {
                // AppImage 最便于独立更新；若不存在则回退到 .deb
                find_matching(&|n| n.ends_with(".appimage") && arch_matches(n))
                    .or_else(|| find_matching(&|n| n.ends_with(".deb") && arch_matches(n)))
            }
        }
        _ => None,
    }
}

/// 构造带 User-Agent 的 HTTP 客户端
fn build_http_client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .user_agent(format!("ShellMaster3/{}", CURRENT_VERSION))
        .build()
        .map_err(|e| format!("初始化 HTTP 客户端失败: {}", e))
}

/// 从 GitHub Releases API 查询最新发布版本
pub async fn fetch_latest_release() -> Result<ReleaseInfo, String> {
    let client = build_http_client(Duration::from_secs(20))?;
    let resp = client
        .get(latest_release_api_url())
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("无法连接 GitHub: {}", e))?;

    let status = resp.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Err("GitHub 仓库暂无已发布版本".to_string());
    }
    if status == reqwest::StatusCode::FORBIDDEN || status == reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return Err("GitHub API 请求频率受限，请稍后再试".to_string());
    }
    if !status.is_success() {
        return Err(format!("GitHub API 返回错误状态码: {}", status));
    }

    let gh: GitHubRelease = resp
        .json()
        .await
        .map_err(|e| format!("解析发布信息失败: {}", e))?;

    if gh.draft {
        return Err("最新版本仍处于草稿状态".to_string());
    }

    let is_appimage = std::env::var_os("APPIMAGE").is_some();
    let asset = select_platform_asset(
        &gh.assets,
        std::env::consts::OS,
        std::env::consts::ARCH,
        is_appimage,
    );

    let version = gh.tag_name.trim_start_matches(['v', 'V']).to_string();
    let name = gh
        .name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| gh.tag_name.clone());

    Ok(ReleaseInfo {
        version,
        tag_name: gh.tag_name,
        name,
        body: gh.body.unwrap_or_default(),
        html_url: gh.html_url,
        published_at: gh.published_at.unwrap_or_default(),
        asset,
    })
}

/// 获取更新包缓存目录（`~/.config/shellmaster/updates` 或系统临时目录）
pub fn get_updates_dir() -> Result<PathBuf, String> {
    let base = crate::services::storage::get_config_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("shellmaster3"));
    let dir = base.join("updates");
    std::fs::create_dir_all(&dir).map_err(|e| format!("无法创建更新缓存目录: {}", e))?;
    Ok(dir)
}

/// 流式下载 Release 安装包到本地缓存目录，实时回调下载进度 `(downloaded, total)`
pub async fn download_release_asset<F>(
    asset: &ReleaseAsset,
    mut on_progress: F,
) -> Result<PathBuf, String>
where
    F: FnMut(u64, Option<u64>),
{
    let updates_dir = get_updates_dir()?;
    let safe_name = Path::new(&asset.name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .ok_or_else(|| "无效的安装包文件名".to_string())?;

    let final_path = updates_dir.join(safe_name);

    // 若本地已有完整缓存，直接复用，避免重复下载
    if asset.size > 0 {
        if let Ok(meta) = std::fs::metadata(&final_path) {
            if meta.len() == asset.size {
                on_progress(asset.size, Some(asset.size));
                return Ok(final_path);
            }
        }
    }

    let part_path = updates_dir.join(format!("{}.part", safe_name));
    let _ = tokio::fs::remove_file(&part_path).await;

    // 大安装包下载不设过短的总超时，避免慢网中断；只设连接超时
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .user_agent(format!("ShellMaster3/{}", CURRENT_VERSION))
        .build()
        .map_err(|e| format!("初始化下载客户端失败: {}", e))?;

    let resp = client
        .get(&asset.download_url)
        .send()
        .await
        .map_err(|e| format!("下载请求失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("下载失败，HTTP 状态码: {}", resp.status()));
    }

    let total = resp
        .content_length()
        .or_else(|| (asset.size > 0).then_some(asset.size));

    let mut file = tokio::fs::File::create(&part_path)
        .await
        .map_err(|e| format!("创建临时下载文件失败: {}", e))?;

    let mut downloaded: u64 = 0;
    let mut last_report = Instant::now();
    let mut stream = resp.bytes_stream();

    on_progress(0, total);

    while let Some(chunk_res) = stream.next().await {
        let chunk = match chunk_res {
            Ok(c) => c,
            Err(e) => {
                let _ = tokio::fs::remove_file(&part_path).await;
                return Err(format!("下载中断: {}", e));
            }
        };
        if let Err(e) = file.write_all(&chunk).await {
            let _ = tokio::fs::remove_file(&part_path).await;
            return Err(format!("写入安装包失败: {}", e));
        }
        downloaded += chunk.len() as u64;
        if last_report.elapsed() >= Duration::from_millis(80) {
            on_progress(downloaded, total);
            last_report = Instant::now();
        }
    }

    if let Err(e) = file.flush().await {
        let _ = tokio::fs::remove_file(&part_path).await;
        return Err(format!("刷新文件缓冲失败: {}", e));
    }
    drop(file);

    if downloaded == 0 {
        let _ = tokio::fs::remove_file(&part_path).await;
        return Err("下载的安装包为空".to_string());
    }
    if let Some(expected) = total {
        if expected > 0 && downloaded != expected {
            let _ = tokio::fs::remove_file(&part_path).await;
            return Err(format!(
                "安装包大小校验失败（期望 {}，实际 {}）",
                format_bytes(expected),
                format_bytes(downloaded)
            ));
        }
    }

    tokio::fs::rename(&part_path, &final_path)
        .await
        .map_err(|e| format!("保存安装包失败: {}", e))?;

    on_progress(downloaded, total.or(Some(downloaded)));
    Ok(final_path)
}

/// 格式化字节数为人类可读字符串
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// 用系统默认浏览器打开外部链接（如 GitHub Release 页面）
pub fn open_external_url(url: &str) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("无效的外部链接".to_string());
    }
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(url).status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .status();
    #[cfg(target_os = "linux")]
    let status = std::process::Command::new("xdg-open").arg(url).status();

    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("打开链接失败（退出码 {:?}）", s.code())),
        Err(e) => Err(format!("无法启动浏览器: {}", e)),
    }
}

/// 安全地进行 POSIX shell 单引号转义
#[cfg(any(unix, test))]
fn shell_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

/// 从可执行文件路径识别所属的 macOS `.app` 包路径
///
/// 典型路径：`/Applications/ShellMaster3.app/Contents/MacOS/shellmaster3`
#[cfg(any(target_os = "macos", test))]
fn detect_macos_app_bundle(exe_path: &Path) -> Option<PathBuf> {
    let macos_dir = exe_path.parent()?;
    if macos_dir.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let contents_dir = macos_dir.parent()?;
    if contents_dir.file_name()?.to_str()? != "Contents" {
        return None;
    }
    let app_bundle = contents_dir.parent()?;
    let ext = app_bundle.extension()?.to_str()?;
    if ext.eq_ignore_ascii_case("app") {
        Some(app_bundle.to_path_buf())
    } else {
        None
    }
}

/// 判断目录是否可写（通过创建并立即删除探针文件来准确验证权限）
#[cfg(target_os = "macos")]
fn is_dir_writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".shellmaster3_write_test_{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 执行平台原生更新安装（阻塞操作，应在后台线程运行）
pub fn install_update(file_path: &Path) -> Result<InstallOutcome, String> {
    if !file_path.exists() {
        return Err("安装包文件不存在，请重新下载".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        install_update_macos(file_path)
    }

    #[cfg(target_os = "windows")]
    {
        install_update_windows(file_path)
    }

    #[cfg(target_os = "linux")]
    {
        install_update_linux(file_path)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = file_path;
        Err("当前操作系统暂不支持自动安装更新".to_string())
    }
}

#[cfg(target_os = "macos")]
fn install_update_macos(file_path: &Path) -> Result<InstallOutcome, String> {
    use std::process::{Command, Stdio};

    let current_exe =
        std::env::current_exe().map_err(|e| format!("无法获取当前程序路径: {}", e))?;
    let Some(target_app) = detect_macos_app_bundle(&current_exe) else {
        // 开发环境直接 `cargo run` 运行时不在 `.app` 包内：打开 DMG 供手动安装
        Command::new("open")
            .arg(file_path)
            .status()
            .map_err(|e| format!("打开安装包失败: {}", e))?;
        return Ok(InstallOutcome::OpenedInstaller {
            message: format!(
                "已打开安装包 {}（当前非 .app 方式运行，请手动拖入应用程序目录）",
                file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
            ),
        });
    };

    let Some(target_parent) = target_app.parent() else {
        return Err("无法确定应用程序安装目录".to_string());
    };

    if !is_dir_writable(target_parent) {
        Command::new("open")
            .arg(file_path)
            .status()
            .map_err(|e| format!("打开安装包失败: {}", e))?;
        return Ok(InstallOutcome::OpenedInstaller {
            message: format!(
                "当前用户对 {} 无写权限，已打开 DMG 安装包，请手动拖入替换",
                target_parent.display()
            ),
        });
    }

    let is_dmg = file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("dmg"))
        .unwrap_or(false);
    if !is_dmg {
        Command::new("open")
            .arg(file_path)
            .status()
            .map_err(|e| format!("打开安装包失败: {}", e))?;
        return Ok(InstallOutcome::OpenedInstaller {
            message: "已打开更新包，请按提示完成安装".to_string(),
        });
    }

    let updates_dir = get_updates_dir()?;
    let pid = std::process::id();
    let mount_dir = updates_dir.join(format!("mount_{}", pid));
    let stage_dir = updates_dir.join(format!("stage_{}", pid));
    let _ = std::fs::remove_dir_all(&mount_dir);
    let _ = std::fs::remove_dir_all(&stage_dir);
    std::fs::create_dir_all(&mount_dir).map_err(|e| format!("创建临时挂载目录失败: {}", e))?;
    std::fs::create_dir_all(&stage_dir).map_err(|e| format!("创建暂存目录失败: {}", e))?;

    // 1. 静默挂载 DMG
    let attach = Command::new("hdiutil")
        .args([
            "attach",
            "-nobrowse",
            "-readonly",
            "-mountpoint",
            mount_dir.to_str().ok_or("挂载路径编码无效")?,
            file_path.to_str().ok_or("安装包路径编码无效")?,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| format!("挂载 DMG 失败: {}", e))?;

    if !attach.status.success() {
        let _ = std::fs::remove_dir_all(&mount_dir);
        let _ = std::fs::remove_dir_all(&stage_dir);
        let stderr = String::from_utf8_lossy(&attach.stderr);
        return Err(format!("挂载 DMG 失败: {}", stderr.trim()));
    }

    // 2. 查找挂载卷中的 .app
    let found_app = std::fs::read_dir(&mount_dir)
        .ok()
        .and_then(|entries| {
            entries.flatten().find_map(|entry| {
                let p = entry.path();
                let is_app = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("app"))
                    .unwrap_or(false);
                if is_app && p.is_dir() {
                    Some(p)
                } else {
                    None
                }
            })
        });

    let staged_app = stage_dir.join(
        target_app
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("ShellMaster3.app"),
    );

    let copy_result = if let Some(src_app) = found_app {
        // 使用 ditto 完整复制 bundle（保留符号链接、资源和签名元数据）
        Command::new("ditto")
            .arg(&src_app)
            .arg(&staged_app)
            .status()
            .map_err(|e| format!("复制新版本应用失败: {}", e))
            .and_then(|s| {
                if s.success() {
                    Ok(())
                } else {
                    Err(format!("ditto 复制失败（退出码 {:?}）", s.code()))
                }
            })
    } else {
        Err("DMG 安装包中未找到 .app 应用".to_string())
    };

    // 3. 无论复制成功与否，始终卸载 DMG
    let _ = Command::new("hdiutil")
        .args(["detach", mount_dir.to_str().unwrap_or_default(), "-force"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = std::fs::remove_dir_all(&mount_dir);

    if let Err(err) = copy_result {
        let _ = std::fs::remove_dir_all(&stage_dir);
        return Err(err);
    }

    // 4. 生成独立后台脚本：等待当前进程退出 -> 原子替换 .app -> 清理 quarantine -> 重新启动
    let backup_app = target_app.with_extension("app.old");
    let script_path = updates_dir.join(format!("apply_update_{}.sh", pid));
    let script_content = format!(
        "#!/bin/sh\n\
         PID={pid}\n\
         TARGET={target}\n\
         STAGED={staged}\n\
         BACKUP={backup}\n\
         STAGE_DIR={stage_dir}\n\
         SELF={self_script}\n\
         i=0\n\
         while kill -0 \"$PID\" 2>/dev/null && [ \"$i\" -lt 60 ]; do\n\
           sleep 0.25\n\
           i=$((i + 1))\n\
         done\n\
         rm -rf \"$BACKUP\"\n\
         if [ -d \"$TARGET\" ]; then\n\
           mv \"$TARGET\" \"$BACKUP\" || exit 1\n\
         fi\n\
         if mv \"$STAGED\" \"$TARGET\"; then\n\
           rm -rf \"$BACKUP\"\n\
           xattr -rd com.apple.quarantine \"$TARGET\" 2>/dev/null || true\n\
           open -n \"$TARGET\"\n\
         else\n\
           if [ -d \"$BACKUP\" ]; then\n\
             mv \"$BACKUP\" \"$TARGET\"\n\
           fi\n\
         fi\n\
         rm -rf \"$STAGE_DIR\"\n\
         rm -f \"$SELF\"\n",
        pid = pid,
        target = shell_quote(&target_app.to_string_lossy()),
        staged = shell_quote(&staged_app.to_string_lossy()),
        backup = shell_quote(&backup_app.to_string_lossy()),
        stage_dir = shell_quote(&stage_dir.to_string_lossy()),
        self_script = shell_quote(&script_path.to_string_lossy()),
    );

    std::fs::write(&script_path, script_content)
        .map_err(|e| format!("写入更新脚本失败: {}", e))?;

    Command::new("sh")
        .arg(&script_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("启动更新脚本失败: {}", e))?;

    Ok(InstallOutcome::Restarting)
}

#[cfg(target_os = "windows")]
fn install_update_windows(file_path: &Path) -> Result<InstallOutcome, String> {
    use std::process::{Command, Stdio};

    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if ext == "msi" {
        Command::new("msiexec")
            .args(["/i", &file_path.to_string_lossy()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("启动 MSI 安装程序失败: {}", e))?;
        return Ok(InstallOutcome::Restarting);
    }

    // 启动 NSIS 安装程序（退出当前应用以便覆盖旧可执行文件）
    Command::new(file_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("启动安装程序失败: {}", e))?;

    Ok(InstallOutcome::Restarting)
}

#[cfg(target_os = "linux")]
fn install_update_linux(file_path: &Path) -> Result<InstallOutcome, String> {
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Stdio};

    let is_appimage_asset = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase().ends_with(".appimage"))
        .unwrap_or(false);

    if let (true, Some(current_appimage)) = (is_appimage_asset, std::env::var_os("APPIMAGE")) {
        let target = PathBuf::from(current_appimage);
        let _ = std::fs::set_permissions(file_path, std::fs::Permissions::from_mode(0o755));

        let updates_dir = get_updates_dir()?;
        let pid = std::process::id();
        let script_path = updates_dir.join(format!("apply_update_{}.sh", pid));
        let script_content = format!(
            "#!/bin/sh\n\
             PID={pid}\n\
             SRC={src}\n\
             TARGET={target}\n\
             SELF={self_script}\n\
             i=0\n\
             while kill -0 \"$PID\" 2>/dev/null && [ \"$i\" -lt 60 ]; do\n\
               sleep 0.25\n\
               i=$((i + 1))\n\
             done\n\
             chmod +x \"$SRC\"\n\
             cp -f \"$SRC\" \"$TARGET.new\" && mv -f \"$TARGET.new\" \"$TARGET\" && chmod +x \"$TARGET\"\n\
             nohup \"$TARGET\" >/dev/null 2>&1 &\n\
             rm -f \"$SELF\"\n",
            pid = pid,
            src = shell_quote(&file_path.to_string_lossy()),
            target = shell_quote(&target.to_string_lossy()),
            self_script = shell_quote(&script_path.to_string_lossy()),
        );
        std::fs::write(&script_path, script_content)
            .map_err(|e| format!("写入更新脚本失败: {}", e))?;
        Command::new("sh")
            .arg(&script_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("启动更新脚本失败: {}", e))?;

        return Ok(InstallOutcome::Restarting);
    }

    // 非 AppImage 运行环境（如 .deb）：使用系统默认程序打开安装包
    Command::new("xdg-open")
        .arg(file_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("打开安装包失败: {}", e))?;

    Ok(InstallOutcome::OpenedInstaller {
        message: format!("已打开安装包 {}", file_path.display()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_v016_assets() -> Vec<GitHubAsset> {
        vec![
            GitHubAsset {
                name: "ShellMaster3_0.1.6_aarch64.dmg".into(),
                browser_download_url: "https://example.com/aarch64.dmg".into(),
                size: 16_000_000,
            },
            GitHubAsset {
                name: "ShellMaster3_0.1.6_x64.dmg".into(),
                browser_download_url: "https://example.com/x64.dmg".into(),
                size: 17_000_000,
            },
            GitHubAsset {
                name: "shellmaster3_0.1.6_x64-setup.exe".into(),
                browser_download_url: "https://example.com/x64-setup.exe".into(),
                size: 18_000_000,
            },
            GitHubAsset {
                name: "shellmaster3_0.1.6_amd64.deb".into(),
                browser_download_url: "https://example.com/amd64.deb".into(),
                size: 15_000_000,
            },
            GitHubAsset {
                name: "shellmaster3_0.1.6_x86_64.AppImage".into(),
                browser_download_url: "https://example.com/x86_64.AppImage".into(),
                size: 20_000_000,
            },
        ]
    }

    #[test]
    fn semver_comparison_handles_prefixes_and_prereleases() {
        assert!(is_newer_version("0.1.6", "v0.1.7"));
        assert!(is_newer_version("v0.1.6", "0.1.10"));
        assert!(is_newer_version("0.1.9", "0.2.0"));
        assert!(is_newer_version("0.2.0-beta.1", "0.2.0"));
        assert!(is_newer_version("0.2.0-beta.1", "0.2.0-beta.2"));
        assert!(is_newer_version("0.2.0-beta.2", "0.2.0-beta.10"));
        assert!(!is_newer_version("0.1.6", "v0.1.6"));
        assert!(!is_newer_version("0.1.7", "v0.1.6"));
        assert!(!is_newer_version("0.2.0", "0.2.0-rc.1"));
        assert!(!is_newer_version("0.1.6", "not-a-version"));
    }

    #[test]
    fn platform_asset_selection_matches_release_artifacts() {
        let assets = sample_v016_assets();

        let mac_arm = select_platform_asset(&assets, "macos", "aarch64", false).unwrap();
        assert_eq!(mac_arm.name, "ShellMaster3_0.1.6_aarch64.dmg");

        let mac_x64 = select_platform_asset(&assets, "macos", "x86_64", false).unwrap();
        assert_eq!(mac_x64.name, "ShellMaster3_0.1.6_x64.dmg");

        let win_x64 = select_platform_asset(&assets, "windows", "x86_64", false).unwrap();
        assert_eq!(win_x64.name, "shellmaster3_0.1.6_x64-setup.exe");

        let linux_appimage = select_platform_asset(&assets, "linux", "x86_64", true).unwrap();
        assert_eq!(linux_appimage.name, "shellmaster3_0.1.6_x86_64.AppImage");

        // 无 AppImage 时应回退到 .deb
        let deb_only: Vec<GitHubAsset> = assets
            .into_iter()
            .filter(|a| !a.name.ends_with(".AppImage"))
            .collect();
        let linux_deb = select_platform_asset(&deb_only, "linux", "x86_64", false).unwrap();
        assert_eq!(linux_deb.name, "shellmaster3_0.1.6_amd64.deb");
    }

    #[test]
    fn detect_macos_app_bundle_only_matches_bundled_executables() {
        assert_eq!(
            detect_macos_app_bundle(Path::new(
                "/Applications/ShellMaster3.app/Contents/MacOS/shellmaster3"
            )),
            Some(PathBuf::from("/Applications/ShellMaster3.app"))
        );
        assert_eq!(
            detect_macos_app_bundle(Path::new("/Users/dev/project/target/debug/shellmaster3")),
            None
        );
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("/Applications/App.app"), "'/Applications/App.app'");
        assert_eq!(shell_quote("/tmp/a'b/c"), "'/tmp/a'\\''b/c'");
    }

    #[test]
    fn format_bytes_human_readable() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2 KB");
        assert_eq!(format_bytes(16 * 1024 * 1024), "16.0 MB");
    }
}
