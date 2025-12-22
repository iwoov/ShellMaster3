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
mod terminal;
mod theme;

use assets::Assets;
use gpui_component::theme::{Theme, ThemeMode as GpuiThemeMode};
use models::settings::ThemeMode;
use pages::HomePage;
use services::storage;

/// 获取资源目录路径
/// 在开发环境中使用项目的 assets 目录，在 .app 包中使用 Resources 目录
fn get_assets_path() -> PathBuf {
    // 首先尝试从可执行文件的位置推断 .app 包中的 Resources 目录
    if let Ok(exe_path) = std::env::current_exe() {
        // 在 .app 包中，可执行文件位于：ShellMaster3.app/Contents/MacOS/shellmaster3
        // Resources 目录位于：ShellMaster3.app/Contents/Resources/
        if let Some(parent) = exe_path.parent() {
            if parent.ends_with("MacOS") {
                if let Some(contents) = parent.parent() {
                    let resources = contents.join("Resources").join("assets");
                    if resources.exists() {
                        return resources;
                    }
                }
            }
        }
    }

    // 开发环境：使用 CARGO_MANIFEST_DIR
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

fn main() {
    // 初始化日志系统
    // 可以通过 RUST_LOG 环境变量控制日志级别，例如：RUST_LOG=debug cargo run
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_target(false) // 不显示 target（模块路径）
        .init();

    Application::new()
        .with_assets(Assets {
            base: get_assets_path(),
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

            // 初始化终端模块（注册 Terminal 上下文的按键绑定）
            crate::terminal::init(cx);

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
