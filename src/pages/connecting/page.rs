// ConnectingPage 连接中页面组件

use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::services::storage;
use crate::ssh::event::{ConnectionStage, LogEntry, LogLevel};
use crate::state::{SessionState, SessionTab};

/// 连接进度状态
pub struct ConnectingProgress {
    /// 当前连接阶段
    pub current_stage: ConnectionStage,
    /// 连接日志
    pub logs: Vec<LogEntry>,
    /// 错误信息
    pub error_message: Option<String>,
    /// 是否已完成
    pub is_completed: bool,
    /// 是否已启动连接
    pub connection_started: bool,
    /// Host key 验证状态
    pub host_key_verification: Option<HostKeyVerificationState>,
    /// Host key 响应发送器（用于发送用户选择）
    host_key_tx: Option<tokio::sync::oneshot::Sender<crate::ssh::event::HostKeyAction>>,
}

/// Host key 验证状态
#[derive(Clone)]
pub struct HostKeyVerificationState {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub is_mismatch: bool, // true 表示密钥已变化（可能安全风险）
}

impl ConnectingProgress {
    pub fn new(_tab_id: String) -> Self {
        Self {
            current_stage: ConnectionStage::Initializing,
            logs: Vec::new(),
            error_message: None,
            is_completed: false,
            connection_started: false,
            host_key_verification: None,
            host_key_tx: None,
        }
    }

    /// 更新连接阶段
    pub fn set_stage(&mut self, stage: ConnectionStage) {
        self.current_stage = stage;
        if stage == ConnectionStage::Connected {
            self.is_completed = true;
        }
    }

    /// 添加日志
    pub fn add_log(&mut self, log: LogEntry) {
        self.logs.push(log);
    }

    /// 设置错误
    pub fn set_error(&mut self, error: String) {
        self.error_message = Some(error);
    }

    /// 标记连接已启动
    pub fn mark_started(&mut self) {
        self.connection_started = true;
    }

    /// 设置 host key 验证状态
    pub fn set_host_key_verification(
        &mut self,
        host: String,
        port: u16,
        key_type: String,
        fingerprint: String,
        is_mismatch: bool,
    ) {
        self.host_key_verification = Some(HostKeyVerificationState {
            host,
            port,
            key_type,
            fingerprint,
            is_mismatch,
        });
    }

    /// 清除 host key 验证状态
    pub fn clear_host_key_verification(&mut self) {
        self.host_key_verification = None;
    }

    /// 设置 host key 响应发送器
    pub fn set_host_key_tx(
        &mut self,
        tx: tokio::sync::oneshot::Sender<crate::ssh::event::HostKeyAction>,
    ) {
        self.host_key_tx = Some(tx);
    }

    /// 取出 host key 响应发送器
    pub fn take_host_key_tx(
        &mut self,
    ) -> Option<tokio::sync::oneshot::Sender<crate::ssh::event::HostKeyAction>> {
        self.host_key_tx.take()
    }
}

/// 渲染连接页面
pub fn render_connecting_page(
    tab: &SessionTab,
    progress_state: Entity<ConnectingProgress>,
    session_state: Entity<SessionState>,
    cx: &App,
) -> impl IntoElement {
    let lang = storage::load_settings()
        .map(|s| s.theme.language)
        .unwrap_or(Language::Chinese);

    let progress = progress_state.read(cx);
    let current_stage = progress.current_stage;
    let has_error = progress.error_message.is_some();
    let error_msg = progress.error_message.clone();
    let server_label = tab.server_label.clone();
    let tab_id = tab.id.clone();
    let logs = progress.logs.clone();
    let host_key_verification = progress.host_key_verification.clone();

    let bg_color = crate::theme::background_color(cx);
    let primary = cx.theme().primary;
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted;
    let muted_foreground = cx.theme().muted_foreground;
    let destructive: Hsla = rgb(0xef4444).into();
    let success_color: Hsla = rgb(0x22c55e).into();
    let warn_color: Hsla = rgb(0xf59e0b).into();

    // 容器宽度
    let container_width = px(640.0);

    // 克隆用于闭包
    let tab_id_for_cancel = tab_id.clone();
    let session_state_for_cancel = session_state.clone();

    // 定义要显示的阶段
    let display_stages = vec![
        ConnectionStage::Initializing,
        ConnectionStage::ConnectingHost,
        ConnectionStage::Handshaking,
        ConnectionStage::Authenticating,
        ConnectionStage::EstablishingChannel,
        ConnectionStage::StartingSession,
        ConnectionStage::Connected,
    ];

    // 计算当前进度索引
    let current_step_index = display_stages
        .iter()
        .rposition(|&s| s <= current_stage)
        .unwrap_or(0);

    // 如果已经连接成功，确保进度填满
    let current_step_index = if current_stage == ConnectionStage::Connected {
        display_stages.len() - 1
    } else {
        current_step_index
    };

    // 计算进度百分比 (0.0 - 1.0)
    let progress_percent = if display_stages.len() > 1 {
        current_step_index as f32 / (display_stages.len() - 1) as f32
    } else {
        0.0
    };

    // 标题文字
    let title = if has_error {
        i18n::t(&lang, "connecting.error_title")
    } else if current_stage == ConnectionStage::Connected {
        i18n::t(&lang, "connecting.connected")
    } else {
        i18n::t(&lang, "connecting.title")
    };

    // 图标颜色
    let icon_bg = if has_error {
        destructive.opacity(0.1)
    } else if current_stage == ConnectionStage::Connected {
        success_color.opacity(0.1)
    } else {
        primary.opacity(0.1)
    };

    let icon_color = if has_error {
        destructive
    } else if current_stage == ConnectionStage::Connected {
        success_color
    } else {
        primary
    };

    div()
        .flex_1()
        .h_full()
        .bg(bg_color)
        .flex()
        .flex_col()
        .justify_center()
        .items_center()
        .gap_8()
        // 服务器图标
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_4()
                .child(
                    div()
                        .w_20()
                        .h_20()
                        .rounded_2xl()
                        .bg(icon_bg)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .w_10()
                                .h_10()
                                .child(render_icon(icons::SERVER, icon_color.into())),
                        ),
                )
                // 标题
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_1()
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(foreground)
                                .child(title),
                        )
                        .child(
                            div()
                                .text_base()
                                .text_color(muted_foreground)
                                .child(format!("\"{}\"", server_label)),
                        ),
                ),
        )
        // 进度步进器 (Stepper)
        .child(
            div()
                .w(container_width)
                .relative()
                .py_2()
                // 背景轨道
                .child(
                    div()
                        .absolute()
                        .top(px(12.0))
                        .left(px(40.0)) // 偏移半个节点容器宽度 (80/2)
                        .right(px(40.0))
                        .h(px(2.0))
                        .bg(muted.opacity(0.3))
                        .rounded_full(),
                )
                // 进度填充
                .child(
                    div()
                        .absolute()
                        .top(px(12.0))
                        .left(px(40.0))
                        .h(px(2.0))
                        .bg(if has_error { destructive } else { primary })
                        .rounded_full()
                        // 总宽度 640 - 80 = 560
                        .w(px(560.0 * progress_percent)),
                )
                // 节点层
                .child(
                    div()
                        .relative()
                        .flex()
                        .justify_between()
                        .items_start()
                        .children(display_stages.into_iter().enumerate().map(|(idx, stage)| {
                            let is_completed = idx <= current_step_index;
                            let is_current = idx == current_step_index;

                            // 节点样式
                            let node_color = if has_error && is_current {
                                destructive
                            } else if is_completed {
                                if current_stage == ConnectionStage::Connected {
                                    success_color
                                } else {
                                    primary
                                }
                            } else {
                                muted // 未完成的颜色
                            };

                            let node_bg = if is_completed {
                                node_color
                            } else {
                                bg_color // 未完成为空心或背景色
                            };

                            let node_border = if is_completed {
                                node_color
                            } else {
                                muted // 未完成的边框
                            };

                            // 获取标签
                            let label = match lang {
                                Language::Chinese => stage.label_zh(),
                                Language::English => stage.label_en(),
                            };

                            // 简化标签，如果是长文本可以考虑截断或换行，这里暂时通过 CSS 处理
                            // 7 个节点，每个节点的宽度大约是 1/7，文字需要居中

                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap_2()
                                .w(px(80.0)) // 固定宽度确保居中对齐
                                .child(
                                    // 节点圆圈
                                    div()
                                        .w(px(10.0))
                                        .h(px(10.0))
                                        .rounded_full()
                                        .bg(node_bg)
                                        .border_2()
                                        .border_color(node_border)
                                        // 增加白色边框使得进度条穿过时有间隔感？或者直接覆盖
                                        // 实际上，如果节点是实心的，它会盖住线。
                                        // 这里的 z-index 默认是按顺序，节点在轨道之后，所以会盖住。
                                        // 添加一个外圈白色以与线隔开？
                                        .child(
                                            div()
                                                .absolute()
                                                .inset_0()
                                                .rounded_full()
                                                .border_2()
                                                .border_color(bg_color)
                                                .h_full()
                                                .w_full(),
                                        ), // 实际上以上做法比较复杂，简单的 z-ordering 就够了。
                                           // 只需设置背景色，若未完成则是灰色背景或空心。
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_center()
                                        .text_color(if is_completed || is_current {
                                            foreground
                                        } else {
                                            muted_foreground
                                        })
                                        .child(label),
                                )
                        })),
                ),
        )
        // 错误信息显示
        .children(if let Some(msg) = error_msg {
            Some(
                div()
                    .w(container_width)
                    .mt_4()
                    .p_3()
                    .bg(destructive.opacity(0.1))
                    .rounded_md()
                    .border_1()
                    .border_color(destructive.opacity(0.3))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().text_sm().text_color(destructive).child(msg)),
            )
        } else {
            None
        })
        // Host Key 验证信息显示
        .children(if let Some(ref hk) = host_key_verification {
            Some(
                div()
                    .w(container_width)
                    .mt_4()
                    .p_4()
                    .bg(if hk.is_mismatch {
                        warn_color.opacity(0.1)
                    } else {
                        primary.opacity(0.1)
                    })
                    .rounded_lg()
                    .border_1()
                    .border_color(if hk.is_mismatch {
                        warn_color.opacity(0.4)
                    } else {
                        primary.opacity(0.3)
                    })
                    .flex()
                    .flex_col()
                    .gap_3()
                    // 标题行
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().w_5().h_5().child(render_icon(
                                if hk.is_mismatch {
                                    icons::X
                                } else {
                                    icons::LOCK
                                },
                                if hk.is_mismatch {
                                    warn_color.into()
                                } else {
                                    primary.into()
                                },
                            )))
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if hk.is_mismatch {
                                        warn_color
                                    } else {
                                        foreground
                                    })
                                    .child(if hk.is_mismatch {
                                        i18n::t(&lang, "connecting.host_key.key_changed")
                                    } else {
                                        i18n::t(&lang, "connecting.host_key.first_connection")
                                    }),
                            ),
                    )
                    // 指纹信息
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(muted_foreground)
                                    .flex_shrink_0()
                                    .child(i18n::t(&lang, "connecting.host_key.fingerprint")),
                            )
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .bg(cx.theme().secondary.opacity(0.3))
                                    .rounded_md()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_family("monospace")
                                            .text_color(foreground)
                                            .child(hk.fingerprint.clone()),
                                    ),
                            ),
                    )
                    // 操作按钮
                    .child({
                        let ps_accept = progress_state.clone();
                        let ps_once = progress_state.clone();
                        let ps_reject = progress_state.clone();

                        div()
                            .flex()
                            .gap_2()
                            .mt_1()
                            // 信任并保存
                            .child(
                                div()
                                    .id("hk-accept-save")
                                    .px_3()
                                    .py(px(6.0))
                                    .bg(primary)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.9))
                                    .on_click(move |_, _, cx| {
                                        ps_accept.update(cx, |state, _| {
                                            if let Some(tx) = state.take_host_key_tx() {
                                                let _ = tx.send(
                                                    crate::ssh::event::HostKeyAction::AcceptAndSave,
                                                );
                                            }
                                        });
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(gpui::white())
                                            .child(i18n::t(
                                                &lang,
                                                "connecting.host_key.btn_accept_save",
                                            )),
                                    ),
                            )
                            // 仅本次信任
                            .child(
                                div()
                                    .id("hk-accept-once")
                                    .px_3()
                                    .py(px(6.0))
                                    .bg(cx.theme().secondary)
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(cx.theme().secondary_hover))
                                    .on_click(move |_, _, cx| {
                                        ps_once.update(cx, |state, _| {
                                            if let Some(tx) = state.take_host_key_tx() {
                                                let _ = tx.send(
                                                    crate::ssh::event::HostKeyAction::AcceptOnce,
                                                );
                                            }
                                        });
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(foreground)
                                            .child(i18n::t(
                                                &lang,
                                                "connecting.host_key.btn_accept_once",
                                            )),
                                    ),
                            )
                            // 拒绝
                            .child(
                                div()
                                    .id("hk-reject")
                                    .px_3()
                                    .py(px(6.0))
                                    .bg(destructive.opacity(0.1))
                                    .rounded_md()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(destructive.opacity(0.2)))
                                    .on_click(move |_, _, cx| {
                                        ps_reject.update(cx, |state, _| {
                                            if let Some(tx) = state.take_host_key_tx() {
                                                let _ = tx
                                                    .send(crate::ssh::event::HostKeyAction::Reject);
                                            }
                                        });
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(destructive)
                                            .child(i18n::t(
                                                &lang,
                                                "connecting.host_key.btn_reject",
                                            )),
                                    ),
                            )
                    }),
            )
        } else {
            None
        })
        // 日志区域
        .child(
            div()
                .w(container_width)
                .h(px(200.0)) // 稍微增加高度
                .overflow_hidden()
                .bg(cx.theme().secondary.opacity(0.15))
                .border_1()
                .border_color(cx.theme().border.opacity(0.3))
                .rounded_lg()
                .p_3()
                .mt_4()
                .child(
                    div().flex().flex_col().gap(px(4.0)).children(
                        logs.iter()
                            .rev()
                            .take(10) // 增加显示的日志数
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                            .map(|log| {
                                render_log_entry(
                                    log,
                                    foreground,
                                    muted_foreground,
                                    warn_color,
                                    destructive,
                                )
                            }),
                    ),
                ),
        )
        // 取消按钮
        .child(if current_stage != ConnectionStage::Connected {
            div()
                .id("cancel-connect-btn")
                .mt_4()
                .px_8()
                .py_2()
                .bg(cx.theme().secondary)
                .rounded_md()
                .cursor_pointer()
                .hover(|s| s.bg(cx.theme().secondary_hover))
                .on_click(move |_, _, cx| {
                    session_state_for_cancel.update(cx, |state, _| {
                        state.close_tab(&tab_id_for_cancel);
                    });
                })
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(foreground)
                        .child(i18n::t(&lang, "connecting.cancel")),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        })
}

/// 渲染日志条目
fn render_log_entry(
    log: &LogEntry,
    foreground: Hsla,
    muted: Hsla,
    warn: Hsla,
    error: Hsla,
) -> impl IntoElement {
    let (level_indicator, text_color) = match log.level {
        LogLevel::Debug => ("◦", muted.opacity(0.7)),
        LogLevel::Info => ("•", foreground.opacity(0.9)),
        LogLevel::Warn => ("▲", warn),
        LogLevel::Error => ("✕", error),
    };

    div()
        .flex()
        .items_start()
        .gap_2()
        .child(
            div()
                .text_xs()
                .text_color(muted.opacity(0.6))
                .w(px(72.0)) // 增加宽度防止时间换行
                .flex_shrink_0()
                .child(format!("[{}]", log.timestamp.format("%H:%M:%S"))),
        )
        .child(
            div()
                .text_xs()
                .text_color(text_color)
                .w_3()
                .flex_shrink_0()
                .child(level_indicator),
        )
        .child(
            div()
                .text_xs()
                .text_color(text_color)
                .flex_1()
                // 允许日志换行
                .whitespace_normal()
                .child(log.message.clone()),
        )
}
