use gpui_kit::*;
use gpui_kit::component::theme::Theme;
use gpui_kit::component::ActiveTheme;
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
    dark_config.colors.list_active = Some("#172230".into());
    dark_config.colors.list_active_border = Some("#00000000".into());
    // 表格（SFTP 文件列表）：与内容区同底色，悬停 / 选中取主页色板，选中行不画边框，行间只留很淡的分隔线
    dark_config.colors.table = Some("#0B1017".into());
    dark_config.colors.table_head = Some("#0B1017".into());
    dark_config.colors.table_even = Some("#0B1017".into());
    dark_config.colors.table_hover = Some("#131C28".into());
    dark_config.colors.table_active = Some("#4C8DFF29".into());
    dark_config.colors.table_active_border = Some("#00000000".into());
    dark_config.colors.table_row_border = Some("#151E29".into());
    dark_config.colors.drop_target = Some("#4C8DFF14".into());
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

    // 组件库控件：输入框聚焦边框、开关关闭态、删除等危险操作按钮（默认值是灰色系，与主题不搭）
    dark_config.colors.ring = Some("#4C8DFF".into());
    dark_config.colors.switch = Some("#2B3747".into());
    dark_config.colors.danger = Some("#E5484D".into());
    dark_config.colors.danger_hover = Some("#D13F44".into());
    dark_config.colors.danger_active = Some("#B8373B".into());
    dark_config.colors.danger_foreground = Some("#FFFFFF".into());

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
    light_config.colors.list_active = Some("#E8EEF6".into());
    light_config.colors.list_active_border = Some("#00000000".into());
    light_config.colors.table = Some("#F4F7FA".into());
    light_config.colors.table_head = Some("#F4F7FA".into());
    light_config.colors.table_even = Some("#F4F7FA".into());
    light_config.colors.table_hover = Some("#E9EFF6".into());
    light_config.colors.table_active = Some("#2867D824".into());
    light_config.colors.table_active_border = Some("#00000000".into());
    light_config.colors.table_row_border = Some("#E4EAF1".into());
    light_config.colors.drop_target = Some("#2867D814".into());

    light_config.colors.primary = Some("#2867D8".into());
    light_config.colors.primary_hover = Some("#1E56BA".into());
    light_config.colors.primary_foreground = Some("#ffffff".into());

    // Secondary: 浅灰色，替代默认可能的高对比度黑色
    light_config.colors.secondary = Some("#EDF2F7".into());
    light_config.colors.secondary_hover = Some("#E2EAF3".into());
    light_config.colors.secondary_foreground = Some("#172033".into());

    light_config.colors.ring = Some("#2867D8".into());
    light_config.colors.switch = Some("#CBD5E1".into());
    light_config.colors.danger = Some("#DC2626".into());
    light_config.colors.danger_hover = Some("#C42020".into());
    light_config.colors.danger_active = Some("#A91B1B".into());
    light_config.colors.danger_foreground = Some("#FFFFFF".into());

    // 更新全局主题并同步 tokens 与 gpui-base 派生主题（滚动条、分隔条、文本默认样式等）
    let current_mode = {
        let theme = Theme::global_mut(cx);
        theme.dark_theme = Rc::new(dark_config);
        theme.light_theme = Rc::new(light_config);
        theme.notification.placement = Anchor::TopCenter;
        theme.mode
    };
    Theme::change(current_mode, None, cx);
}

// 兼容性帮助函数 - 现在的实现直接返回全局主题颜色，
// 因为全局主题已经被我们正确初始化了。

/// 获取适配深色模式的主背景色
pub fn background_color(cx: &App) -> Hsla {
    cx.theme().background
}

/// 获取适配深色模式的标题栏背景色
pub fn titlebar_color(cx: &App) -> Hsla {
    cx.theme().title_bar
}

/// 主页和弹窗使用的界面色板。
///
/// gpui-component 的主题只有一档 hover / muted，列表、卡片、表单需要更细的层次
/// （行悬停、选中、内嵌底色、三档文字），这里按深浅模式补齐，其余直接取主题色。
#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Hsla,
    pub sidebar: Hsla,
    /// 卡片、输入框的底色
    pub surface: Hsla,
    /// 卡片内的内嵌区域（地址条）
    pub inset: Hsla,
    /// 次级按钮底色
    pub muted: Hsla,
    /// 行、导航项、标签的悬停底色
    pub hover: Hsla,
    /// 悬停行内按钮的悬停底色、选中的切换项
    pub hover_strong: Hsla,
    /// 选中的导航项
    pub selected: Hsla,
    pub border: Hsla,
    /// 分组标题后的细线
    pub line: Hsla,
    pub text: Hsla,
    /// 地址中的主机部分
    pub text_soft: Hsla,
    /// 描述、未选中的导航项
    pub text_muted: Hsla,
    /// 计数、时间、占位文字
    pub text_faint: Hsla,
    pub primary: Hsla,
    pub primary_hover: Hsla,
    pub primary_foreground: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
    pub danger_hover: Hsla,
    /// 弹窗遮罩
    pub overlay: Hsla,
    /// 输入框、下拉框、分段切换的底色和边框（与组件库输入框一致）
    pub field: Hsla,
    pub field_border: Hsla,
    pub is_dark: bool,
}

/// 取当前模式下的主页色板
pub fn palette(cx: &App) -> Palette {
    let theme = cx.theme();
    let is_dark = theme.mode.is_dark();
    let pick = |dark: u32, light: u32| -> Hsla { rgb(if is_dark { dark } else { light }).into() };

    Palette {
        background: theme.background,
        sidebar: theme.sidebar,
        surface: theme.popover,
        inset: pick(0x0C1219, 0xF4F7FA),
        muted: theme.secondary,
        hover: pick(0x131C28, 0xE9EFF6),
        hover_strong: theme.secondary_hover,
        selected: pick(0x172230, 0xE8EEF6),
        border: theme.border,
        line: theme.border.opacity(if is_dark { 0.7 } else { 0.9 }),
        text: theme.foreground,
        text_soft: pick(0xB6C2D1, 0x3B485D),
        text_muted: theme.muted_foreground,
        text_faint: pick(0x71829A, 0x627085),
        primary: theme.primary,
        primary_hover: theme.primary_hover,
        primary_foreground: theme.primary_foreground,
        success: pick(0x22C55E, 0x16A34A),
        warning: pick(0xF59E0B, 0xB45309),
        danger: theme.danger,
        danger_hover: theme.danger_hover,
        overlay: if is_dark {
            gpui_kit::black().opacity(0.55)
        } else {
            Hsla::from(rgb(0x0F172A)).opacity(0.3)
        },
        field: theme.background,
        field_border: theme.input,
        is_dark,
    }
}

/// 在 RGB 空间按比例混合两种颜色：`t = 0` 得到 `from`，`t = 1` 得到 `to`
pub fn mix(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let (a, b) = (Rgba::from(from), Rgba::from(to));
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    Rgba {
        r: lerp(a.r, b.r),
        g: lerp(a.g, b.g),
        b: lerp(a.b, b.b),
        a: lerp(a.a, b.a),
    }
    .into()
}
