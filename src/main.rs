// ShellMaster - SSH/SFTP 客户端
// 应用入口

use gpui::*;
use gpui_component::Root;
use std::path::PathBuf;

mod assets;
mod components;
mod constants;
mod models;
mod pages;

// 暂未使用，保留结构
mod i18n;
mod router;
mod services;
mod ssh;
mod state;
mod theme;

use assets::Assets;
use gpui_component::theme::{Theme, ThemeMode as GpuiThemeMode};
use models::settings::ThemeMode;
use pages::HomePage;
use services::storage;

fn main() {
    Application::new()
        .with_assets(Assets {
            base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        })
        .run(|cx: &mut App| {
            // 初始化 gpui-component 组件库（必须在使用任何组件之前调用）
            gpui_component::init(cx);

            // 根据保存的设置初始化主题模式
            if let Ok(settings) = storage::load_settings() {
                match settings.theme.mode {
                    ThemeMode::Light => Theme::change(GpuiThemeMode::Light, None, cx),
                    ThemeMode::Dark => Theme::change(GpuiThemeMode::Dark, None, cx),
                    ThemeMode::System => {} // 默认已跟随系统
                }
            }

            // 应用自定义全局主题配置（覆盖默认深色模式颜色）
            crate::theme::init(cx);

            let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(16.), px(16.))),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| HomePage::new(cx));
                    // 使用 Root 包装视图，这是 gpui-component 的要求
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .unwrap();
            cx.activate(true);
        });
}
