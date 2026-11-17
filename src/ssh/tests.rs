// SSH 连接核心的集成测试：在进程内启动 russh 服务器，走真实的握手、认证和通道流程。
//
// 主机密钥通过 `HostKeyPolicy::trusted` 预先信任，测试不会弹出确认、也不会写入 known hosts。

use std::borrow::Cow;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use russh::keys::ssh_key::HashAlg;
use russh::keys::PrivateKey;
use russh::server::{self, Auth, Msg, Session};
use russh::{Channel, ChannelId, CryptoVec, MethodKind, MethodSet};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use super::client::SshClient;
use super::config::{host_key_id, AuthMethod, HostKeyPolicy, KeepaliveConfig, SshConfig};
use super::error::SshError;
use super::event::{ConnectionEvent, HostKeyAction};
use super::manager::SshManager;
use super::session::PtyRequest;

/// 仅用于测试的服务器主机密钥（ed25519）
const TEST_HOST_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACD1ec8pTddjrnccHg2gP1M1WTsqa+x/aYEXjoU+jtdsigAAAJjDr3rBw696
wQAAAAtzc2gtZWQyNTUxOQAAACD1ec8pTddjrnccHg2gP1M1WTsqa+x/aYEXjoU+jtdsig
AAAEAi1Gje+OknNw0gMVnWeWl6lcEOqtaE/zxijcoqmlW9e/V5zylN12OudxweDaA/UzVZ
Oypr7H9pgReOhT6O12yKAAAAFXNoZWxsbWFzdGVyLXRlc3QtaG9zdA==
-----END OPENSSH PRIVATE KEY-----
";

const PASSWORD: &str = "correct horse";

/// 终端测试中 shell 一次性输出的数据量
const SHELL_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy)]
enum PasswordMode {
    /// 接受 password 方式
    Password,
    /// 只开放 keyboard-interactive（如 FreeBSD 默认配置）
    KeyboardInteractiveOnly,
}

#[derive(Clone)]
struct TestHandler {
    mode: PasswordMode,
}

impl server::Handler for TestHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        Ok(match self.mode {
            PasswordMode::Password if password == PASSWORD => Auth::Accept,
            PasswordMode::Password => Auth::reject(),
            PasswordMode::KeyboardInteractiveOnly => {
                let mut methods = MethodSet::empty();
                methods.push(MethodKind::KeyboardInteractive);
                Auth::Reject {
                    proceed_with_methods: Some(methods),
                    partial_success: false,
                }
            }
        })
    }

    async fn auth_keyboard_interactive<'a>(
        &'a mut self,
        _user: &str,
        _submethods: &str,
        response: Option<server::Response<'a>>,
    ) -> Result<Auth, Self::Error> {
        Ok(match response {
            None => Auth::Partial {
                name: Cow::Borrowed(""),
                instructions: Cow::Borrowed(""),
                prompts: Cow::Owned(vec![(Cow::Borrowed("Password: "), false)]),
            },
            Some(mut response) => {
                if response.next().as_deref() == Some(PASSWORD.as_bytes()) {
                    Auth::Accept
                } else {
                    Auth::reject()
                }
            }
        })
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    /// 模拟 OpenSSH 的消息顺序：输出 -> EOF -> exit-status -> 关闭
    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let exit_status = if data == b"fail" { 3 } else { 0 };
        session.channel_success(channel)?;
        session.data(channel, CryptoVec::from_slice(b"hello\n"))?;
        session.eof(channel)?;
        session.exit_status_request(channel, exit_status)?;
        session.close(channel)?;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    /// shell 一次性输出大量数据后退出
    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        for chunk in vec![b'x'; SHELL_OUTPUT_BYTES].chunks(8 * 1024) {
            session.data(channel, CryptoVec::from_slice(chunk))?;
        }
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }

    /// 作为跳板机：把通道转发到目标地址
    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        let target = format!("{}:{}", host_to_connect, port_to_connect);
        tokio::spawn(async move {
            if let Ok(mut tcp) = tokio::net::TcpStream::connect(target).await {
                let mut stream = channel.into_stream();
                let _ = tokio::io::copy_bidirectional(&mut stream, &mut tcp).await;
            }
        });
        Ok(true)
    }
}

struct TestServer {
    addr: SocketAddr,
    fingerprint: String,
}

impl TestServer {
    fn trusted_entry(&self) -> (String, String) {
        (
            host_key_id("127.0.0.1", self.addr.port()),
            self.fingerprint.clone(),
        )
    }
}

async fn start_server(mode: PasswordMode) -> TestServer {
    let host_key = PrivateKey::from_openssh(TEST_HOST_KEY).expect("valid test host key");
    let fingerprint = host_key.public_key().fingerprint(HashAlg::Sha256).to_string();

    let mut methods = MethodSet::all();
    match mode {
        PasswordMode::Password => methods.remove(MethodKind::KeyboardInteractive),
        PasswordMode::KeyboardInteractiveOnly => methods.remove(MethodKind::Password),
    }
    let config = Arc::new(server::Config {
        keys: vec![host_key],
        methods,
        auth_rejection_time: Duration::from_millis(10),
        auth_rejection_time_initial: Some(Duration::ZERO),
        ..Default::default()
    });

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let config = config.clone();
            tokio::spawn(async move {
                if let Ok(session) = server::run_stream(config, socket, TestHandler { mode }).await
                {
                    let _ = session.await;
                }
            });
        }
    });

    TestServer { addr, fingerprint }
}

fn client_config(server: &TestServer, password: &str, policy: HostKeyPolicy) -> SshConfig {
    SshConfig {
        label: format!("test-{}", server.addr.port()),
        host: "127.0.0.1".to_string(),
        port: server.addr.port(),
        username: "tester".to_string(),
        auth: AuthMethod::Password(password.to_string()),
        connect_timeout: 5,
        jump_host: None,
        proxy: None,
        keepalive: KeepaliveConfig {
            enabled: false,
            interval: 0,
            max_retries: 3,
        },
        compression: false,
        host_key_policy: policy,
    }
}

fn trusting(servers: &[&TestServer]) -> HostKeyPolicy {
    HostKeyPolicy {
        interactive: false,
        strict: false,
        trusted: servers.iter().map(|server| server.trusted_entry()).collect(),
    }
}

async fn connect(config: SshConfig) -> Result<super::session::SshSession, SshError> {
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    SshClient::new(config, event_tx)
        .connect("test-session".to_string())
        .await
}

#[tokio::test]
async fn password_login_and_exec_exit_codes() {
    let server = start_server(PasswordMode::Password).await;
    let session = connect(client_config(&server, PASSWORD, trusting(&[&server])))
        .await
        .expect("password login");

    let ok = session.open_exec().await.unwrap().exec("true").await.unwrap();
    assert_eq!(ok.stdout, b"hello\n");
    // exit-status 在 EOF 之后才到，也要拿到
    assert_eq!(ok.exit_code, Some(0));
    assert!(ok.is_success());

    let failed = session.open_exec().await.unwrap().exec("fail").await.unwrap();
    assert_eq!(failed.exit_code, Some(3));
    assert!(!failed.is_success());

    let key_id = server.trusted_entry().0;
    assert_eq!(
        session.verified_host_keys().get(&key_id),
        Some(&server.fingerprint)
    );
    session.close().await.unwrap();
}

#[tokio::test]
async fn password_falls_back_to_keyboard_interactive() {
    let server = start_server(PasswordMode::KeyboardInteractiveOnly).await;
    let session = connect(client_config(&server, PASSWORD, trusting(&[&server])))
        .await
        .expect("keyboard-interactive login with the saved password");
    assert!(session.is_alive());

    let error = connect(client_config(&server, "wrong", trusting(&[&server])))
        .await
        .err()
        .expect("wrong password is rejected");
    assert!(matches!(error, SshError::Auth(_)), "{error}");
}

#[tokio::test]
async fn wrong_password_is_not_retryable() {
    let server = start_server(PasswordMode::Password).await;
    let error = connect(client_config(&server, "wrong", trusting(&[&server])))
        .await
        .err()
        .expect("wrong password is rejected");
    assert!(matches!(error, SshError::Auth(_)), "{error}");
    assert!(!error.is_retryable());
}

#[tokio::test]
async fn unconfirmed_host_key_is_rejected_when_not_interactive() {
    let server = start_server(PasswordMode::Password).await;
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let error = SshClient::new(
        client_config(&server, PASSWORD, HostKeyPolicy::default()),
        event_tx,
    )
    .connect("test-session".to_string())
    .await
    .err()
    .expect("unknown key must be rejected");

    assert!(matches!(error, SshError::HostKey(_)), "{error}");
    assert!(!error.is_retryable());
    while let Ok(event) = event_rx.try_recv() {
        assert!(
            !matches!(
                event,
                ConnectionEvent::HostKeyVerification { .. } | ConnectionEvent::HostKeyMismatch { .. }
            ),
            "non-interactive policy must not prompt"
        );
    }
}

#[tokio::test]
async fn trusted_key_must_match_exactly() {
    let server = start_server(PasswordMode::Password).await;
    let mut policy = trusting(&[&server]);
    let key_id = server.trusted_entry().0;
    policy
        .trusted
        .insert(key_id, "SHA256:someOtherKeyThatWasTrustedBefore".to_string());
    let error = connect(client_config(&server, PASSWORD, policy))
        .await
        .err()
        .expect("a different key must be rejected");
    assert!(matches!(error, SshError::HostKey(_)), "{error}");
}

#[tokio::test]
async fn connects_through_jump_host() {
    let jump = start_server(PasswordMode::Password).await;
    let target = start_server(PasswordMode::KeyboardInteractiveOnly).await;
    let policy = trusting(&[&jump, &target]);

    let mut config = client_config(&target, PASSWORD, policy.clone());
    config.jump_host = Some(Box::new(client_config(&jump, PASSWORD, policy)));

    let session = connect(config).await.expect("login through the jump host");
    let output = session.open_exec().await.unwrap().exec("true").await.unwrap();
    assert_eq!(output.stdout, b"hello\n");
    assert_eq!(output.exit_code, Some(0));
    // 两跳的主机密钥都记录下来，供重连免确认
    assert_eq!(session.verified_host_keys().len(), 2);
    session.close().await.unwrap();
}

#[tokio::test]
async fn jump_host_failure_names_the_jump_host() {
    let jump = start_server(PasswordMode::Password).await;
    let target = start_server(PasswordMode::Password).await;
    let policy = trusting(&[&jump, &target]);

    let mut config = client_config(&target, PASSWORD, policy.clone());
    let mut jump_config = client_config(&jump, "wrong", policy);
    jump_config.label = "bastion".to_string();
    config.jump_host = Some(Box::new(jump_config));

    let error = connect(config).await.err().expect("jump host login fails");
    assert!(error.to_string().contains("bastion"), "{error}");
    assert!(!error.is_retryable());
}

#[tokio::test]
async fn terminal_output_is_read_completely() {
    let server = start_server(PasswordMode::Password).await;
    let session = connect(client_config(&server, PASSWORD, trusting(&[&server])))
        .await
        .unwrap();
    let terminal = session.open_terminal(PtyRequest::default()).await.unwrap();

    let mut total = 0;
    let mut reads = 0;
    while let Some(chunk) = tokio::time::timeout(Duration::from_secs(10), terminal.read())
        .await
        .expect("terminal read does not hang")
        .unwrap()
    {
        assert!(!chunk.is_empty(), "read never returns empty chunks");
        total += chunk.len();
        reads += 1;
    }
    assert_eq!(total, SHELL_OUTPUT_BYTES);
    assert!(reads >= 1);
    // 通道结束后继续读取仍返回 None
    assert!(terminal.read().await.unwrap().is_none());
}

#[tokio::test]
async fn closing_a_tab_cancels_the_pending_connection() {
    // 接受 TCP 连接但从不发送 SSH 版本号的“服务器”，连接会一直停在握手阶段
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let mut sockets = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            sockets.push(socket);
        }
    });
    let silent = TestServer {
        addr,
        fingerprint: String::new(),
    };
    let mut config = client_config(&silent, PASSWORD, HostKeyPolicy::default());
    config.connect_timeout = 30;

    let manager = SshManager::global();
    let id = format!("test-cancel-{}", uuid::Uuid::new_v4());
    let mut events = manager.connect(config, id.clone()).event_rx;
    tokio::time::sleep(Duration::from_millis(300)).await;
    manager.close_session(&id);

    let outcome = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(event) = events.recv().await {
            if matches!(
                event,
                ConnectionEvent::Connected { .. } | ConnectionEvent::Failed { .. }
            ) {
                return Some(event);
            }
        }
        None
    })
    .await
    .expect("event channel closes after cancellation");
    assert!(outcome.is_none(), "cancelled connection reported {outcome:?}");
    assert!(manager.get_session(&id).is_none());
}

#[tokio::test]
async fn closing_a_tab_while_confirming_host_key_cancels_the_wait() {
    let server = start_server(PasswordMode::Password).await;
    let policy = HostKeyPolicy {
        interactive: true,
        ..HostKeyPolicy::default()
    };
    let manager = SshManager::global();
    let id = format!("test-host-key-{}", uuid::Uuid::new_v4());
    let mut events = manager
        .connect(client_config(&server, PASSWORD, policy), id.clone())
        .event_rx;

    // 等到弹出主机密钥确认（不作回应，模拟用户还在核对），然后关闭标签页
    let responder = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match events.recv().await {
                Some(ConnectionEvent::HostKeyVerification { responder, .. }) => return responder,
                Some(_) => {}
                None => panic!("connection ended before asking for confirmation"),
            }
        }
    })
    .await
    .expect("host key confirmation is requested");
    manager.close_session(&id);

    let outcome = tokio::time::timeout(Duration::from_secs(3), async {
        while let Some(event) = events.recv().await {
            if matches!(event, ConnectionEvent::Connected { .. }) {
                return Some(event);
            }
        }
        None
    })
    .await
    .expect("event channel closes after cancellation");
    assert!(outcome.is_none());
    // 取消之后才回应，不会再建立连接
    responder.respond(HostKeyAction::AcceptOnce);
    assert!(manager.get_session(&id).is_none());
}

#[tokio::test]
async fn accepted_host_key_registers_session_before_connected_event() {
    let server = start_server(PasswordMode::Password).await;
    let policy = HostKeyPolicy {
        interactive: true,
        ..HostKeyPolicy::default()
    };
    let manager = SshManager::global();
    let id = format!("test-accept-{}", uuid::Uuid::new_v4());
    let mut events = manager
        .connect(client_config(&server, PASSWORD, policy), id.clone())
        .event_rx;

    let connected = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(event) = events.recv().await {
            match event {
                ConnectionEvent::HostKeyVerification { responder, .. } => {
                    responder.respond(HostKeyAction::AcceptOnce)
                }
                ConnectionEvent::Connected { session_id } => return Some(session_id),
                ConnectionEvent::Failed { error, .. } => panic!("connection failed: {error}"),
                _ => {}
            }
        }
        None
    })
    .await
    .expect("connects in time");
    assert_eq!(connected.as_deref(), Some(id.as_str()));

    // 收到 Connected 时会话已经可用，且记下了本次信任的密钥
    let session = manager.get_session(&id).expect("session registered");
    assert!(session.is_alive());
    assert_eq!(
        session.verified_host_keys().get(&server.trusted_entry().0),
        Some(&server.fingerprint)
    );
    manager.close_session(&id);
}
