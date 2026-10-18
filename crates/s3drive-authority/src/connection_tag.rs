//! Diagnostic correlation only: never an authorization or replay capability.
use std::{
    net::{IpAddr, SocketAddr},
    sync::atomic::{AtomicU64, Ordering},
};

/// Only canonical loopback endpoints are representable; no IP/host is retained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoopbackSocket {
    pub ipv6: bool,
    pub client_port: u16,
    pub server_port: u16,
}
impl LoopbackSocket {
    pub(crate) fn from_endpoints(client: SocketAddr, server: SocketAddr) -> Option<Self> {
        let ipv6 = match (client.ip(), server.ip()) {
            (IpAddr::V4(c), IpAddr::V4(s)) if c == std::net::Ipv4Addr::LOCALHOST && s == c => false,
            (IpAddr::V6(c), IpAddr::V6(s)) if c == std::net::Ipv6Addr::LOCALHOST && s == c => true,
            _ => return None,
        };
        (client.port() != 0 && server.port() != 0).then_some(Self {
            ipv6,
            client_port: client.port(),
            server_port: server.port(),
        })
    }
    fn encode(self) -> u64 {
        (1 << 33)
            | (u64::from(self.ipv6) << 32)
            | (u64::from(self.client_port) << 16)
            | u64::from(self.server_port)
    }
}

/// Snapshot of successful connector candidates, not the selected RPC socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectorCandidate {
    Unobserved,
    /// Metadata is absent or is not a canonical loopback address pair.
    Omitted,
    Loopback(LoopbackSocket),
    /// More than one successful connector; no individual candidate is exposed.
    Multiple,
}

/// Connection information reported by a pinned dependency's error. Even a
/// reported connection does not prove delivery, authentication or commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorConnection {
    /// No matching legacy error was observed in the bounded source walk.
    Unobserved,
    /// A legacy error was observed, but its connect_info() returned None.
    Unavailable,
    /// Connection info exists, but TCP metadata is missing or not loopback.
    Omitted,
    Loopback(LoopbackSocket),
    /// More than one legacy error was observed; no single connection is exposed.
    Multiple,
}
impl ErrorConnection {
    pub(crate) fn observe(
        &mut self,
        info: Option<&hyper_util::client::legacy::connect::Connected>,
    ) {
        if *self != Self::Unobserved {
            *self = Self::Multiple;
            return;
        }
        let Some(info) = info else {
            *self = Self::Unavailable;
            return;
        };
        let mut extras = axum::http::Extensions::new();
        info.get_extras(&mut extras);
        *self = extras
            .get::<hyper_util::client::legacy::connect::HttpInfo>()
            .and_then(|info| LoopbackSocket::from_endpoints(info.local_addr(), info.remote_addr()))
            .map_or(Self::Omitted, Self::Loopback);
    }
}
#[derive(Default)]
pub(crate) struct Candidate(AtomicU64);
impl Candidate {
    const OMITTED: u64 = u64::MAX;
    const MULTIPLE: u64 = u64::MAX - 1;
    pub fn observe(&self, value: Option<LoopbackSocket>) {
        let value = value.map_or(Self::OMITTED, LoopbackSocket::encode);
        if self
            .0
            .compare_exchange(0, value, Ordering::Release, Ordering::Relaxed)
            .is_err()
        {
            self.0.store(Self::MULTIPLE, Ordering::Release);
        }
    }
    pub fn snapshot(&self) -> ConnectorCandidate {
        match self.0.load(Ordering::Acquire) {
            0 => ConnectorCandidate::Unobserved,
            Self::OMITTED => ConnectorCandidate::Omitted,
            Self::MULTIPLE => ConnectorCandidate::Multiple,
            value => ConnectorCandidate::Loopback(LoopbackSocket {
                ipv6: value & (1 << 32) != 0,
                client_port: ((value >> 16) & 0xffff) as u16,
                server_port: (value & 0xffff) as u16,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_canonical_matching_loopback_families_and_nonzero_ports_are_retained() {
        for (client, server, ipv6) in [
            ("127.0.0.1:1234", "127.0.0.1:4321", false),
            ("[::1]:1234", "[::1]:4321", true),
        ] {
            let socket =
                LoopbackSocket::from_endpoints(client.parse().unwrap(), server.parse().unwrap())
                    .unwrap();
            assert_eq!(
                socket,
                LoopbackSocket {
                    ipv6,
                    client_port: 1234,
                    server_port: 4321
                }
            );
            let candidate = Candidate::default();
            candidate.observe(Some(socket));
            assert_eq!(candidate.snapshot(), ConnectorCandidate::Loopback(socket));
        }
        for (client, server) in [
            ("192.0.2.10:1234", "127.0.0.1:4321"),
            ("127.0.0.1:1234", "192.0.2.10:4321"),
            ("127.0.0.2:1234", "127.0.0.2:4321"),
            ("[::1]:1234", "127.0.0.1:4321"),
            ("[::ffff:127.0.0.1]:1234", "[::ffff:127.0.0.1]:4321"),
            ("127.0.0.1:0", "127.0.0.1:4321"),
        ] {
            assert!(
                LoopbackSocket::from_endpoints(client.parse().unwrap(), server.parse().unwrap())
                    .is_none()
            );
        }
    }
    #[test]
    fn missing_and_multiple_candidates_never_expose_a_misleading_single_socket() {
        let tag = LoopbackSocket {
            ipv6: false,
            client_port: 1234,
            server_port: 4321,
        };
        for first in [Some(tag), None] {
            let candidate = Candidate::default();
            assert_eq!(candidate.snapshot(), ConnectorCandidate::Unobserved);
            candidate.observe(first);
            assert_eq!(
                candidate.snapshot(),
                first.map_or(ConnectorCandidate::Omitted, ConnectorCandidate::Loopback)
            );
            candidate.observe(Some(tag));
            assert_eq!(candidate.snapshot(), ConnectorCandidate::Multiple);
            candidate.observe(None);
            assert_eq!(candidate.snapshot(), ConnectorCandidate::Multiple);
        }
    }
    #[test]
    fn simultaneous_successes_remain_ambiguous_instead_of_replacing_the_first_candidate() {
        let candidate = Candidate::default();
        let tag = LoopbackSocket {
            ipv6: false,
            client_port: 1234,
            server_port: 4321,
        };
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| candidate.observe(Some(tag)));
            }
        });
        assert_eq!(candidate.snapshot(), ConnectorCandidate::Multiple);
    }
    #[test]
    fn error_info_distinguishes_absence_omits_unknown_extras_and_refuses_ambiguity() {
        use hyper_util::client::legacy::connect::Connected;
        let private = "never-retain-unknown-connection-extra";
        let info = Connected::new().extra(private.to_owned());
        let mut observed = ErrorConnection::Unobserved;
        observed.observe(Some(&info));
        assert_eq!(observed, ErrorConnection::Omitted);
        assert!(!format!("{observed:?}").contains(private));
        observed.observe(None);
        assert_eq!(observed, ErrorConnection::Multiple);
        observed.observe(Some(&info));
        assert_eq!(observed, ErrorConnection::Multiple);
        let mut unavailable = ErrorConnection::Unobserved;
        unavailable.observe(None);
        assert_eq!(unavailable, ErrorConnection::Unavailable);
        unavailable.observe(Some(&Connected::new()));
        assert_eq!(unavailable, ErrorConnection::Multiple);
    }
}
