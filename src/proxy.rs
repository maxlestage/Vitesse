//! Trusted reverse proxies (`app.set('trust proxy', …)` in Express): which
//! `X-Forwarded-For`, `X-Forwarded-Proto` and `X-Forwarded-Host` headers
//! [`Request::ip`](crate::Request::ip), [`Request::protocol`](crate::Request::protocol)
//! and [`Request::hostname`](crate::Request::hostname) believe.

use std::fmt;
use std::net::IpAddr;

/// Which proxies are trusted to report the client's address, protocol and
/// host, set with [`App::trust_proxy`](crate::App::trust_proxy).
///
/// | Value | Meaning |
/// |---|---|
/// | `false` | Nobody (the default): `req.ip()` is the address of the TCP peer |
/// | `true` | Every proxy: the client is the left-most address of `X-Forwarded-For` |
/// | `2` (a number) | The `n` proxies closest to the server |
/// | `"loopback, 10.0.0.0/8"` | The proxies whose address is in these networks |
///
/// Addresses can be IPs or networks in CIDR notation, plus the names
/// `loopback` (`127.0.0.1/8`, `::1`), `linklocal` (`169.254.0.0/16`,
/// `fe80::/10`) and `uniquelocal` (`10.0.0.0/8`, `172.16.0.0/12`,
/// `192.168.0.0/16`, `fc00::/7`), like in Express.
///
/// ```
/// use vitesse::TrustProxy;
///
/// let trust: TrustProxy = "loopback, 10.0.0.0/8".parse().unwrap();
/// assert!(trust.trusts("10.1.2.3".parse().unwrap(), 0));
/// assert!(!trust.trusts("203.0.113.9".parse().unwrap(), 0));
/// ```
#[derive(Clone, Default, PartialEq, Eq)]
pub enum TrustProxy {
    /// No proxy is trusted.
    #[default]
    None,
    /// Every proxy is trusted.
    All,
    /// The given number of proxies, the closest to the server, are trusted.
    Hops(usize),
    /// The proxies whose address is in one of these networks are trusted.
    Networks(Vec<Network>),
}

/// An IP network (`10.0.0.0/8`), for [`TrustProxy::Networks`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Network {
    addr: IpAddr,
    prefix: u8,
}

impl Network {
    /// Whether `ip` belongs to this network.
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip.to_canonical()) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix))
                    .unwrap_or(0);
                u32::from(net) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix))
                    .unwrap_or(0);
                u128::from(net) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

impl fmt::Debug for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.addr, self.prefix)
    }
}

impl std::str::FromStr for Network {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let s = s.trim();
        let (addr, prefix) = match s.split_once('/') {
            Some((addr, prefix)) => (addr, Some(prefix)),
            None => (s, None),
        };
        let addr: IpAddr = addr
            .parse()
            .map_err(|_| format!("invalid IP address '{addr}'"))?;
        let addr = addr.to_canonical();
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(p) => p
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= max)
                .ok_or_else(|| format!("invalid network prefix in '{s}'"))?,
            None => max,
        };
        Ok(Network { addr, prefix })
    }
}

impl TrustProxy {
    /// Whether the proxy at `ip`, `hop` steps away from the server (`0`: the
    /// TCP peer), is trusted.
    pub fn trusts(&self, ip: IpAddr, hop: usize) -> bool {
        match self {
            TrustProxy::None => false,
            TrustProxy::All => true,
            TrustProxy::Hops(n) => hop < *n,
            TrustProxy::Networks(networks) => networks.iter().any(|n| n.contains(ip)),
        }
    }

    /// The client's address: walks `X-Forwarded-For` from the right, as
    /// long as the addresses are trusted proxies.
    pub(crate) fn client_ip<'a>(
        &self,
        peer: IpAddr,
        forwarded_for: impl Iterator<Item = &'a str>,
    ) -> IpAddr {
        if *self == TrustProxy::None {
            return peer;
        }
        // Les valeurs, de la plus proche du serveur à la plus lointaine.
        let mut hops: Vec<&str> = forwarded_for.flat_map(|v| v.split(',')).collect();
        hops.reverse();
        let mut client = peer;
        for (i, hop) in hops.into_iter().enumerate() {
            if !self.trusts(client, i) {
                return client;
            }
            match parse_forwarded_ip(hop) {
                Some(ip) => client = ip,
                // Une valeur illisible : on s'arrête au dernier proxy de confiance.
                None => return client,
            }
        }
        client
    }
}

/// An address of `X-Forwarded-For`: `203.0.113.9`, `[2001:db8::1]:443`,
/// `198.51.100.7:8080`…
fn parse_forwarded_ip(value: &str) -> Option<IpAddr> {
    let value = value.trim();
    if let Ok(ip) = value.parse::<IpAddr>() {
        return Some(ip.to_canonical());
    }
    if let Some(rest) = value.strip_prefix('[') {
        return rest.split_once(']')?.0.parse::<IpAddr>().ok();
    }
    let (ip, _port) = value.rsplit_once(':')?;
    ip.parse::<std::net::Ipv4Addr>().ok().map(IpAddr::V4)
}

impl fmt::Debug for TrustProxy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrustProxy::None => f.write_str("TrustProxy::None"),
            TrustProxy::All => f.write_str("TrustProxy::All"),
            TrustProxy::Hops(n) => write!(f, "TrustProxy::Hops({n})"),
            TrustProxy::Networks(n) => f.debug_tuple("TrustProxy::Networks").field(n).finish(),
        }
    }
}

impl From<bool> for TrustProxy {
    fn from(all: bool) -> Self {
        if all {
            TrustProxy::All
        } else {
            TrustProxy::None
        }
    }
}

impl From<usize> for TrustProxy {
    fn from(hops: usize) -> Self {
        TrustProxy::Hops(hops)
    }
}

impl From<i32> for TrustProxy {
    fn from(hops: i32) -> Self {
        TrustProxy::Hops(usize::try_from(hops).unwrap_or(0))
    }
}

impl std::str::FromStr for TrustProxy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let mut networks = Vec::new();
        for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let named: &[&str] = match part {
                "loopback" => &["127.0.0.1/8", "::1/128"],
                "linklocal" => &["169.254.0.0/16", "fe80::/10"],
                "uniquelocal" => &["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16", "fc00::/7"],
                other => {
                    networks.push(other.parse()?);
                    continue;
                }
            };
            for network in named {
                networks.push(network.parse()?);
            }
        }
        Ok(TrustProxy::Networks(networks))
    }
}

impl From<&str> for TrustProxy {
    /// # Panics
    /// If an address is invalid (a configuration error, caught at startup).
    #[track_caller]
    fn from(s: &str) -> Self {
        match s.parse() {
            Ok(trust) => trust,
            Err(e) => panic!("invalid trust_proxy value: {e}"),
        }
    }
}

impl From<Vec<Network>> for TrustProxy {
    fn from(networks: Vec<Network>) -> Self {
        TrustProxy::Networks(networks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn networks() {
        let n: Network = "10.0.0.0/8".parse().unwrap();
        assert!(n.contains(ip("10.200.1.1")));
        assert!(!n.contains(ip("11.0.0.1")));
        assert!(n.contains(ip("::ffff:10.0.0.1")));
        let n: Network = "fd00::/8".parse().unwrap();
        assert!(n.contains(ip("fd12::1")));
        assert!(!n.contains(ip("fe80::1")));
        let n: Network = "0.0.0.0/0".parse().unwrap();
        assert!(n.contains(ip("1.2.3.4")));
        assert!("10.0.0.0/33".parse::<Network>().is_err());
        assert!("nope".parse::<Network>().is_err());
    }

    #[test]
    fn client_ip_like_express() {
        let xff = ["203.0.113.9, 198.51.100.7", "10.0.0.2"];
        let peer = ip("127.0.0.1");
        let walk = |trust: TrustProxy| trust.client_ip(peer, xff.iter().copied());
        assert_eq!(walk(TrustProxy::None), peer);
        assert_eq!(walk(TrustProxy::All), ip("203.0.113.9"));
        assert_eq!(walk(TrustProxy::Hops(1)), ip("10.0.0.2"));
        assert_eq!(walk(TrustProxy::Hops(2)), ip("198.51.100.7"));
        assert_eq!(walk(TrustProxy::Hops(10)), ip("203.0.113.9"));
        assert_eq!(walk("loopback, 10.0.0.0/8".into()), ip("198.51.100.7"));
        assert_eq!(walk("10.0.0.0/8".into()), peer);
        // Ports et crochets.
        let xff = ["[2001:db8::1]:443, 198.51.100.7:8080"];
        assert_eq!(
            TrustProxy::All.client_ip(peer, xff.iter().copied()),
            ip("2001:db8::1")
        );
    }
}
