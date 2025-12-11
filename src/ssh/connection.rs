// SSH 连接管理

use gpui::*;
use std::time::Duration;

use crate::pages::connecting::ConnectingProgress;
use crate::services::storage;
use crate::state::{SessionState, SessionStatus};

/// SSH 连接步骤
pub const CONNECT_STEPS: [&str; 4] = ["初始化连接", "验证身份", "建立安全通道", "启动会话"];

/// 执行 SSH 连接模拟（异步任务）
///
/// 这个函数会被 spawn 调用，执行异步连接操作
/// 将来可以替换为真正的 SSH 连接逻辑
pub async fn run_ssh_connection(
    server_id: String,
    server_label: String,
    tab_id: String,
    progress_state: Entity<ConnectingProgress>,
    session_state: Entity<SessionState>,
    async_cx: &mut AsyncApp,
) {
    // 模拟连接步骤，每步 300ms，总共 1.2s
    for (i, step_name) in CONNECT_STEPS.iter().enumerate() {
        println!(
            "[SSH] [{}] 步骤 {}/4: {}...",
            server_label,
            i + 1,
            step_name
        );

        // 等待 300ms（当前步骤执行中）
        async_cx
            .background_executor()
            .timer(Duration::from_millis(300))
            .await;

        // 步骤完成，推进到下一步
        let _ = async_cx.update(|cx| {
            progress_state.update(cx, |p, cx| {
                p.advance();
                cx.notify(); // 强制刷新 UI
            });
        });
    }

    // 所有步骤完成，更新会话状态为已连接
    println!("[SSH] [{}] 连接成功!", server_label);

    // 更新服务器的最后连接时间
    if let Err(e) = storage::update_server_last_connected(&server_id) {
        eprintln!("[SSH] 更新服务器最后连接时间失败: {}", e);
    }

    let _ = async_cx.update(|cx| {
        session_state.update(cx, |state, cx| {
            state.update_tab_status(&tab_id, SessionStatus::Connected);
            cx.notify(); // 强制刷新 UI
        });
    });
}
