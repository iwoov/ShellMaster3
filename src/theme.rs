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
    // 主背景：深蓝色 #20293A
    dark_config.colors.background = Some("#20293A".into());
    // 弹窗/卡片：稍亮的深蓝色 #283446
    dark_config.colors.popover = Some("#283446".into());
    // 侧边栏：更深的蓝黑色 #1A2535
    dark_config.colors.sidebar = Some("#1A2535".into());
    // 标题栏：与侧边栏一致
    dark_config.colors.title_bar = Some("#1A2535".into());

    // Muted/Secondary helps with contrast
    // 统一卡片背景色为深蓝黑色 (与侧边栏一致)，替代默认的灰黑色
    dark_config.colors.muted = Some("#1A2535".into());
    dark_config.colors.muted_foreground = Some("#94a3b8".into()); // slate-400
                                                                  // 列表悬停颜色：明亮的蓝灰色 #334155
    dark_config.colors.list_hover = Some("#334155".into());
    // 输入框边框
    dark_config.colors.input = Some("#475569".into());
    // 边框颜色：更明显的蓝灰色
    dark_config.colors.border = Some("#4a5c72".into());
    // 标题栏边框颜色
    dark_config.colors.title_bar_border = Some("#4a5c72".into());

    // 按钮颜色优化 (Dark)
    // Primary: 保持亮蓝色，确保 vibrant
    dark_config.colors.primary = Some("#3b82f6".into()); // Blue 500
    dark_config.colors.primary_hover = Some("#2563eb".into()); // Blue 600
    dark_config.colors.primary_foreground = Some("#ffffff".into());

    // Secondary: 避免纯黑/深灰，使用 Slate 700 作为按钮底色
    dark_config.colors.secondary = Some("#334155".into()); // Slate 700
    dark_config.colors.secondary_hover = Some("#475569".into()); // Slate 600
    dark_config.colors.secondary_foreground = Some("#ffffff".into());

    // ================== Light Mode Customization ==================
    // 按钮颜色优化 (Light)
    // Primary: 亮蓝色，更有活力 (替代默认的黑/白)
    light_config.colors.primary = Some("#3b82f6".into()); // Blue 500
    light_config.colors.primary_hover = Some("#2563eb".into()); // Blue 600
    light_config.colors.primary_foreground = Some("#ffffff".into());

    // Secondary: 浅灰色，替代默认可能的高对比度黑色
    light_config.colors.secondary = Some("#f1f5f9".into()); // Slate 100
    light_config.colors.secondary_hover = Some("#e2e8f0".into()); // Slate 200
    light_config.colors.secondary_foreground = Some("#0f172a".into()); // Slate 900 (Dark text)

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
