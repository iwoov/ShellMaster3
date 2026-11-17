use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use once_cell::sync::Lazy;
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use tokio::task::AbortHandle;
use tracing::{error, info};

use super::client::SshClient;
use super::config::SshConfig;
use super::event::ConnectionEvent;
use super::session::SshSession;

/// 连接句柄：接收连接过程中的事件（阶段、日志、主机密钥确认、结果）
pub struct ConnectionHandle {
    /// 事件接收器；连接任务被取消时会直接关闭
    pub event_rx: mpsc::UnboundedReceiver<ConnectionEvent>,
}

/// 进行中的连接任务
struct PendingConnect {
    /// 用于识别同一会话 ID 下被替换掉的旧任务
    generation: u64,
    task: AbortHandle,
}

/// 全局 SSH 管理器
/// 负责管理 Tokio 运行时和所有 SSH 会话
pub struct SshManager {
    /// Tokio 运行时，用于执行所有 SSH 异步任务
    runtime: Runtime,
    /// 活跃会话映射表 (Tab ID -> Session)
    sessions: RwLock<HashMap<String, Arc<SshSession>>>,
    /// 进行中的连接任务 (Tab ID -> 任务)。锁顺序：先 pending 再 sessions。
    pending: Mutex<HashMap<String, PendingConnect>>,
    next_generation: AtomicU64,
}

impl SshManager {
    /// 创建新的 SSH 管理器
    fn new() -> Self {
        // 创建多线程 Tokio 运行时
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("ssh-worker")
            .build()
            .expect("Failed to create SSH Tokio runtime");

        Self {
            runtime,
            sessions: RwLock::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            next_generation: AtomicU64::new(0),
        }
    }

    /// 获取全局单例
    pub fn global() -> &'static SshManager {
        static MANAGER: Lazy<SshManager> = Lazy::new(SshManager::new);
        &MANAGER
    }

    /// 获取 Tokio 运行时引用
    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// 获取会话
    pub fn get_session(&self, id: &str) -> Option<Arc<SshSession>> {
        self.sessions
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(id)
            .cloned()
    }

    /// 关闭会话并清理资源；连接仍在进行中时直接取消
    pub fn close_session(&self, id: &str) {
        let session = {
            let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(pending_connect) = pending.remove(id) {
                info!("[SSH Manager] Cancelling in-progress connection {}", id);
                pending_connect.task.abort();
            }
            // 持有 pending 锁时移除，避免与刚完成的连接注册交错
            self.sessions
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(id)
        };

        if let Some(session) = session {
            self.spawn_close(session);
        }
    }

    fn spawn_close(&self, session: Arc<SshSession>) {
        self.runtime.spawn(async move {
            info!("[SSH Manager] Closing session {}", session.id());
            if let Err(e) = session.close().await {
                error!(
                    "[SSH Manager] Failed to close session {}: {}",
                    session.id(),
                    e
                );
            } else {
                info!(
                    "[SSH Manager] Session {} closed and resources cleaned up",
                    session.id()
                );
            }
        });
    }

    /// 启动连接任务
    ///
    /// 成功时先把会话注册到管理器，再发送 `Connected` 事件；
    /// 同一 ID 发起新连接或调用 `close_session` 都会取消仍在进行中的旧任务。
    pub fn connect(&self, config: SshConfig, session_id: String) -> ConnectionHandle {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);

        // 持有 pending 锁完成 spawn 与登记，确保任务结束时一定能找到自己的登记项
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        let id = session_id.clone();
        let task = self.runtime.spawn(async move {
            let client = SshClient::new(config, event_tx.clone());
            let result = client.connect(id.clone()).await;
            let manager = SshManager::global();
            match result {
                Ok(session) => {
                    if manager.complete_connect(&id, generation, session) {
                        let _ = event_tx.send(ConnectionEvent::Connected { session_id: id });
                    }
                }
                Err(e) => {
                    manager.forget_pending(&id, generation);
                    let _ = event_tx.send(ConnectionEvent::Failed {
                        error: e.to_string(),
                        retryable: e.is_retryable(),
                    });
                }
            }
        });

        if let Some(previous) = pending.insert(
            session_id,
            PendingConnect {
                generation,
                task: task.abort_handle(),
            },
        ) {
            previous.task.abort();
        }

        ConnectionHandle { event_rx }
    }

    /// 连接成功后调用：连接仍然有效（没有被取消或替换）时注册会话并返回 true；
    /// 否则关闭这个已经没人要的连接。
    fn complete_connect(&self, id: &str, generation: u64, session: SshSession) -> bool {
        let previous = {
            let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
            if !pending
                .get(id)
                .is_some_and(|pending_connect| pending_connect.generation == generation)
            {
                drop(pending);
                info!(
                    "[SSH Manager] Connection {} finished after being cancelled; closing it",
                    id
                );
                self.spawn_close(Arc::new(session));
                return false;
            }
            pending.remove(id);
            self.sessions
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(id.to_string(), Arc::new(session))
        };

        // 重连成功会替换旧会话，旧连接一并关闭（含其跳板机连接）
        if let Some(previous) = previous {
            self.spawn_close(previous);
        }
        true
    }

    fn forget_pending(&self, id: &str, generation: u64) {
        let mut pending = self.pending.lock().unwrap_or_else(PoisonError::into_inner);
        if pending
            .get(id)
            .is_some_and(|pending_connect| pending_connect.generation == generation)
        {
            pending.remove(id);
        }
    }
}
