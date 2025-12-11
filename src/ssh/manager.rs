use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use once_cell::sync::Lazy;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

use super::client::SshClient;
use super::config::SshConfig;
use super::error::SshError;
use super::event::ConnectionEvent;
use super::session::SshSession;

/// 全局 SSH 管理器
/// 负责管理 Tokio 运行时和所有 SSH 会话
pub struct SshManager {
    /// Tokio 运行时，用于执行所有 SSH 异步任务
    runtime: Runtime,
    /// 活跃会话映射表 (Server ID -> Session)
    sessions: Arc<RwLock<HashMap<String, Arc<SshSession>>>>,
}

impl SshManager {
    /// 创建新的 SSH 管理器
    fn new() -> Self {
        // 创建多线程 Tokio 运行时
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            // .worker_threads(4) // 可选：限制线程数
            .thread_name("ssh-worker")
            .build()
            .expect("Failed to create SSH Tokio runtime");

        Self {
            runtime,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 获取全局单例
    pub fn global() -> &'static SshManager {
        static MANAGER: Lazy<SshManager> = Lazy::new(|| SshManager::new());
        &MANAGER
    }

    /// 获取 Tokio 运行时引用
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// 注册会话
    pub fn register_session(&self, session: SshSession) -> Arc<SshSession> {
        let id = session.id().to_string();
        let session = Arc::new(session);
        self.sessions.write().unwrap().insert(id, session.clone());
        session
    }

    /// 获取会话
    pub fn get_session(&self, id: &str) -> Option<Arc<SshSession>> {
        self.sessions.read().unwrap().get(id).cloned()
    }

    /// 移除会话
    pub fn remove_session(&self, id: &str) -> Option<Arc<SshSession>> {
        self.sessions.write().unwrap().remove(id)
    }

    /// 关闭会话并清理资源
    pub fn close_session(&self, id: &str) {
        if let Some(session) = self.remove_session(id) {
            let _ = self.runtime.spawn(async move {
                println!("[SSH Manager] Closing session {}", session.id());
                if let Err(e) = session.close().await {
                    eprintln!(
                        "[SSH Manager] Failed to close session {}: {}",
                        session.id(),
                        e
                    );
                } else {
                    println!(
                        "[SSH Manager] Session {} closed and resources cleaned up",
                        session.id()
                    );
                }
            });
        }
    }

    /// 启动连接任务
    /// 返回事件接收器
    pub fn connect(
        &self,
        config: SshConfig,
        session_id: String,
    ) -> mpsc::UnboundedReceiver<ConnectionEvent> {
        let (tx, rx) = mpsc::unbounded_channel();
        let manager_config = config.clone();

        // 在全局运行时中启动连接任务
        self.runtime.spawn(async move {
            let client = SshClient::new(manager_config, tx.clone());
            let result = client.connect(session_id).await;

            match result {
                Ok(session) => {
                    // 连接成功，注册到管理器
                    SshManager::global().register_session(session);
                }
                Err(e) => {
                    let _ = tx.send(ConnectionEvent::Failed {
                        error: e.to_string(),
                    });
                }
            }
        });

        rx
    }
}
