// ConnectingPage 连接中页面组件

use gpui::*;
use gpui_component::ActiveTheme;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::i18n;
use crate::models::settings::Language;
use crate::services::storage;
use crate::state::{SessionState, SessionTab};

/// 连接步骤
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConnectStep {
    Initializing = 0,
    Authenticating = 1,
    Establishing = 2,
    Starting = 3,
    Done = 4,
}

impl ConnectStep {
    fn label(&self, lang: &Language) -> &'static str {
        match self {
            ConnectStep::Initializing => i18n::t(lang, "connecting.step.initializing"),
            ConnectStep::Authenticating => i18n::t(lang, "connecting.step.authenticating"),
            ConnectStep::Establishing => i18n::t(lang, "connecting.step.establishing"),
            ConnectStep::Starting => i18n::t(lang, "connecting.step.starting"),
            ConnectStep::Done => i18n::t(lang, "connecting.step.done"),
        }
    }
}

/// 连接进度状态（用于动画）
pub struct ConnectingProgress {
    pub current_step: ConnectStep,
    pub progress: f32, // 0.0 - 1.0
    pub error_message: Option<String>,
    pub simulation_started: bool,
}

impl ConnectingProgress {
    pub fn new(_tab_id: String) -> Self {
        Self {
            current_step: ConnectStep::Initializing,
            progress: 0.0,
            error_message: None,
            simulation_started: false,
        }
    }

    /// 推进到下一步
    pub fn advance(&mut self) {
        match self.current_step {
            ConnectStep::Initializing => {
                self.current_step = ConnectStep::Authenticating;
                self.progress = 0.25;
            }
            ConnectStep::Authenticating => {
                self.current_step = ConnectStep::Establishing;
                self.progress = 0.5;
            }
            ConnectStep::Establishing => {
                self.current_step = ConnectStep::Starting;
                self.progress = 0.75;
            }
            ConnectStep::Starting => {
                self.current_step = ConnectStep::Done;
                self.progress = 1.0;
            }
            ConnectStep::Done => {}
        }
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
    let current_step = progress.current_step;
    let progress_value = progress.progress;
    let has_error = progress.error_message.is_some();
    let error_msg = progress.error_message.clone();
    let server_label = tab.server_label.clone();
    let tab_id = tab.id.clone();

    let bg_color = crate::theme::background_color(cx);
    let primary = cx.theme().primary;
    let foreground = cx.theme().foreground;
    let muted_foreground = cx.theme().muted_foreground;
    let _border = cx.theme().border;
    let _card_bg = cx.theme().popover;
    let destructive: Hsla = rgb(0xef4444).into();

    // 进度条宽度
    let progress_width_val = 400.0_f32;
    let progress_width = px(progress_width_val);
    let filled_width = px(progress_width_val * progress_value);

    // 克隆用于闭包
    let tab_id_for_cancel = tab_id.clone();
    let session_state_for_cancel = session_state.clone();

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
                .w_20()
                .h_20()
                .rounded_2xl()
                .bg(primary.opacity(0.1))
                .flex()
                .items_center()
                .justify_center()
                .child(render_icon(icons::SERVER, primary.into())),
        )
        // 标题
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(foreground)
                        .child(if has_error {
                            i18n::t(&lang, "connecting.error_title")
                        } else {
                            i18n::t(&lang, "connecting.title")
                        }),
                )
                .child(
                    div()
                        .text_base()
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
                .h_2()
                .bg(cx.theme().secondary)
                .rounded_full()
                .overflow_hidden()
                .child(div().w(filled_width).h_full().bg(primary).rounded_full())
                .into_any_element()
        })
        // 步骤列表
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_step(
                    ConnectStep::Initializing,
                    current_step,
                    &lang,
                    primary,
                    foreground,
                    muted_foreground,
                ))
                .child(render_step(
                    ConnectStep::Authenticating,
                    current_step,
                    &lang,
                    primary,
                    foreground,
                    muted_foreground,
                ))
                .child(render_step(
                    ConnectStep::Establishing,
                    current_step,
                    &lang,
                    primary,
                    foreground,
                    muted_foreground,
                ))
                .child(render_step(
                    ConnectStep::Starting,
                    current_step,
                    &lang,
                    primary,
                    foreground,
                    muted_foreground,
                )),
        )
        // 取消按钮
        .child(
            div()
                .id("cancel-connect-btn")
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
                ),
        )
}

/// 渲染单个步骤
fn render_step(
    step: ConnectStep,
    current: ConnectStep,
    lang: &Language,
    primary: Hsla,
    foreground: Hsla,
    muted: Hsla,
) -> impl IntoElement {
    let is_done = step < current;
    let is_current = step == current;

    let (icon_color, text_color) = if is_done {
        (primary, foreground)
    } else if is_current {
        (primary, foreground)
    } else {
        (muted, muted)
    };

    let indicator = if is_done {
        "✓"
    } else if is_current {
        "●"
    } else {
        "○"
    };

    div()
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .w_5()
                .text_center()
                .text_sm()
                .text_color(icon_color)
                .child(indicator),
        )
        .child(
            div()
                .text_sm()
                .text_color(text_color)
                .child(step.label(lang)),
        )
}
