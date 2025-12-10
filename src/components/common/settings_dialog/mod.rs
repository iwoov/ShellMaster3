use gpui::prelude::*;
use gpui::*;

use crate::components::common::icon::render_icon;
use crate::constants::icons;
use crate::models::settings::{AppSettings, Language, ThemeMode};
use crate::services::storage;

/// 设置导航区域类型
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum SettingsSection {
    #[default]
    Theme,
    Terminal,
    KeyBindings,
    Sftp,
    Monitor,
    Connection,
    Sync,
    System,
    About,
}

impl SettingsSection {
    pub fn label(&self) -> &'static str {
        match self {
            SettingsSection::Theme => "主题设置",
            SettingsSection::Terminal => "终端设置",
            SettingsSection::KeyBindings => "按键绑定",
            SettingsSection::Sftp => "SFTP 设置",
            SettingsSection::Monitor => "监控设置",
            SettingsSection::Connection => "连接设置",
            SettingsSection::Sync => "数据同步",
            SettingsSection::System => "系统配置",
            SettingsSection::About => "关于",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            SettingsSection::Theme => icons::GRID,
            SettingsSection::Terminal => icons::TERMINAL,
            SettingsSection::KeyBindings => icons::CODE,
            SettingsSection::Sftp => icons::FOLDER,
            SettingsSection::Monitor => icons::SERVER,
            SettingsSection::Connection => icons::LINK,
            SettingsSection::Sync => icons::GLOBE,
            SettingsSection::System => icons::SETTINGS,
            SettingsSection::About => icons::USER,
        }
    }
}

/// 设置弹窗状态
pub struct SettingsDialogState {
    pub visible: bool,
    pub current_section: SettingsSection,
    pub settings: AppSettings,
    /// 标记设置是否有变更
    pub has_changes: bool,
}

impl Default for SettingsDialogState {
    fn default() -> Self {
        let settings = storage::load_settings().unwrap_or_default();
        Self {
            visible: false,
            current_section: SettingsSection::Theme,
            settings,
            has_changes: false,
        }
    }
}

impl SettingsDialogState {
    pub fn open(&mut self) {
        // 打开时重新加载设置
        self.settings = storage::load_settings().unwrap_or_default();
        self.visible = true;
        self.current_section = SettingsSection::Theme;
        self.has_changes = false;
    }

    pub fn close(&mut self) {
        self.visible = false;
    }

    pub fn save(&mut self) {
        if let Err(e) = storage::save_settings(&self.settings) {
            eprintln!("保存设置失败: {}", e);
        }
        self.has_changes = false;
    }

    /// 标记设置已变更
    pub fn mark_changed(&mut self) {
        self.has_changes = true;
    }
}

/// 渲染设置弹窗覆盖层
pub fn render_settings_dialog_overlay(
    state: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let state_for_close = state.clone();
    let state_for_content = state.clone();

    div()
        .id("settings-dialog-container")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        // 背景遮罩层
        .child(
            div()
                .id("settings-dialog-backdrop")
                .absolute()
                .inset_0()
                .bg(rgba(0x00000080))
                .on_click(move |_, _, cx| {
                    state_for_close.update(cx, |s, _| s.close());
                }),
        )
        // 弹窗内容
        .child(render_dialog_content(state_for_content, cx))
}

/// 渲染弹窗内容
fn render_dialog_content(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_for_nav = state.clone();
    let state_for_cancel = state.clone();
    let state_for_save = state.clone();

    div()
        .id("settings-dialog-content")
        .w(px(800.))
        .h(px(560.))
        .bg(rgb(0xffffff))
        .rounded_lg()
        .shadow_lg()
        .flex()
        .overflow_hidden()
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .child(render_left_nav(state_for_nav))
        .child(render_right_content(
            state,
            state_for_cancel,
            state_for_save,
            cx,
        ))
}

/// 渲染左侧导航菜单
fn render_left_nav(state: Entity<SettingsDialogState>) -> impl IntoElement {
    let sections = [
        SettingsSection::Theme,
        SettingsSection::Terminal,
        SettingsSection::KeyBindings,
        SettingsSection::Sftp,
        SettingsSection::Monitor,
        SettingsSection::Connection,
        SettingsSection::Sync,
        SettingsSection::System,
        SettingsSection::About,
    ];

    div()
        .w(px(180.))
        .h_full()
        .bg(rgb(0xf8fafc))
        .rounded_l_lg()
        .border_r_1()
        .border_color(rgb(0xe2e8f0))
        .flex()
        .flex_col()
        .p_4()
        .gap_1()
        .children(sections.into_iter().map(|section| {
            let state = state.clone();
            render_nav_item(state, section)
        }))
}

/// 渲染导航项
fn render_nav_item(
    state: Entity<SettingsDialogState>,
    section: SettingsSection,
) -> impl IntoElement {
    let state_for_click = state.clone();

    div()
        .id(SharedString::from(format!("settings-nav-{:?}", section)))
        .px_3()
        .py_2()
        .rounded_md()
        .cursor_pointer()
        .flex()
        .items_center()
        .gap_2()
        .hover(|s| s.bg(rgb(0xe2e8f0)))
        .on_click(move |_, _, cx| {
            state_for_click.update(cx, |s, _| {
                s.current_section = section;
            });
        })
        .child(render_icon(section.icon(), rgb(0x64748b).into()))
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x475569))
                .child(section.label()),
        )
}

/// 渲染右侧内容区域
fn render_right_content(
    state: Entity<SettingsDialogState>,
    state_for_cancel: Entity<SettingsDialogState>,
    state_for_save: Entity<SettingsDialogState>,
    cx: &App,
) -> impl IntoElement {
    let state_for_panel = state.clone();

    div()
        .flex_1()
        .h_full()
        .flex()
        .flex_col()
        // 标题栏
        .child(
            div()
                .h(px(56.))
                .flex_shrink_0()
                .border_b_1()
                .border_color(rgb(0xe2e8f0))
                .flex()
                .items_center()
                .px_6()
                .child(
                    div()
                        .text_lg()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x1e293b))
                        .child("设置"),
                ),
        )
        // 内容区域
        .child(
            div()
                .id("settings-form-scroll")
                .flex_1()
                .overflow_scroll()
                .p_6()
                .child(render_section_content(state_for_panel, cx)),
        )
        // 底部按钮
        .child(render_footer_buttons(state_for_cancel, state_for_save))
}

/// 渲染当前分区内容
fn render_section_content(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let section = state.read(cx).current_section;

    match section {
        SettingsSection::Theme => render_theme_panel(state, cx).into_any_element(),
        SettingsSection::Terminal => render_terminal_panel(state, cx).into_any_element(),
        SettingsSection::About => render_about_panel().into_any_element(),
        _ => render_placeholder_panel(section).into_any_element(),
    }
}

/// 渲染底部按钮
fn render_footer_buttons(
    state_for_cancel: Entity<SettingsDialogState>,
    state_for_save: Entity<SettingsDialogState>,
) -> impl IntoElement {
    div()
        .h(px(64.))
        .flex_shrink_0()
        .border_t_1()
        .border_color(rgb(0xe2e8f0))
        .flex()
        .items_center()
        .justify_end()
        .gap_3()
        .px_6()
        // 取消按钮
        .child(
            div()
                .id("settings-cancel-btn")
                .px_4()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(rgb(0xd1d5db))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0xf3f4f6)))
                .on_click(move |_, _, cx| {
                    state_for_cancel.update(cx, |s, _| s.close());
                })
                .child(div().text_sm().text_color(rgb(0x374151)).child("取消")),
        )
        // 保存按钮
        .child(
            div()
                .id("settings-save-btn")
                .px_4()
                .py_2()
                .rounded_md()
                .bg(rgb(0x3b82f6))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(0x2563eb)))
                .on_click(move |_, _, cx| {
                    state_for_save.update(cx, |s, _| {
                        s.save();
                        s.close();
                    });
                })
                .child(div().text_sm().text_color(rgb(0xffffff)).child("保存")),
        )
}

// ======================== 面板实现 ========================

/// 渲染主题设置面板
fn render_theme_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let current_mode = state_read.settings.theme.mode.clone();
    let current_language = state_read.settings.theme.language.clone();

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 语言设置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("语言 / Language"))
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(render_language_button(
                            state.clone(),
                            Language::Chinese,
                            current_language == Language::Chinese,
                        ))
                        .child(render_language_button(
                            state.clone(),
                            Language::English,
                            current_language == Language::English,
                        )),
                ),
        )
        // 外观模式
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("外观模式"))
                .child(
                    div()
                        .flex()
                        .gap_3()
                        .child(render_theme_mode_button(
                            state.clone(),
                            ThemeMode::Light,
                            "浅色模式",
                            current_mode == ThemeMode::Light,
                        ))
                        .child(render_theme_mode_button(
                            state.clone(),
                            ThemeMode::Dark,
                            "深色模式",
                            current_mode == ThemeMode::Dark,
                        ))
                        .child(render_theme_mode_button(
                            state.clone(),
                            ThemeMode::System,
                            "跟随系统",
                            current_mode == ThemeMode::System,
                        )),
                ),
        )
        // 字体设置
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("字体设置"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_setting_row(
                            "界面字体",
                            &state_read.settings.theme.ui_font_family,
                        ))
                        .child(render_setting_row(
                            "界面字号",
                            &format!("{} px", state_read.settings.theme.ui_font_size),
                        )),
                ),
        )
}

/// 渲染语言按钮
fn render_language_button(
    state: Entity<SettingsDialogState>,
    lang: Language,
    selected: bool,
) -> impl IntoElement {
    let bg_color = if selected {
        rgb(0x3b82f6)
    } else {
        rgb(0xf1f5f9)
    };
    let text_color = if selected {
        rgb(0xffffff)
    } else {
        rgb(0x475569)
    };
    let label = lang.label();

    div()
        .id(SharedString::from(format!("lang-{:?}", lang)))
        .px_4()
        .py_2()
        .rounded_md()
        .bg(bg_color)
        .cursor_pointer()
        .hover(|s| {
            if selected {
                s.bg(rgb(0x2563eb))
            } else {
                s.bg(rgb(0xe2e8f0))
            }
        })
        .on_click(move |_, _, cx| {
            state.update(cx, |s, _| {
                s.settings.theme.language = lang.clone();
                s.mark_changed();
            });
        })
        .child(div().text_sm().text_color(text_color).child(label))
}

/// 渲染主题模式按钮
fn render_theme_mode_button(
    state: Entity<SettingsDialogState>,
    mode: ThemeMode,
    label: &'static str,
    selected: bool,
) -> impl IntoElement {
    let bg_color = if selected {
        rgb(0x3b82f6)
    } else {
        rgb(0xf1f5f9)
    };
    let text_color = if selected {
        rgb(0xffffff)
    } else {
        rgb(0x475569)
    };

    div()
        .id(SharedString::from(format!("theme-mode-{:?}", mode)))
        .px_4()
        .py_2()
        .rounded_md()
        .bg(bg_color)
        .cursor_pointer()
        .hover(|s| {
            if selected {
                s.bg(rgb(0x2563eb))
            } else {
                s.bg(rgb(0xe2e8f0))
            }
        })
        .on_click(move |_, _, cx| {
            state.update(cx, |s, _| {
                s.settings.theme.mode = mode.clone();
                s.mark_changed();
            });
        })
        .child(div().text_sm().text_color(text_color).child(label))
}

/// 渲染终端设置面板
fn render_terminal_panel(state: Entity<SettingsDialogState>, cx: &App) -> impl IntoElement {
    let state_read = state.read(cx);
    let terminal = &state_read.settings.terminal;

    div()
        .flex()
        .flex_col()
        .gap_6()
        // 字体
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("字体"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_setting_row("终端字体", &terminal.font_family))
                        .child(render_setting_row(
                            "字号",
                            &format!("{} px", terminal.font_size),
                        ))
                        .child(render_setting_row(
                            "行高",
                            &format!("{:.1}", terminal.line_height),
                        ))
                        .child(render_setting_row(
                            "启用连字",
                            if terminal.ligatures { "是" } else { "否" },
                        )),
                ),
        )
        // 配色
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("配色方案"))
                .child(render_setting_row("终端主题", &terminal.color_scheme)),
        )
        // 显示
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(render_section_title("显示"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(render_setting_row(
                            "光标闪烁",
                            if terminal.cursor_blink { "是" } else { "否" },
                        ))
                        .child(render_setting_row(
                            "滚动缓冲区",
                            &format!("{} 行", terminal.scrollback_lines),
                        )),
                ),
        )
}

/// 渲染关于面板
fn render_about_panel() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_6()
        .pt_8()
        // Logo / 应用名
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .w(px(64.))
                        .h(px(64.))
                        .rounded_xl()
                        .bg(rgb(0x3b82f6))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .text_xl()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgb(0xffffff))
                                .child("SM"),
                        ),
                )
                .child(
                    div()
                        .text_xl()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(0x1e293b))
                        .child("ShellMaster"),
                )
                .child(div().text_sm().text_color(rgb(0x64748b)).child("v1.0.0")),
        )
        // 技术信息
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(render_about_row("平台", "macOS"))
                .child(render_about_row("架构", std::env::consts::ARCH))
                .child(render_about_row("Rust", env!("CARGO_PKG_RUST_VERSION"))),
        )
        // 版权
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x9ca3af))
                .child("© 2024 ShellMaster. All rights reserved."),
        )
}

/// 渲染占位面板
fn render_placeholder_panel(section: SettingsSection) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .h(px(300.))
        .child(
            div()
                .text_lg()
                .text_color(rgb(0x9ca3af))
                .child(format!("{} 设置将在后续版本实现", section.label())),
        )
}

// ======================== 辅助渲染函数 ========================

fn render_section_title(title: &'static str) -> impl IntoElement {
    div()
        .text_base()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(0x1e293b))
        .child(title)
}

fn render_setting_row(label: &'static str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .py_2()
        .child(div().text_sm().text_color(rgb(0x475569)).child(label))
        .child(
            div()
                .text_sm()
                .text_color(rgb(0x64748b))
                .child(value.to_string()),
        )
}

fn render_about_row(label: &'static str, value: &'static str) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_4()
        .child(
            div()
                .w(px(80.))
                .text_sm()
                .text_color(rgb(0x64748b))
                .child(label),
        )
        .child(div().text_sm().text_color(rgb(0x1e293b)).child(value))
}
