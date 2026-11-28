// TCP 连接建立
//
// - DNS 异步解析，解析耗时同样受连接超时约束；
// - 对所有解析结果按 IPv6 / IPv4 交替排列，每隔 250ms 发起一个新的连接尝试，先连上者胜出
//   （RFC 8305 Happy Eyeballs 的简化版），某个地址族不通时不会一直卡到超时。

use std::collections::VecDeque;
use std::io;
use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::TcpStream;
use tokio::task::JoinSet;
use tokio::time::{sleep_until, timeout_at, Instant};

use super::error::SshError;

/// 两次连接尝试之间的间隔（RFC 8305 建议值）
const ATTEMPT_DELAY: Duration = Duration::from_millis(250);

/// 解析主机名并建立 TCP 连接，整体耗时不超过 `limit`
pub async fn connect_tcp(host: &str, port: u16, limit: Duration) -> Result<TcpStream, SshError> {
    let deadline = Instant::now() + limit;
    let timeout_error = || SshError::Timeout(limit.as_secs());

    let resolved: Vec<SocketAddr> = timeout_at(deadline, tokio::net::lookup_host((host, port)))
        .await
        .map_err(|_| timeout_error())?
        .map_err(|e| resolve_error(host, e))?
        .collect();
    let addrs = interleave_families(resolved);
    if addrs.is_empty() {
        return Err(resolve_error(
            host,
            io::Error::new(io::ErrorKind::NotFound, "no address found"),
        ));
    }

    let mut remaining = addrs.into_iter();
    let mut attempts: JoinSet<io::Result<TcpStream>> = JoinSet::new();
    let mut last_error: Option<io::Error> = None;
    let mut next_launch = Instant::now();

    loop {
        let can_launch = remaining.len() > 0;
        if attempts.is_empty() && !can_launch {
            break;
        }

        tokio::select! {
            // 到点（或上一个尝试已失败）就发起下一个地址的连接
            _ = sleep_until(next_launch), if can_launch => {
                if let Some(addr) = remaining.next() {
                    attempts.spawn(TcpStream::connect(addr));
                }
                next_launch = Instant::now() + ATTEMPT_DELAY;
            }
            joined = attempts.join_next(), if !attempts.is_empty() => {
                match joined {
                    // 成功：丢弃 JoinSet 时其余尝试会被自动中止
                    Some(Ok(Ok(stream))) => return Ok(stream),
                    Some(Ok(Err(e))) => {
                        last_error = Some(e);
                        next_launch = Instant::now();
                    }
                    Some(Err(e)) => {
                        last_error = Some(io::Error::other(e.to_string()));
                        next_launch = Instant::now();
                    }
                    None => {}
                }
            }
            _ = sleep_until(deadline) => return Err(timeout_error()),
        }
    }

    Err(SshError::Io(last_error.unwrap_or_else(|| {
        io::Error::other(format!("could not connect to {}:{}", host, port))
    })))
}

/// DNS 解析失败归为 IO 错误（可重试）：切换网络或刚唤醒时 DNS 往往暂时不可用，
/// 自动重连不应因此直接放弃
fn resolve_error(host: &str, error: io::Error) -> SshError {
    SshError::Io(io::Error::new(
        error.kind(),
        format!("failed to resolve '{}': {}", host, error),
    ))
}

/// 保留解析器给出的首选地址族，然后两个地址族交替排列
fn interleave_families(addrs: Vec<SocketAddr>) -> Vec<SocketAddr> {
    let Some(first) = addrs.first() else {
        return addrs;
    };
    let prefer_v6 = first.is_ipv6();
    let (mut preferred, mut other): (VecDeque<_>, VecDeque<_>) = addrs
        .iter()
        .copied()
        .partition(|addr| addr.is_ipv6() == prefer_v6);

    let mut result = Vec::with_capacity(addrs.len());
    loop {
        match (preferred.pop_front(), other.pop_front()) {
            (None, None) => break,
            (a, b) => result.extend(a.into_iter().chain(b)),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleaves_address_families_keeping_preferred_first() {
        let v6a: SocketAddr = "[2001:db8::1]:22".parse().unwrap();
        let v6b: SocketAddr = "[2001:db8::2]:22".parse().unwrap();
        let v4a: SocketAddr = "192.0.2.1:22".parse().unwrap();
        let v4b: SocketAddr = "192.0.2.2:22".parse().unwrap();

        assert_eq!(
            interleave_families(vec![v6a, v6b, v4a, v4b]),
            vec![v6a, v4a, v6b, v4b]
        );
        assert_eq!(
            interleave_families(vec![v4a, v6a, v6b]),
            vec![v4a, v6a, v6b]
        );
        assert!(interleave_families(Vec::new()).is_empty());
    }

    #[test]
    fn dns_failures_are_retryable() {
        let error = resolve_error(
            "example.com",
            io::Error::new(io::ErrorKind::Other, "nodename nor servname provided"),
        );
        assert!(error.is_retryable());
        assert!(error.to_string().contains("example.com"));
    }

    #[tokio::test]
    async fn falls_back_to_next_address_when_first_refuses() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let _ = listener.accept().await;
        });

        // "localhost" 可能先解析出 ::1（未监听，立即被拒），应自动回退到 127.0.0.1
        let stream = connect_tcp("localhost", port, Duration::from_secs(5))
            .await
            .expect("connects via a working address");
        assert_eq!(stream.peer_addr().unwrap().port(), port);
    }
}
