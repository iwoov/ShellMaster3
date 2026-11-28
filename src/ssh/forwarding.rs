//! SSH 端口转发引擎：支持本地转发（-L）、远程转发（-R）与动态 SOCKS5 转发（-D）。

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, Mutex};

use russh::client::Handle;
use russh::Channel;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use super::handler::SshClientHandler;

/// 端口转发类型
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ForwardingKind {
    /// 本地端口转发（-L）：本地监听 -> SSH 隧道 -> 远端目标
    #[default]
    Local,
    /// 远程端口转发（-R）：远端监听 -> SSH 隧道 -> 本地目标
    Remote,
    /// 动态 SOCKS5 代理（-D）：本地 SOCKS5 监听 -> SSH 隧道 -> 任意目标
    Dynamic,
}

impl ForwardingKind {
    pub fn short_tag(self) -> &'static str {
        match self {
            ForwardingKind::Local => "L",
            ForwardingKind::Remote => "R",
            ForwardingKind::Dynamic => "D",
        }
    }
}

/// 一条端口转发规则
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortForwardRule {
    pub id: String,
    pub kind: ForwardingKind,
    pub bind_host: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
}

impl PortForwardRule {
    /// 用于界面展示的简洁描述
    pub fn display_route(&self) -> String {
        match self.kind {
            ForwardingKind::Dynamic => format!("SOCKS5 {}:{}", self.bind_host, self.bind_port),
            ForwardingKind::Local | ForwardingKind::Remote => format!(
                "{}:{} → {}:{}",
                self.bind_host, self.bind_port, self.target_host, self.target_port
            ),
        }
    }

    /// 供一键复制的监听地址
    pub fn listen_endpoint(&self) -> String {
        format!("{}:{}", self.bind_host, self.bind_port)
    }
}

/// 远程端口转发注册表：`(remote_bind_host, remote_bind_port) -> (local_target_host, local_target_port)`
#[derive(Clone, Default)]
pub struct RemoteForwardRegistry {
    inner: Arc<Mutex<HashMap<(String, u16), (String, u16)>>>,
}

impl RemoteForwardRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, bind_host: String, bind_port: u16, target_host: String, target_port: u16) {
        if let Ok(mut map) = self.inner.lock() {
            map.insert((bind_host, bind_port), (target_host, target_port));
        }
    }

    pub fn remove(&self, bind_host: &str, bind_port: u16) {
        if let Ok(mut map) = self.inner.lock() {
            map.remove(&(bind_host.to_string(), bind_port));
        }
    }

    /// 按服务端上报的 `(connected_address, connected_port)` 查找本地目标；
    /// OpenSSH 服务端有时上报 `0.0.0.0` / `localhost` / 空串，精确匹配不到时按端口回退匹配。
    pub fn resolve(&self, connected_address: &str, connected_port: u16) -> Option<(String, u16)> {
        let map = self.inner.lock().ok()?;
        if let Some(target) = map.get(&(connected_address.to_string(), connected_port)) {
            return Some(target.clone());
        }
        map.iter()
            .find(|((_, port), _)| *port == connected_port)
            .map(|(_, target)| target.clone())
    }
}

/// 运行中的端口转发句柄，Drop 或 `stop()` 时停止监听并撤销远端转发
pub struct ActivePortForward {
    pub rule: PortForwardRule,
    cancel: CancellationToken,
    handle: Arc<RwLock<Handle<SshClientHandler>>>,
    remote_registry: RemoteForwardRegistry,
}

impl ActivePortForward {
    /// 停止转发：本地/动态转发关闭监听循环，远程转发注销映射并通知服务端 cancel_tcpip_forward
    pub async fn stop(self) {
        self.cancel.cancel();
        if self.rule.kind == ForwardingKind::Remote {
            self.remote_registry
                .remove(&self.rule.bind_host, self.rule.bind_port);
            let _ = self
                .handle
                .read()
                .await
                .cancel_tcpip_forward(self.rule.bind_host.clone(), self.rule.bind_port as u32)
                .await;
        }
        info!(
            "[Forward] Stopped {} forward: {}",
            self.rule.kind.short_tag(),
            self.rule.display_route()
        );
    }

    /// 同步触发停止信号（关闭会话标签或销毁时调用）
    pub fn cancel_now(&self) {
        self.cancel.cancel();
        if self.rule.kind == ForwardingKind::Remote {
            self.remote_registry
                .remove(&self.rule.bind_host, self.rule.bind_port);
        }
    }
}

/// 启动一条端口转发规则（-L / -R / -D）
pub async fn start_port_forward(
    mut rule: PortForwardRule,
    handle: Arc<RwLock<Handle<SshClientHandler>>>,
    remote_registry: RemoteForwardRegistry,
) -> Result<ActivePortForward, String> {
    let cancel = CancellationToken::new();

    match rule.kind {
        ForwardingKind::Local => {
            let listener = TcpListener::bind((rule.bind_host.as_str(), rule.bind_port))
                .await
                .map_err(|e| {
                    format!(
                        "Failed to bind local {}:{}: {}",
                        rule.bind_host, rule.bind_port, e
                    )
                })?;
            if let Ok(addr) = listener.local_addr() {
                rule.bind_port = addr.port();
            }

            let target_host = rule.target_host.clone();
            let target_port = rule.target_port;
            let handle_clone = handle.clone();
            let cancel_clone = cancel.clone();

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel_clone.cancelled() => break,
                        accepted = listener.accept() => {
                            let (mut tcp, peer) = match accepted {
                                Ok(pair) => pair,
                                Err(e) => {
                                    warn!("[Forward -L] Accept error: {}", e);
                                    break;
                                }
                            };
                            let handle_inner = handle_clone.clone();
                            let th = target_host.clone();
                            tokio::spawn(async move {
                                let open_res = handle_inner
                                    .read()
                                    .await
                                    .channel_open_direct_tcpip(
                                        th.clone(),
                                        target_port as u32,
                                        peer.ip().to_string(),
                                        peer.port() as u32,
                                    )
                                    .await;
                                match open_res {
                                    Ok(channel) => {
                                        let mut ssh_stream = channel.into_stream();
                                        let _ = tokio::io::copy_bidirectional(&mut tcp, &mut ssh_stream).await;
                                    }
                                    Err(e) => {
                                        debug!("[Forward -L] direct-tcpip to {}:{} failed: {}", th, target_port, e);
                                    }
                                }
                            });
                        }
                    }
                }
            });
        }
        ForwardingKind::Dynamic => {
            let listener = TcpListener::bind((rule.bind_host.as_str(), rule.bind_port))
                .await
                .map_err(|e| {
                    format!(
                        "Failed to bind SOCKS5 {}:{}: {}",
                        rule.bind_host, rule.bind_port, e
                    )
                })?;
            if let Ok(addr) = listener.local_addr() {
                rule.bind_port = addr.port();
            }

            let handle_clone = handle.clone();
            let cancel_clone = cancel.clone();

            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = cancel_clone.cancelled() => break,
                        accepted = listener.accept() => {
                            let (tcp, peer) = match accepted {
                                Ok(pair) => pair,
                                Err(e) => {
                                    warn!("[Forward -D] Accept error: {}", e);
                                    break;
                                }
                            };
                            let handle_inner = handle_clone.clone();
                            tokio::spawn(async move {
                                if let Err(e) = handle_socks5_connection(tcp, peer, handle_inner).await {
                                    debug!("[Forward -D] SOCKS5 session ended: {}", e);
                                }
                            });
                        }
                    }
                }
            });
        }
        ForwardingKind::Remote => {
            let bound_port = handle
                .write()
                .await
                .tcpip_forward(rule.bind_host.clone(), rule.bind_port as u32)
                .await
                .map_err(|e| {
                    format!(
                        "Remote tcpip_forward {}:{} rejected by server: {}",
                        rule.bind_host, rule.bind_port, e
                    )
                })?;
            if bound_port > 0 && bound_port <= u16::MAX as u32 {
                rule.bind_port = bound_port as u16;
            }
            remote_registry.insert(
                rule.bind_host.clone(),
                rule.bind_port,
                rule.target_host.clone(),
                rule.target_port,
            );
        }
    }

    info!(
        "[Forward] Started {} forward: {}",
        rule.kind.short_tag(),
        rule.display_route()
    );

    Ok(ActivePortForward {
        rule,
        cancel,
        handle,
        remote_registry,
    })
}

/// 处理服务端推送的远程转发入站通道（-R）
pub fn spawn_remote_forward_bridge(
    channel: Channel<russh::client::Msg>,
    local_host: String,
    local_port: u16,
) {
    tokio::spawn(async move {
        match TcpStream::connect((local_host.as_str(), local_port)).await {
            Ok(mut tcp) => {
                let mut ssh_stream = channel.into_stream();
                let _ = tokio::io::copy_bidirectional(&mut ssh_stream, &mut tcp).await;
            }
            Err(e) => {
                debug!(
                    "[Forward -R] Failed to connect local target {}:{}: {}",
                    local_host, local_port, e
                );
            }
        }
    });
}

/// 解析 SOCKS5 地址字段（RFC 1928）
pub(crate) fn parse_socks5_target(atyp: u8, payload: &[u8]) -> Result<(String, u16), String> {
    match atyp {
        0x01 => {
            // IPv4: 4 bytes addr + 2 bytes port
            if payload.len() < 6 {
                return Err("truncated IPv4 SOCKS5 request".into());
            }
            let ip = Ipv4Addr::new(payload[0], payload[1], payload[2], payload[3]);
            let port = u16::from_be_bytes([payload[4], payload[5]]);
            Ok((ip.to_string(), port))
        }
        0x03 => {
            // Domain: 1 byte len + domain + 2 bytes port
            if payload.is_empty() {
                return Err("truncated domain SOCKS5 request".into());
            }
            let len = payload[0] as usize;
            if payload.len() < 1 + len + 2 {
                return Err("truncated domain SOCKS5 payload".into());
            }
            let domain = String::from_utf8(payload[1..1 + len].to_vec())
                .map_err(|_| "invalid UTF-8 domain in SOCKS5 request".to_string())?;
            let port = u16::from_be_bytes([payload[1 + len], payload[2 + len]]);
            Ok((domain, port))
        }
        0x04 => {
            // IPv6: 16 bytes addr + 2 bytes port
            if payload.len() < 18 {
                return Err("truncated IPv6 SOCKS5 request".into());
            }
            let mut octets = [0u8; 16];
            octets.copy_from_slice(&payload[..16]);
            let ip = Ipv6Addr::from(octets);
            let port = u16::from_be_bytes([payload[16], payload[17]]);
            Ok((ip.to_string(), port))
        }
        other => Err(format!("unsupported SOCKS5 ATYP: {other:#x}")),
    }
}

async fn handle_socks5_connection(
    mut tcp: TcpStream,
    peer: std::net::SocketAddr,
    handle: Arc<RwLock<Handle<SshClientHandler>>>,
) -> Result<(), String> {
    // 1. 认证方法协商（仅接受 NO AUTH 0x00）
    let mut header = [0u8; 2];
    tcp.read_exact(&mut header)
        .await
        .map_err(|e| e.to_string())?;
    if header[0] != 0x05 {
        return Err(format!("unsupported SOCKS version: {}", header[0]));
    }
    let nmethods = header[1] as usize;
    let mut methods = vec![0u8; nmethods];
    tcp.read_exact(&mut methods)
        .await
        .map_err(|e| e.to_string())?;
    if !methods.contains(&0x00) {
        let _ = tcp.write_all(&[0x05, 0xFF]).await;
        return Err("SOCKS5 client does not support NO AUTH".into());
    }
    tcp.write_all(&[0x05, 0x00])
        .await
        .map_err(|e| e.to_string())?;

    // 2. CONNECT 请求头
    let mut req = [0u8; 4];
    tcp.read_exact(&mut req).await.map_err(|e| e.to_string())?;
    if req[0] != 0x05 || req[1] != 0x01 {
        let _ = tcp
            .write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
            .await;
        return Err(format!("unsupported SOCKS5 command: {}", req[1]));
    }
    let atyp = req[3];
    let payload = match atyp {
        0x01 => {
            let mut buf = vec![0u8; 6];
            tcp.read_exact(&mut buf).await.map_err(|e| e.to_string())?;
            buf
        }
        0x03 => {
            let mut len_buf = [0u8; 1];
            tcp.read_exact(&mut len_buf)
                .await
                .map_err(|e| e.to_string())?;
            let len = len_buf[0] as usize;
            let mut buf = vec![0u8; 1 + len + 2];
            buf[0] = len_buf[0];
            tcp.read_exact(&mut buf[1..])
                .await
                .map_err(|e| e.to_string())?;
            buf
        }
        0x04 => {
            let mut buf = vec![0u8; 18];
            tcp.read_exact(&mut buf).await.map_err(|e| e.to_string())?;
            buf
        }
        other => {
            let _ = tcp
                .write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await;
            return Err(format!("unsupported SOCKS5 ATYP: {other:#x}"));
        }
    };

    let (dest_host, dest_port) = parse_socks5_target(atyp, &payload)?;

    // 3. 打开 SSH direct-tcpip 通道并回复成功
    let open_res = handle
        .read()
        .await
        .channel_open_direct_tcpip(
            dest_host.clone(),
            dest_port as u32,
            peer.ip().to_string(),
            peer.port() as u32,
        )
        .await;
    let channel = match open_res {
        Ok(ch) => ch,
        Err(e) => {
            let _ = tcp
                .write_all(&[0x05, 0x05, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
                .await;
            return Err(format!("direct-tcpip to {dest_host}:{dest_port} failed: {e}"));
        }
    };

    tcp.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0])
        .await
        .map_err(|e| e.to_string())?;

    let mut ssh_stream = channel.into_stream();
    let _ = tokio::io::copy_bidirectional(&mut tcp, &mut ssh_stream).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_socks5_ipv4_domain_and_ipv6_targets() {
        assert_eq!(
            parse_socks5_target(0x01, &[127, 0, 0, 1, 0x1F, 0x90]).unwrap(),
            ("127.0.0.1".into(), 8080)
        );
        let mut domain = vec![11u8];
        domain.extend_from_slice(b"example.com");
        domain.extend_from_slice(&443u16.to_be_bytes());
        assert_eq!(
            parse_socks5_target(0x03, &domain).unwrap(),
            ("example.com".into(), 443)
        );
        let mut ipv6 = [0u8; 18];
        ipv6[15] = 1;
        ipv6[16..18].copy_from_slice(&5432u16.to_be_bytes());
        assert_eq!(
            parse_socks5_target(0x04, &ipv6).unwrap(),
            ("::1".into(), 5432)
        );
    }

    #[test]
    fn remote_registry_matches_exact_address_or_port_fallback() {
        let reg = RemoteForwardRegistry::new();
        reg.insert("0.0.0.0".into(), 8080, "127.0.0.1".into(), 3000);
        assert_eq!(reg.resolve("0.0.0.0", 8080), Some(("127.0.0.1".into(), 3000)));
        assert_eq!(
            reg.resolve("localhost", 8080),
            Some(("127.0.0.1".into(), 3000))
        );
        assert_eq!(reg.resolve("0.0.0.0", 9000), None);
        reg.remove("0.0.0.0", 8080);
        assert_eq!(reg.resolve("0.0.0.0", 8080), None);
    }
}
