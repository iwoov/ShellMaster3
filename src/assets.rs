// Asset 资源管理

use std::borrow::Cow;

use anyhow::Result;
use gpui::{AssetSource, SharedString};
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

/// 组合资源源：优先使用本应用嵌入的资源，找不到时回退到 gpui-component-assets。
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // 优先本应用资源
        if let Some(file) = EmbeddedAssets::get(path) {
            return Ok(Some(file.data));
        }

        // 回退到组件库内置图标
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        // 本应用资源（返回完整相对路径，与 gpui-component-assets 行为一致）
        let mut results: Vec<SharedString> = EmbeddedAssets::iter()
            .filter(|p| p.starts_with(path))
            .map(|p| SharedString::from(p.to_string()))
            .collect();

        // 合并组件库资源
        if let Ok(component_assets) = gpui_component_assets::Assets.list(path) {
            for asset in component_assets {
                if !results.contains(&asset) {
                    results.push(asset);
                }
            }
        }

        Ok(results)
    }
}
