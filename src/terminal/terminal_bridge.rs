// 终端桥接器 - 连接 PTY channel 和终端状态
// 负责尺寸计算、PTY 数据读取循环

use std::sync::Arc;

use gpui::*;

use crate::models::settings::TerminalSettings;
use crate::ssh::session::{PtyRequest, TerminalChannel};
use crate::terminal::TerminalState;

/// 计算终端尺寸
///
/// 根据渲染区域像素尺寸和字体设置，计算终端的列数和行数
pub fn calculate_terminal_size(
    area_width: f32,
    area_height: f32,
    settings: &TerminalSettings,
) -> (u32, u32, f32, f32) {
    // 等宽字体：字符宽度约为字体大小的 0.6 倍
    let cell_width = settings.font_size as f32 * 0.6;
    let line_height = settings.font_size as f32 * settings.line_height;

    let cols = (area_width / cell_width).floor() as u32;
    let rows = (area_height / line_height).floor() as u32;

    (cols.max(1), rows.max(1), cell_width, line_height)
}

/// 根据渲染区域尺寸创建 PTY 请求
pub fn create_pty_request(
    area_width: f32,
    area_height: f32,
    settings: &TerminalSettings,
) -> PtyRequest {
    let (cols, rows, _, _) = calculate_terminal_size(area_width, area_height, settings);

    PtyRequest {
        term: "xterm-256color".to_string(),
        col_width: cols,
        row_height: rows,
        pix_width: area_width as u32,
        pix_height: area_height as u32,
        modes: vec![],
    }
}

/// 启动 PTY 读取循环 (fire-and-forget)
/// 读取循环会持续运行直到通道关闭
pub fn start_pty_reader(channel: Arc<TerminalChannel>, terminal: Entity<TerminalState>, cx: &App) {
    // 使用与 connector.rs 相同的 spawn 模式
    cx.spawn(async move |async_cx| {
        println!("[PTY Reader] Started");

        loop {
            // 读取 PTY 输出
            let result = channel.read().await;
            match result {
                Ok(Some(data)) if !data.is_empty() => {
                    println!("[PTY Reader] Received {} bytes", data.len());
                    // 将数据喂给终端
                    let terminal_clone = terminal.clone();
                    let _ = async_cx.update(|cx| {
                        terminal_clone.update(cx, |t, cx| {
                            t.input(&data);
                            cx.notify();
                        });
                    });
                }
                Ok(Some(_)) => {
                    // 空数据，短暂等待后继续
                    async_cx
                        .background_executor()
                        .timer(std::time::Duration::from_millis(10))
                        .await;
                }
                Ok(None) => {
                    println!("[PTY Reader] Channel closed");
                    break;
                }
                Err(e) => {
                    eprintln!("[PTY Reader] Error: {:?}", e);
                    break;
                }
            }
        }

        println!("[PTY Reader] Stopped");
    })
    .detach();
}

/// 发送数据到 PTY
pub async fn send_to_pty(channel: &TerminalChannel, data: &[u8]) {
    if let Err(e) = channel.write(data).await {
        eprintln!("[PTY] Write error: {:?}", e);
    }
}
