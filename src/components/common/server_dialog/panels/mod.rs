// 面板模块导出

pub mod basic_info;
pub mod jump_host;
pub mod proxy_settings;

pub use basic_info::render_basic_info_form;
pub use jump_host::render_jump_host_form;
pub use proxy_settings::render_proxy_settings_form;
