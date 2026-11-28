// 系统监控组件模块

pub mod detail_dialog;
pub mod disk_card;
pub mod load_card;
pub mod network_card;
pub mod system_card;
pub mod view;

pub use detail_dialog::{render_detail_dialog, DetailDialogState};
pub use view::render_monitor_view;
