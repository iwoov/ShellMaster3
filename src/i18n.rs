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
        "common.loading" => "加载中...",
        "common.edit" => "编辑",
        "common.delete" => "删除",

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

        // 终端设置
        "settings.terminal.font" => "字体",
        "settings.terminal.font_family" => "终端字体",
        "settings.terminal.font_size" => "字号",
        "settings.terminal.line_height" => "行高",
        "settings.terminal.ligatures" => "启用连字",
        "settings.terminal.color_scheme" => "配色方案",
        "settings.terminal.theme" => "终端主题",
        "settings.terminal.display" => "显示",
        "settings.terminal.cursor_blink" => "光标闪烁",
        "settings.terminal.scrollback" => "滚动缓冲区",

        // 按键绑定
        "settings.keybindings.coming_soon" => "按键绑定编辑器将在后续版本实现",
        "settings.keybindings.description" => "可自定义终端和SFTP快捷键",

        // SFTP 设置
        "settings.sftp.file_display" => "文件显示",
        "settings.sftp.show_hidden" => "显示隐藏文件",
        "settings.sftp.folders_first" => "文件夹优先",
        "settings.sftp.transfer" => "传输设置",
        "settings.sftp.concurrent" => "并发传输数",
        "settings.sftp.preserve_time" => "保留时间戳",
        "settings.sftp.resume" => "断点续传",
        "settings.sftp.editor" => "编辑器",
        "settings.sftp.builtin_editor" => "使用内置编辑器",
        "settings.sftp.syntax_highlight" => "语法高亮",

        // 监控设置
        "settings.monitor.data_collection" => "数据采集",
        "settings.monitor.history_retention" => "历史保留(分钟)",
        "settings.monitor.auto_deploy" => "自动部署Agent",
        "settings.monitor.display_items" => "显示项目",
        "settings.monitor.cpu" => "CPU",
        "settings.monitor.memory" => "内存",
        "settings.monitor.disk" => "磁盘",
        "settings.monitor.network" => "网络",
        "settings.monitor.alerts" => "告警阈值",
        "settings.monitor.cpu_threshold" => "CPU (%)",
        "settings.monitor.memory_threshold" => "内存 (%)",
        "settings.monitor.disk_threshold" => "磁盘 (%)",

        // 连接设置
        "settings.connection.ssh" => "SSH 设置",
        "settings.connection.default_port" => "默认端口",
        "settings.connection.timeout" => "连接超时(秒)",
        "settings.connection.keepalive" => "心跳间隔(秒)",
        "settings.connection.compression" => "启用压缩",
        "settings.connection.reconnect" => "自动重连",
        "settings.connection.reconnect_enabled" => "自动重连",
        "settings.connection.reconnect_attempts" => "重连次数",
        "settings.connection.reconnect_interval" => "重连间隔(秒)",

        // 数据同步
        "settings.sync.status" => "同步状态",
        "settings.sync.enabled" => "启用同步",
        "settings.sync.auto" => "自动同步",
        "settings.sync.content" => "同步内容",
        "settings.sync.servers" => "服务器配置",
        "settings.sync.groups" => "分组信息",
        "settings.sync.settings" => "应用设置",
        "settings.sync.keybindings" => "快捷键",
        "settings.sync.webdav" => "WebDAV 配置",
        "settings.sync.webdav_url" => "服务器地址",
        "settings.sync.webdav_username" => "用户名",
        "settings.sync.webdav_password" => "密码",
        "settings.sync.webdav_path" => "存储路径",
        "settings.sync.test_connection" => "测试连接",
        "settings.sync.sync_now" => "立即同步",

        // 系统配置
        "settings.system.startup" => "启动",
        "settings.system.auto_start" => "开机启动",
        "settings.system.start_minimized" => "启动时最小化",
        "settings.system.check_updates" => "检查更新",
        "settings.system.window" => "窗口",
        "settings.system.close_to_tray" => "关闭到托盘",
        "settings.system.show_tray_icon" => "显示托盘图标",
        "settings.system.single_instance" => "单实例运行",
        "settings.system.notification" => "通知",
        "settings.system.notify_disconnect" => "断开连接通知",
        "settings.system.notify_transfer" => "传输完成通知",
        "settings.system.logging" => "日志",
        "settings.system.logging_enabled" => "启用日志",
        "settings.system.log_retention" => "日志保留(天)",

        // 关于
        "settings.about.platform" => "平台",
        "settings.about.arch" => "架构",
        "settings.about.copyright" => "© 2024 ShellMaster. All rights reserved.",

        // 侧边栏
        "sidebar.hosts" => "服务器",
        "sidebar.monitor" => "主机监控",
        "sidebar.snippets" => "快捷命令",

        "sidebar.known_hosts" => "已知主机",
        "sidebar.history" => "历史记录",
        "sidebar.settings" => "设置",

        // 历史记录时间
        "history.just_now" => "刚刚",
        "history.minutes_ago" => "分钟前",
        "history.hours_ago" => "小时前",
        "history.days_ago" => "天前",

        // 服务器对话框
        "server_dialog.add_title" => "添加服务器",
        "server_dialog.edit_title" => "编辑服务器",
        "server_dialog.nav.basic_info" => "基本信息",
        "server_dialog.nav.jump_host" => "跳板机",
        "server_dialog.nav.proxy" => "代理设置",
        "server_dialog.nav.other" => "其他设置",
        "server_dialog.group" => "服务器分组",
        "server_dialog.group_placeholder" => "选择或输入分组",
        "server_dialog.label" => "服务器标签",
        "server_dialog.label_placeholder" => "请输入服务器名称",
        "server_dialog.host" => "主机地址",
        "server_dialog.host_placeholder" => "IP 或域名",
        "server_dialog.port" => "端口",
        "server_dialog.username" => "用户名",
        "server_dialog.password" => "密码",
        "server_dialog.auth_type" => "认证方式",
        "server_dialog.auth_password" => "密码认证",
        "server_dialog.auth_key" => "密钥认证",
        "server_dialog.private_key" => "私钥文件",
        "server_dialog.private_key_placeholder" => "点击浏览选择私钥文件...",
        "server_dialog.passphrase" => "私钥密码（可选）",
        "server_dialog.jump_host_address" => "跳板机地址",
        "server_dialog.jump_host_placeholder" => "输入跳板机地址 (Host:Port)",
        "server_dialog.enable_jump_host" => "启用跳板机",
        "server_dialog.enable_proxy" => "启用代理",
        "server_dialog.proxy_host" => "代理服务器地址",
        "server_dialog.proxy_port" => "端口",
        "server_dialog.proxy_username" => "代理用户名 (可选)",
        "server_dialog.proxy_password" => "代理密码 (可选)",
        "server_dialog.browse" => "浏览",
        "server_dialog.description" => "描述",
        "server_dialog.description_placeholder" => "输入服务器描述（可选）",
        "server_dialog.no_other_settings" => "暂无其他设置选项",

        // 服务器列表
        "server_list.add_server" => "添加服务器",
        "server_list.empty_title" => "暂无服务器",
        "server_list.empty_description" => "点击下方按钮添加您的第一台服务器",
        "server_list.header.server" => "服务器",
        "server_list.header.host" => "主机",
        "server_list.header.port" => "端口",
        "server_list.header.description" => "描述",
        "server_list.header.account" => "账号",
        "server_list.header.last_connected" => "最近连接",
        "server_list.header.actions" => "操作",
        "server_list.ungrouped" => "未分组",
        "server_list.never_connected" => "从未",
        "server_list.placeholder.snippets" => "代码片段功能",
        "server_list.placeholder.known_hosts" => "已知主机管理",
        "server_list.placeholder.history" => "连接历史记录",

        // 连接页面
        "connecting.title" => "正在连接",
        "connecting.error_title" => "连接失败",
        "connecting.cancel" => "取消连接",
        "connecting.step.initializing" => "初始化连接...",
        "connecting.step.authenticating" => "验证身份...",
        "connecting.step.establishing" => "建立安全通道...",
        "connecting.step.starting" => "启动会话...",
        "connecting.step.done" => "连接成功",

        // Host Key 验证
        "connecting.host_key.first_connection" => "🔐 首次连接此主机",
        "connecting.host_key.key_changed" => "⚠️ 主机密钥已变更",
        "connecting.host_key.fingerprint" => "密钥指纹：",
        "connecting.host_key.key_saved" => "✓ 密钥已自动保存",
        "connecting.host_key.btn_accept_save" => "信任并保存",
        "connecting.host_key.btn_accept_once" => "仅本次信任",
        "connecting.host_key.btn_reject" => "拒绝连接",
        "connecting.connected" => "连接成功",

        // 会话页面
        "session.connected" => "已连接到",
        "session.terminal_placeholder" => "终端功能正在开发中...",
        // 会话侧边栏
        "session.sidebar.quick_actions" => "快捷操作",
        "session.sidebar.new_terminal" => "新建终端",
        "session.sidebar.file_browser" => "文件浏览",
        "session.sidebar.system_info" => "系统信息",
        // Monitor 面板
        "session.monitor.title" => "系统监控",
        "session.monitor.cpu" => "CPU 使用率",
        "session.monitor.memory" => "内存使用",
        "session.monitor.disk" => "磁盘使用",
        "session.monitor.placeholder" => "监控数据加载中...",
        // Terminal 面板
        "session.terminal.title" => "终端",
        "session.terminal.placeholder" => "终端正在初始化...",
        // SFTP 面板
        "session.sftp.title" => "文件传输",
        "session.sftp.local" => "本地",
        "session.sftp.remote" => "远程",
        "session.sftp.placeholder" => "文件列表加载中...",

        // Snippets 快捷命令
        "snippets.add_group" => "新建组",
        "snippets.add_command" => "新建命令",
        "snippets.breadcrumb.all" => "全部",
        "snippets.empty.title" => "暂无快捷命令",
        "snippets.empty.description" => "创建命令组或命令开始使用",
        "snippets.dialog.group_name" => "组名称",
        "snippets.dialog.command_name" => "命令名称",
        "snippets.dialog.command_content" => "命令内容",
        "snippets.dialog.parent_group" => "父级分组",
        "snippets.dialog.description" => "描述",
        "snippets.dialog.edit_group" => "编辑分组",
        "snippets.dialog.edit_command" => "编辑命令",
        "snippets.dialog.enter_name" => "请输入名称...",
        "snippets.dialog.enter_command" => "请输入命令...",

        // 小侧栏
        "mini_sidebar.snippets" => "快捷命令",
        "mini_sidebar.transfer" => "传输管理",

        _ => key,
    }
}

fn en_us(key: &'static str) -> &'static str {
    match key {
        // Common
        "common.save" => "Save",
        "common.cancel" => "Cancel",
        "common.confirm" => "Confirm",
        "common.loading" => "Loading...",
        "common.edit" => "Edit",
        "common.delete" => "Delete",

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

        // Terminal Settings
        "settings.terminal.font" => "Font",
        "settings.terminal.font_family" => "Terminal Font",
        "settings.terminal.font_size" => "Font Size",
        "settings.terminal.line_height" => "Line Height",
        "settings.terminal.ligatures" => "Enable Ligatures",
        "settings.terminal.color_scheme" => "Color Scheme",
        "settings.terminal.theme" => "Terminal Theme",
        "settings.terminal.display" => "Display",
        "settings.terminal.cursor_blink" => "Cursor Blink",
        "settings.terminal.scrollback" => "Scrollback Lines",

        // Key Bindings
        "settings.keybindings.coming_soon" => "Key bindings editor coming in a future release",
        "settings.keybindings.description" => "Customize terminal and SFTP shortcuts",

        // SFTP Settings
        "settings.sftp.file_display" => "File Display",
        "settings.sftp.show_hidden" => "Show Hidden Files",
        "settings.sftp.folders_first" => "Folders First",
        "settings.sftp.transfer" => "Transfer Settings",
        "settings.sftp.concurrent" => "Concurrent Transfers",
        "settings.sftp.preserve_time" => "Preserve Timestamps",
        "settings.sftp.resume" => "Resume Transfers",
        "settings.sftp.editor" => "Editor",
        "settings.sftp.builtin_editor" => "Use Built-in Editor",
        "settings.sftp.syntax_highlight" => "Syntax Highlighting",

        // Monitor Settings
        "settings.monitor.data_collection" => "Data Collection",
        "settings.monitor.history_retention" => "History Retention (min)",
        "settings.monitor.auto_deploy" => "Auto Deploy Agent",
        "settings.monitor.display_items" => "Display Items",
        "settings.monitor.cpu" => "CPU",
        "settings.monitor.memory" => "Memory",
        "settings.monitor.disk" => "Disk",
        "settings.monitor.network" => "Network",
        "settings.monitor.alerts" => "Alert Thresholds",
        "settings.monitor.cpu_threshold" => "CPU (%)",
        "settings.monitor.memory_threshold" => "Memory (%)",
        "settings.monitor.disk_threshold" => "Disk (%)",

        // Connection Settings
        "settings.connection.ssh" => "SSH Settings",
        "settings.connection.default_port" => "Default Port",
        "settings.connection.timeout" => "Connection Timeout (s)",
        "settings.connection.keepalive" => "Keepalive Interval (s)",
        "settings.connection.compression" => "Enable Compression",
        "settings.connection.reconnect" => "Auto Reconnect",
        "settings.connection.reconnect_enabled" => "Auto Reconnect",
        "settings.connection.reconnect_attempts" => "Reconnect Attempts",
        "settings.connection.reconnect_interval" => "Reconnect Interval (s)",

        // Data Sync
        "settings.sync.status" => "Sync Status",
        "settings.sync.enabled" => "Enable Sync",
        "settings.sync.auto" => "Auto Sync",
        "settings.sync.content" => "Sync Content",
        "settings.sync.servers" => "Server Config",
        "settings.sync.groups" => "Group Info",
        "settings.sync.settings" => "App Settings",
        "settings.sync.keybindings" => "Key Bindings",
        "settings.sync.webdav" => "WebDAV Config",
        "settings.sync.webdav_url" => "Server URL",
        "settings.sync.webdav_username" => "Username",
        "settings.sync.webdav_password" => "Password",
        "settings.sync.webdav_path" => "Storage Path",
        "settings.sync.test_connection" => "Test Connection",
        "settings.sync.sync_now" => "Sync Now",

        // System Settings
        "settings.system.startup" => "Startup",
        "settings.system.auto_start" => "Launch at Login",
        "settings.system.start_minimized" => "Start Minimized",
        "settings.system.check_updates" => "Check Updates",
        "settings.system.window" => "Window",
        "settings.system.close_to_tray" => "Close to Tray",
        "settings.system.show_tray_icon" => "Show Tray Icon",
        "settings.system.single_instance" => "Single Instance",
        "settings.system.notification" => "Notification",
        "settings.system.notify_disconnect" => "Disconnect Notification",
        "settings.system.notify_transfer" => "Transfer Complete Notification",
        "settings.system.logging" => "Logging",
        "settings.system.logging_enabled" => "Enable Logging",
        "settings.system.log_retention" => "Log Retention (days)",

        // About
        "settings.about.platform" => "Platform",
        "settings.about.arch" => "Architecture",
        "settings.about.copyright" => "© 2024 ShellMaster. All rights reserved.",

        // Sidebar
        "sidebar.hosts" => "Hosts",
        "sidebar.monitor" => "Monitor",
        "sidebar.snippets" => "Snippets",

        "sidebar.known_hosts" => "Known Hosts",
        "sidebar.history" => "History",
        "sidebar.settings" => "Settings",

        // History Time
        "history.just_now" => "Just now",
        "history.minutes_ago" => "m ago",
        "history.hours_ago" => "h ago",
        "history.days_ago" => "d ago",

        // Server Dialog
        "server_dialog.add_title" => "Add Server",
        "server_dialog.edit_title" => "Edit Server",
        "server_dialog.nav.basic_info" => "Basic Info",
        "server_dialog.nav.jump_host" => "Jump Host",
        "server_dialog.nav.proxy" => "Proxy Settings",
        "server_dialog.nav.other" => "Other Settings",
        "server_dialog.group" => "Server Group",
        "server_dialog.group_placeholder" => "Select or enter group",
        "server_dialog.label" => "Server Label",
        "server_dialog.label_placeholder" => "Enter server name",
        "server_dialog.host" => "Host Address",
        "server_dialog.host_placeholder" => "IP or Domain",
        "server_dialog.port" => "Port",
        "server_dialog.username" => "Username",
        "server_dialog.password" => "Password",
        "server_dialog.auth_type" => "Auth Type",
        "server_dialog.auth_password" => "Password",
        "server_dialog.auth_key" => "Key",
        "server_dialog.private_key" => "Private Key",
        "server_dialog.private_key_placeholder" => "Click to browse for private key...",
        "server_dialog.passphrase" => "Passphrase (optional)",
        "server_dialog.jump_host_address" => "Jump Host Address",
        "server_dialog.jump_host_placeholder" => "Enter jump host (Host:Port)",
        "server_dialog.enable_jump_host" => "Enable Jump Host",
        "server_dialog.enable_proxy" => "Enable Proxy",
        "server_dialog.proxy_host" => "Proxy Host",
        "server_dialog.proxy_port" => "Port",
        "server_dialog.proxy_username" => "Proxy Username (optional)",
        "server_dialog.proxy_password" => "Proxy Password (optional)",
        "server_dialog.browse" => "Browse",
        "server_dialog.description" => "Description",
        "server_dialog.description_placeholder" => "Enter server description (optional)",
        "server_dialog.no_other_settings" => "No other settings available",

        // Server List
        "server_list.add_server" => "Add Server",
        "server_list.empty_title" => "No Servers",
        "server_list.empty_description" => "Click the button below to add your first server",
        "server_list.header.server" => "Server",
        "server_list.header.host" => "Host",
        "server_list.header.port" => "Port",
        "server_list.header.description" => "Description",
        "server_list.header.account" => "Account",
        "server_list.header.last_connected" => "Last Connected",
        "server_list.header.actions" => "Actions",
        "server_list.ungrouped" => "Ungrouped",
        "server_list.never_connected" => "Never",
        "server_list.placeholder.snippets" => "Snippets Feature",
        "server_list.placeholder.known_hosts" => "Known Hosts Management",
        "server_list.placeholder.history" => "Connection History",

        // Connecting Page
        "connecting.title" => "Connecting",
        "connecting.error_title" => "Connection Failed",
        "connecting.cancel" => "Cancel",
        "connecting.step.initializing" => "Initializing connection...",
        "connecting.step.authenticating" => "Authenticating...",
        "connecting.step.establishing" => "Establishing secure channel...",
        "connecting.step.starting" => "Starting session...",
        "connecting.step.done" => "Connected",

        // Host Key Verification
        "connecting.host_key.first_connection" => "🔐 First Connection",
        "connecting.host_key.key_changed" => "⚠️ Host Key Changed",
        "connecting.host_key.fingerprint" => "Fingerprint:",
        "connecting.host_key.key_saved" => "✓ Key saved automatically",
        "connecting.host_key.btn_accept_save" => "Trust & Save",
        "connecting.host_key.btn_accept_once" => "Trust Once",
        "connecting.host_key.btn_reject" => "Reject",
        "connecting.connected" => "Connected",

        // Session Page
        "session.connected" => "Connected to",
        "session.terminal_placeholder" => "Terminal feature coming soon...",
        // Session Sidebar
        "session.sidebar.quick_actions" => "Quick Actions",
        "session.sidebar.new_terminal" => "New Terminal",
        "session.sidebar.file_browser" => "File Browser",
        "session.sidebar.system_info" => "System Info",
        // Monitor Panel
        "session.monitor.title" => "System Monitor",
        "session.monitor.cpu" => "CPU Usage",
        "session.monitor.memory" => "Memory Usage",
        "session.monitor.disk" => "Disk Usage",
        "session.monitor.placeholder" => "Loading monitoring data...",
        // Terminal Panel
        "session.terminal.title" => "Terminal",
        "session.terminal.placeholder" => "Initializing terminal...",
        // SFTP Panel
        "session.sftp.title" => "File Transfer",
        "session.sftp.local" => "Local",
        "session.sftp.remote" => "Remote",
        "session.sftp.placeholder" => "Loading file list...",

        // Snippets
        "snippets.add_group" => "New Group",
        "snippets.add_command" => "New Command",
        "snippets.breadcrumb.all" => "All",
        "snippets.empty.title" => "No Snippets",
        "snippets.empty.description" => "Create a group or command to get started",
        "snippets.dialog.group_name" => "Group Name",
        "snippets.dialog.command_name" => "Command Name",
        "snippets.dialog.command_content" => "Command Content",
        "snippets.dialog.parent_group" => "Parent Group",
        "snippets.dialog.description" => "Description",
        "snippets.dialog.edit_group" => "Edit Group",
        "snippets.dialog.edit_command" => "Edit Command",
        "snippets.dialog.enter_name" => "Enter name...",
        "snippets.dialog.enter_command" => "Enter command...",

        // Mini Sidebar
        "mini_sidebar.snippets" => "Snippets",
        "mini_sidebar.transfer" => "Transfer",

        _ => key,
    }
}
