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
}

impl ConnectingProgress {
    pub fn new(_tab_id: String) -> Self {
        Self {
            current_stage: ConnectionStage::Initializing,
            logs: Vec::new(),
            error_message: None,
            is_completed: false,
            connection_started: false,
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

    /// 获取进度百分比
    pub fn progress(&self) -> f32 {
        self.current_stage.progress()
    }

    /// 标记连接已启动
    pub fn mark_started(&mut self) {
        self.connection_started = true;
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
    let progress_value = progress.progress();
    let has_error = progress.error_message.is_some();
    let error_msg = progress.error_message.clone();
    let server_label = tab.server_label.clone();
    let tab_id = tab.id.clone();
    let logs = progress.logs.clone();

    let bg_color = crate::theme::background_color(cx);
    let primary = cx.theme().primary;
    let foreground = cx.theme().foreground;
    let muted_foreground = cx.theme().muted_foreground;
    let destructive: Hsla = rgb(0xef4444).into();
    let success_color: Hsla = rgb(0x22c55e).into();
    let warn_color: Hsla = rgb(0xf59e0b).into();

    // 进度条宽度
    let progress_width_val = 420.0_f32;
    let progress_width = px(progress_width_val);
    let filled_width = px(progress_width_val * progress_value);

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

    // 标题文字
    let title = if has_error {
        i18n::t(&lang, "connecting.error_title")
    } else if current_stage == ConnectionStage::Connected {
        match lang {
            Language::Chinese => "连接成功",
            Language::English => "Connected",
        }
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
        .gap_5()
        // 服务器图标
        .child(
            div()
                .w_16()
                .h_16()
                .rounded_2xl()
                .bg(icon_bg)
                .flex()
                .items_center()
                .justify_center()
                .child(render_icon(icons::SERVER, icon_color.into())),
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
                        .text_lg()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(foreground)
                        .child(title),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(muted_foreground)
                        .child(format!("\"{}\"", server_label)),
                ),
        )
        // 进度条或错误信息
        .child(if has_error {
            // 错误信息
            div()
                .w(progress_width)
                .p_4()
                .bg(Hsla::from(destructive).opacity(0.1))
                .rounded_lg()
                .border_1()
                .border_color(destructive)
                .child(
                    div()
                        .text_sm()
                        .text_color(destructive)
                        .child(error_msg.unwrap_or_default()),
                )
                .into_any_element()
        } else {
            // 进度条
            div()
                .w(progress_width)
                .h(px(6.0))
                .bg(cx.theme().secondary)
                .rounded_full()
                .overflow_hidden()
                .child(
                    div()
                        .w(filled_width)
                        .h_full()
                        .bg(if current_stage == ConnectionStage::Connected {
                            success_color
                        } else {
                            primary
                        })
                        .rounded_full(),
                )
                .into_any_element()
        })
        // 动态步骤列表
        .child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .children(display_stages.into_iter().map(|stage| {
                    render_stage(
                        stage,
                        current_stage,
                        &lang,
                        primary,
                        success_color,
                        foreground,
                        muted_foreground,
                    )
                })),
        )
        // 日志区域 - 美化版（显示最近8条日志，正序）
        .child(
            div()
                .w(progress_width)
                .h(px(140.0))
                .overflow_hidden()
                .bg(cx.theme().secondary.opacity(0.15))
                .border_1()
                .border_color(cx.theme().border.opacity(0.3))
                .rounded_lg()
                .p_3()
                .child(
                    div().flex().flex_col().gap(px(4.0)).children(
                        logs.iter()
                            .rev()
                            .take(8)
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
        // 取消按钮（成功后隐藏）
        .child(if current_stage != ConnectionStage::Connected {
            div()
                .id("cancel-connect-btn")
                .mt_2()
                .px_6()
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
                        .text_color(foreground)
                        .child(i18n::t(&lang, "connecting.cancel")),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        })
}

/// 渲染单个阶段
fn render_stage(
    stage: ConnectionStage,
    current: ConnectionStage,
    lang: &Language,
    primary: Hsla,
    success: Hsla,
    foreground: Hsla,
    muted: Hsla,
) -> impl IntoElement {
    let stage_val = stage as u8;
    let current_val = current as u8;

    let is_done = stage_val < current_val;
    let is_current = stage == current;
    let is_connected = stage == ConnectionStage::Connected && current == ConnectionStage::Connected;

    let (icon_color, text_color) = if is_done || is_connected {
        (success, foreground)
    } else if is_current {
        (primary, foreground)
    } else {
        (muted, muted)
    };

    let indicator = if is_done || is_connected {
        "✓"
    } else if is_current {
        "●"
    } else {
        "○"
    };

    // 根据语言获取阶段标签
    let label = match lang {
        Language::Chinese => stage.label_zh(),
        Language::English => stage.label_en(),
    };

    div()
        .flex()
        .items_center()
        .gap_2()
        .py(px(2.0))
        .child(
            div()
                .w_4()
                .text_center()
                .text_xs()
                .text_color(icon_color)
                .child(indicator),
        )
        .child(div().text_sm().text_color(text_color).child(label))
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
                .w(px(52.0))
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
                .child(log.message.clone()),
        )
}
