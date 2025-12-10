// Asset 资源管理

use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use gpui::{AssetSource, SharedString};

/// 组合资源源：优先加载本地资源，找不到时回退到 gpui-component-assets
pub struct Assets {
    pub base: PathBuf,
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // 先尝试加载本地资源
        let local_path = self.base.join(path);
        if local_path.exists() {
            return fs::read(&local_path)
                .map(|data| Some(Cow::Owned(data)))
                .map_err(|err| err.into());
        }

        // 回退到 gpui-component-assets
        gpui_component_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        // 合并本地资源和组件库资源
        let mut results = Vec::new();

        // 列出本地资源
        if let Ok(entries) = fs::read_dir(self.base.join(path)) {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    results.push(SharedString::from(name));
                }
            }
        }

        // 列出组件库资源
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
