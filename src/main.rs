// ShellMaster - SSH/SFTP 客户端
// 应用入口

use gpui::*;
use std::path::PathBuf;

mod assets;
mod components;
mod constants;
mod models;
mod pages;

// 暂未使用，保留结构
mod router;
mod services;
mod state;
mod theme;

use assets::Assets;
use pages::HomePage;

fn main() {
    Application::new()
        .with_assets(Assets {
            base: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"),
        })
        .run(|cx: &mut App| {
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
                |_, cx| cx.new(|cx| HomePage::new(cx)),
            )
            .unwrap();
            cx.activate(true);
        });
}
