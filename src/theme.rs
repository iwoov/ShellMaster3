use gpui::*;
use gpui_component::theme::Theme;
use gpui_component::ActiveTheme;
use std::rc::Rc;

/// 初始化全局主题配置
/// 覆盖默认的深色模式主题，使用统一的深蓝色风格
pub fn init(cx: &mut App) {
    // 获取当前全局主题
    // 注意：我们需要先获取当前的配置作为基础，但由于借用规则，我们先克隆需要的配置
    let (mut dark_config, mut light_config) = {
        let theme = Theme::global(cx);
        ((*theme.dark_theme).clone(), (*theme.light_theme).clone())
    };

    // ================== Dark Mode Customization ==================
    // 自定义深色模式颜色
    dark_config.colors.background = Some("#0B1017".into());
    dark_config.colors.foreground = Some("#E7EDF5".into());
    dark_config.colors.popover = Some("#111923".into());
    dark_config.colors.popover_foreground = Some("#E7EDF5".into());
    dark_config.colors.sidebar = Some("#0E151F".into());
    dark_config.colors.sidebar_foreground = Some("#E7EDF5".into());
    dark_config.colors.title_bar = Some("#0E151F".into());

    // Muted/Secondary helps with contrast
    // 统一卡片背景色为深蓝黑色 (与侧边栏一致)，替代默认的灰黑色
    dark_config.colors.muted = Some("#17212D".into());
    dark_config.colors.muted_foreground = Some("#8FA0B5".into());
    dark_config.colors.accent = Some("#1A2D49".into());
    dark_config.colors.accent_foreground = Some("#DCE9FF".into());
    dark_config.colors.list_hover = Some("#172536".into());
    dark_config.colors.table = Some("#111923".into());
    dark_config.colors.table_head = Some("#0E151F".into());
    dark_config.colors.table_even = Some("#121B26".into());
    dark_config.colors.table_hover = Some("#172536".into());
    dark_config.colors.table_row_border = Some("#243140".into());
    dark_config.colors.input = Some("#314155".into());
    dark_config.colors.border = Some("#243140".into());
    dark_config.colors.title_bar_border = Some("#243140".into());

    // 按钮颜色优化 (Dark)
    // Primary: 保持亮蓝色，确保 vibrant
    dark_config.colors.primary = Some("#4C8DFF".into());
    dark_config.colors.primary_hover = Some("#3A7AE6".into());
    dark_config.colors.primary_foreground = Some("#ffffff".into());

    // Secondary: 避免纯黑/深灰，使用 Slate 700 作为按钮底色
    dark_config.colors.secondary = Some("#17212D".into());
    dark_config.colors.secondary_hover = Some("#1E2B3A".into());
    dark_config.colors.secondary_foreground = Some("#E7EDF5".into());

    // ================== Light Mode Customization ==================
    light_config.colors.background = Some("#F4F7FA".into());
    light_config.colors.foreground = Some("#172033".into());
    light_config.colors.popover = Some("#FFFFFF".into());
    light_config.colors.popover_foreground = Some("#172033".into());
    light_config.colors.sidebar = Some("#F9FBFD".into());
    light_config.colors.sidebar_foreground = Some("#172033".into());
    light_config.colors.title_bar = Some("#F9FBFD".into());
    light_config.colors.muted = Some("#EDF2F7".into());
    light_config.colors.muted_foreground = Some("#627085".into());
    light_config.colors.accent = Some("#E6EFFD".into());
    light_config.colors.accent_foreground = Some("#1E4E9D".into());
    light_config.colors.border = Some("#D9E2EC".into());
    light_config.colors.input = Some("#C9D5E3".into());
    light_config.colors.title_bar_border = Some("#D9E2EC".into());
    light_config.colors.list_hover = Some("#EDF3FA".into());
    light_config.colors.table_head = Some("#F4F7FA".into());
    light_config.colors.table_row_border = Some("#D9E2EC".into());

    light_config.colors.primary = Some("#2867D8".into());
    light_config.colors.primary_hover = Some("#1E56BA".into());
    light_config.colors.primary_foreground = Some("#ffffff".into());

    // Secondary: 浅灰色，替代默认可能的高对比度黑色
    light_config.colors.secondary = Some("#EDF2F7".into());
    light_config.colors.secondary_hover = Some("#E2EAF3".into());
    light_config.colors.secondary_foreground = Some("#172033".into());

    // 更新全局主题
    let theme = Theme::global_mut(cx);
    theme.dark_theme = Rc::new(dark_config);
    theme.light_theme = Rc::new(light_config);

    // 应用当前模式的配置
    if theme.mode.is_dark() {
        theme.apply_config(&theme.dark_theme.clone());
    } else {
        theme.apply_config(&theme.light_theme.clone());
    }
}

// 兼容性帮助函数 - 现在的实现直接返回全局主题颜色，
// 因为全局主题已经被我们正确初始化了。

/// 获取适配深色模式的主背景色
pub fn background_color(cx: &App) -> Hsla {
    cx.theme().background
}

/// 获取适配深色模式的弹窗/卡片背景色
pub fn popover_color(cx: &App) -> Hsla {
    cx.theme().popover
}

/// 获取适配深色模式的侧边栏背景色
pub fn sidebar_color(cx: &App) -> Hsla {
    cx.theme().sidebar
}

/// 获取适配深色模式的标题栏背景色
pub fn titlebar_color(cx: &App) -> Hsla {
    cx.theme().title_bar
}
