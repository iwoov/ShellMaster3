// 面板模块导出

pub mod about;
pub mod connection;
pub mod keybindings;
pub mod monitor;
pub mod sftp;
pub mod sync;
pub mod system;
pub mod terminal;
pub mod theme;

pub use about::render_about_panel;
pub use connection::render_connection_panel;
pub use keybindings::render_keybindings_panel;
pub use monitor::render_monitor_panel;
pub use sftp::render_sftp_panel;
pub use sync::render_sync_panel;
pub use system::render_system_panel;
pub use terminal::render_terminal_panel;
pub use theme::render_theme_panel;
