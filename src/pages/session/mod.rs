// 会话页面模块

pub mod activity_rail;
pub mod agent_panel;
pub mod columns;
pub mod monitor_panel;
pub mod session_layout;
pub mod session_sidebar;
pub mod sftp_panel;
pub mod terminal_page;

pub use columns::{fit_columns, SessionColumns};
pub use session_layout::render_session_layout;
