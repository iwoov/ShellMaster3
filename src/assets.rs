// Asset 资源管理

use std::borrow::Cow;

use anyhow::Result;
use gpui_kit::{AssetSource, SharedString};
use rust_embed::RustEmbed;

/// 编译期嵌入的应用资源（assets/ 目录）。
///
/// - release 构建：资源被打进二进制，macOS/Windows/Linux 均为单文件分发，
///   无需在可执行文件旁附带外置 assets 目录。
/// - debug 构建：rust-embed 默认从磁盘按需读取（相对 CARGO_MANIFEST_DIR），
///   保留开发期「改了 SVG 不用重新编译」的体验。
#[derive(RustEmbed)]
#[folder = "assets/"]
struct EmbeddedAssets;

/// 组合资源源：优先使用本应用嵌入的资源，找不到时回退到 gpui-kit-assets。
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // 优先本应用资源
        if let Some(file) = EmbeddedAssets::get(path) {
            return Ok(Some(file.data));
        }

        // 回退到组件库完整内置图标目录（包含 image.svg 等全量 Lucide 图标）
        gpui_kit::assets::AllAssets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        // 本应用资源（返回完整相对路径，与 gpui-kit-assets 行为一致）
        let mut results: Vec<SharedString> = EmbeddedAssets::iter()
            .filter(|p| p.starts_with(path))
            .map(|p| SharedString::from(p.to_string()))
            .collect();

        // 合并组件库资源
        if let Ok(component_assets) = gpui_kit::assets::AllAssets.list(path) {
            for asset in component_assets {
                if !results.contains(&asset) {
                    results.push(asset);
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::Assets;
    use crate::constants::icons;
    use gpui_kit::AssetSource;

    #[test]
    fn all_declared_icons_are_resolvable() {
        let paths = [
            icons::SERVER,
            icons::CODE,
            icons::USER,
            icons::HISTORY,
            icons::HOME,
            icons::SETTINGS,
            icons::PLUS,
            icons::GRID,
            icons::LIST,
            icons::EDIT,
            icons::TRASH,
            icons::COPY,
            icons::TERMINAL,
            icons::DISTRO_UBUNTU,
            icons::DISTRO_DEBIAN,
            icons::DISTRO_CENTOS,
            icons::DISTRO_FEDORA,
            icons::DISTRO_ALPINE,
            icons::DISTRO_ARCH,
            icons::DISTRO_REDHAT,
            icons::DISTRO_ROCKY,
            icons::DISTRO_OPENSUSE,
            icons::LINK,
            icons::GLOBE,
            icons::LOCK,
            icons::FOLDER_OPEN,
            icons::FOLDER,
            icons::CHEVRON_DOWN,
            icons::CHEVRON_UP,
            icons::CHEVRON_RIGHT,
            icons::ELLIPSIS,
            icons::CLOUD,
            icons::LOADER,
            icons::CHECK,
            icons::X,
            icons::UNPLUG,
            icons::PAUSE,
            icons::PLAY,
            icons::MONITOR,
            icons::REFRESH,
            icons::UPLOAD,
            icons::DOWNLOAD,
            icons::PANEL_RIGHT_CLOSE,
            icons::PANEL_RIGHT_OPEN,
            icons::COMMAND,
            icons::TRANSFER,
            icons::FINGERPRINT,
            icons::SEND,
            icons::SPARKLES,
            icons::AI_OPENAI,
            icons::AI_GEMINI,
            icons::AI_CLAUDE,
            icons::AI_DEEPSEEK,
            icons::INFO,
            icons::ARROW_LEFT,
            icons::ARROW_RIGHT,
            icons::ARROW_UP,
            icons::FOLDER_PLUS,
            icons::EYE,
            icons::SEARCH,
            icons::FILE,
            icons::FILE_TEXT,
            icons::FILE_CODE,
            icons::FILE_JSON,
            icons::IMAGE,
            icons::ARCHIVE,
        ];

        for path in paths {
            let loaded = Assets
                .load(path)
                .unwrap_or_else(|e| panic!("failed to load icon {path}: {e}"));
            assert!(
                loaded.is_some_and(|bytes| !bytes.is_empty()),
                "icon {path} resolved to empty or missing data"
            );
        }
    }
}
