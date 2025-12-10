use crate::models::settings::Language;

pub fn t(lang: &Language, key: &'static str) -> &'static str {
    match lang {
        Language::Chinese => zh_cn(key),
        Language::English => en_us(key),
    }
}

fn zh_cn(key: &'static str) -> &'static str {
    match key {
        // 通用
        "common.save" => "保存",
        "common.cancel" => "取消",
        "common.confirm" => "确认",

        // 设置菜单
        "settings.title" => "设置",
        "settings.nav.theme" => "主题设置",
        "settings.nav.terminal" => "终端设置",
        "settings.nav.keybindings" => "按键绑定",
        "settings.nav.sftp" => "SFTP 设置",
        "settings.nav.monitor" => "监控设置",
        "settings.nav.connection" => "连接设置",
        "settings.nav.sync" => "数据同步",
        "settings.nav.system" => "系统配置",
        "settings.nav.about" => "关于",

        // 主题设置
        "settings.theme.language" => "语言 / Language",
        "settings.theme.mode" => "外观模式",
        "settings.theme.mode.light" => "浅色模式",
        "settings.theme.mode.dark" => "深色模式",
        "settings.theme.mode.system" => "跟随系统",
        "settings.theme.font" => "字体设置",
        "settings.theme.font_family" => "界面字体",
        "settings.theme.font_size" => "界面字号",

        _ => key,
    }
}

fn en_us(key: &'static str) -> &'static str {
    match key {
        // Common
        "common.save" => "Save",
        "common.cancel" => "Cancel",
        "common.confirm" => "Confirm",

        // Settings Menu
        "settings.title" => "Settings",
        "settings.nav.theme" => "Theme",
        "settings.nav.terminal" => "Terminal",
        "settings.nav.keybindings" => "Key Bindings",
        "settings.nav.sftp" => "SFTP",
        "settings.nav.monitor" => "Monitor",
        "settings.nav.connection" => "Connection",
        "settings.nav.sync" => "Sync",
        "settings.nav.system" => "System",
        "settings.nav.about" => "About",

        // Theme Settings
        "settings.theme.language" => "Language",
        "settings.theme.mode" => "Appearance",
        "settings.theme.mode.light" => "Light",
        "settings.theme.mode.dark" => "Dark",
        "settings.theme.mode.system" => "System",
        "settings.theme.font" => "Font",
        "settings.theme.font_family" => "UI Font",
        "settings.theme.font_size" => "UI Font Size",

        _ => key,
    }
}
