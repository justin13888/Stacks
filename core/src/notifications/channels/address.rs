//! Which addresses a notification may be delivered to.
//!
//! A channel's URL is the user's to choose, and the server makes the request,
//! so without a check any signed-in user could make Stacks reach its own
//! operator listener, the database, the identity provider or a cloud
//! metadata service. Every delivery therefore resolves its host through
//! [`GuardedResolver`] and connects only to the addresses it permits: the
//! check is made on the addresses actually connected to, so a name that
//! resolves to a public address when the channel is saved and to an internal
//! one when it is delivered (DNS rebinding) is refused all the same. A URL
//! whose host is an address literal never reaches a resolver, so it is
//! checked on its own ([`AddressPolicy::permits_url`]).
//!
//! Loopback, unspecified, link-local, multicast, broadcast and reserved
//! addresses are always refused, and so are the cloud metadata services in
//! [`METADATA_ADDRESSES`], even where an allow-listed block covers them.
//! Private and unique-local addresses (RFC
//! 1918, `100.64.0.0/10`, `fc00::/7`) are refused unless the operator
//! allow-lists them, by host name or by CIDR, with
//! `WW_NOTIFICATION_ALLOWED_HOSTS`. An IPv6 address that carries an IPv4 one
//! (IPv4-mapped, IPv4-compatible, NAT64, 6to4) is judged by the IPv4 address
//! it reaches.

use std::fmt;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

/// The cloud metadata services, refused whatever the allow-list says: each
/// hands out the host's credentials to whoever asks. Two are link-local and
/// so refused already, but are listed so the set is stated in one place.
///
/// - `169.254.169.254`: AWS, GCP, Azure, OpenStack and most others.
/// - `169.254.170.2`: the AWS ECS task metadata and credentials endpoint.
/// - `fd00:ec2::254`: the AWS instance metadata service over IPv6, inside
///   the unique-local `fc00::/7` an operator may allow-list.
/// - `fd20:ce::254`: the GCP metadata server on IPv6-only instances, also
///   inside `fc00::/7`.
/// - `100.100.100.200`: Alibaba Cloud, inside the shared `100.64.0.0/10`.
pub const METADATA_ADDRESSES: [IpAddr; 5] = [
    IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254)),
    IpAddr::V4(Ipv4Addr::new(169, 254, 170, 2)),
    IpAddr::V6(Ipv6Addr::new(0xfd00, 0xec2, 0, 0, 0, 0, 0, 0x254)),
    IpAddr::V6(Ipv6Addr::new(0xfd20, 0xce, 0, 0, 0, 0, 0, 0x254)),
    IpAddr::V4(Ipv4Addr::new(100, 100, 100, 200)),
];

fn is_metadata(ip: IpAddr) -> bool {
    METADATA_ADDRESSES.contains(&ip)
}

/// What an address is, as far as delivering to it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// Anywhere on the internet.
    Public,
    /// The operator's own network: permitted only when allow-listed.
    Private,
    /// The server itself, its link, or no single host: never permitted.
    Internal,
}

fn reach_v4(ip: Ipv4Addr) -> Reach {
    let [a, b, ..] = ip.octets();
    if is_metadata(IpAddr::V4(ip)) {
        return Reach::Internal;
    }
    if a == 0 || ip.is_loopback() || ip.is_link_local() || ip.is_multicast() || a >= 240 {
        // `0.0.0.0/8` is "this host", `240.0.0.0/4` is reserved and holds
        // the limited broadcast address.
        Reach::Internal
    } else if ip.is_private() || (a == 100 && (b & 0xc0) == 64) {
        Reach::Private
    } else {
        Reach::Public
    }
}

/// The IPv4 address carried in the last 32 bits of `segments[hi..=hi + 1]`.
fn embedded_v4(hi: u16, lo: u16) -> Ipv4Addr {
    let [a, b] = hi.to_be_bytes();
    let [c, d] = lo.to_be_bytes();
    Ipv4Addr::new(a, b, c, d)
}

fn reach_v6(ip: Ipv6Addr) -> Reach {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return reach_v4(v4);
    }
    let s = ip.segments();
    if ip.is_unspecified() || ip.is_loopback() || is_metadata(IpAddr::V6(ip)) {
        return Reach::Internal;
    }
    // IPv4-compatible (`::a.b.c.d`, deprecated) and the NAT64 well-known
    // prefix (`64:ff9b::/96`) both end in the IPv4 address they reach.
    if s[..6] == [0; 6] || s[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        return reach_v4(embedded_v4(s[6], s[7]));
    }
    // 6to4 (`2002::/16`) carries it in the second and third segments.
    if s[0] == 0x2002 {
        return reach_v4(embedded_v4(s[1], s[2]));
    }
    if ip.is_multicast() || (s[0] & 0xffc0) == 0xfe80 {
        Reach::Internal
    } else if (s[0] & 0xfe00) == 0xfc00 || (s[0] & 0xffc0) == 0xfec0 || s[..3] == [0x64, 0xff9b, 1]
    {
        // Unique-local, the deprecated site-local, and local-use NAT64
        // (`64:ff9b:1::/48`), which translates into the operator's network.
        Reach::Private
    } else {
        Reach::Public
    }
}

fn reach(ip: IpAddr) -> Reach {
    match ip {
        IpAddr::V4(v4) => reach_v4(v4),
        IpAddr::V6(v6) => reach_v6(v6),
    }
}

/// An address block, `address/prefix`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Cidr {
    network: IpAddr,
    prefix: u8,
}

impl Cidr {
    /// `a.b.c.d/n`, `x::y/n`, or a bare address (a single host).
    fn parse(raw: &str) -> Option<Self> {
        let (addr, prefix) = match raw.split_once('/') {
            Some((addr, prefix)) => (addr, Some(prefix.parse::<u8>().ok()?)),
            None => (raw, None),
        };
        let network = addr.parse::<IpAddr>().ok()?.to_canonical();
        let max = if network.is_ipv4() { 32 } else { 128 };
        let prefix = prefix.unwrap_or(max);
        (prefix <= max).then_some(Self { network, prefix })
    }

    fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, ip.to_canonical()) {
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

/// Which addresses deliveries may connect to: every public one, and the
/// private ones the operator allow-listed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddressPolicy {
    /// Host names whose private addresses are permitted, lower-case and
    /// without a trailing dot.
    hosts: Vec<String>,
    /// Blocks of private addresses that are permitted.
    nets: Vec<Cidr>,
    /// Blocks permitted whatever they are. Only test suites set it, to
    /// deliver to a receiver they run on loopback.
    exempt: Vec<Cidr>,
}

impl AddressPolicy {
    /// The policy `WW_NOTIFICATION_ALLOWED_HOSTS` describes: each entry is a
    /// host name or a CIDR (a bare address is a single host). An empty list
    /// permits public addresses only.
    pub fn from_allowed_hosts(entries: &[String]) -> Result<Self, String> {
        let mut policy = Self::default();
        for entry in entries.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
            if let Some(cidr) = Cidr::parse(entry) {
                policy.nets.push(cidr);
            } else if !is_host_name(entry) {
                return Err(format!(
                    "`{entry}` is neither a host name nor a CIDR (an entry takes no port, \
                     scheme, brackets, user name or wildcard)"
                ));
            } else {
                policy.hosts.push(normalise_host(entry));
            }
        }
        Ok(policy)
    }

    /// This policy, also permitting every address in `cidr`, loopback
    /// included. For test suites only: they run their receivers on
    /// `127.0.0.1`, which no shipped configuration can permit.
    ///
    /// # Panics
    ///
    /// When `cidr` is not one.
    #[cfg(any(test, feature = "test-support"))]
    pub fn exempting(mut self, cidr: &str) -> Self {
        self.exempt
            .push(Cidr::parse(cidr).expect("the exemption is a CIDR"));
        self
    }

    /// Whether a delivery to `host` may connect to `ip`.
    pub fn permits(&self, host: Option<&str>, ip: IpAddr) -> bool {
        if self.exempt.iter().any(|c| c.contains(ip)) {
            return true;
        }
        match reach(ip) {
            Reach::Public => true,
            Reach::Internal => false,
            Reach::Private => {
                self.nets.iter().any(|c| c.contains(ip))
                    || host.is_some_and(|h| self.hosts.contains(&normalise_host(h)))
            }
        }
    }

    /// Whether `url` can be delivered to as far as can be told without
    /// resolving it: a host that is an address literal must be permitted; a
    /// host name is judged when it is resolved.
    pub fn permits_url(&self, url: &reqwest::Url) -> bool {
        // A parsed URL's host is already normalised: an IPv4 address in its
        // canonical dotted form, an IPv6 one in brackets.
        let host = url.host_str().unwrap_or_default();
        let literal = host.strip_prefix('[').and_then(|h| h.strip_suffix(']'));
        match literal.unwrap_or(host).parse::<IpAddr>() {
            Ok(ip) => self.permits(None, ip),
            Err(_) => true,
        }
    }
}

/// Whether `raw` is a host name: dot-separated RFC 1123 labels (1 to 63
/// letters, digits and hyphens, neither starting nor ending with a hyphen),
/// at most 253 characters, with an optional trailing dot. A name whose last
/// label is all digits is refused too: a URL parser reads it as an IPv4
/// address, so as a name it would never match.
fn is_host_name(raw: &str) -> bool {
    let name = raw.strip_suffix('.').unwrap_or(raw);
    let label_ok = |l: &str| {
        (1..=63).contains(&l.len())
            && l.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            && !l.starts_with('-')
            && !l.ends_with('-')
    };
    !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(label_ok)
        && !name
            .rsplit('.')
            .next()
            .is_some_and(|l| l.bytes().all(|b| b.is_ascii_digit()))
}

fn normalise_host(host: &str) -> String {
    host.trim_end_matches('.').to_ascii_lowercase()
}

/// A delivery was refused because every address its host resolves to is
/// one the [`AddressPolicy`] refuses. It names nothing of the host, so the
/// error a delivery reports cannot say what is listening where.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefusedAddress;

impl fmt::Display for RefusedAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the receiver's address is not one Stacks delivers to")
    }
}

impl std::error::Error for RefusedAddress {}

/// The resolver every delivery connects through: the system's, keeping only
/// the addresses the policy permits.
#[derive(Debug, Clone)]
pub(crate) struct GuardedResolver {
    pub(crate) policy: Arc<AddressPolicy>,
}

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let policy = Arc::clone(&self.policy);
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let resolved: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            let permitted: Vec<SocketAddr> = resolved
                .iter()
                .copied()
                .filter(|a| policy.permits(Some(&host), a.ip()))
                .collect();
            if permitted.is_empty() && !resolved.is_empty() {
                return Err(Box::new(RefusedAddress) as _);
            }
            Ok(Box::new(permitted.into_iter()) as Addrs)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(raw: &str) -> IpAddr {
        raw.parse().unwrap()
    }

    fn strict() -> AddressPolicy {
        AddressPolicy::default()
    }

    #[test]
    fn internal_addresses_are_always_refused() {
        let generous = AddressPolicy::from_allowed_hosts(&[
            "0.0.0.0/0".into(),
            "::/0".into(),
            "localhost".into(),
        ])
        .unwrap();
        for raw in [
            "127.0.0.1",
            "127.1.2.3",
            "0.0.0.0",
            "169.254.169.254",
            "224.0.0.1",
            "255.255.255.255",
            "240.0.0.1",
            "::1",
            "::",
            "fe80::1",
            "ff02::1",
            // IPv4-mapped, IPv4-compatible, NAT64 and 6to4 forms of loopback
            // and of the cloud metadata address.
            "::ffff:127.0.0.1",
            "::ffff:169.254.169.254",
            "::127.0.0.1",
            "64:ff9b::7f00:1",
            "64:ff9b::a9fe:a9fe",
            "2002:7f00:1::1",
        ] {
            assert!(!strict().permits(None, ip(raw)), "{raw}");
            assert!(!generous.permits(Some("localhost"), ip(raw)), "{raw}");
        }
    }

    #[test]
    fn private_addresses_are_refused_unless_allow_listed() {
        let private = [
            "10.0.0.1",
            "172.16.5.4",
            "192.168.1.10",
            "100.64.0.1",
            "fc00::1",
            "fd12:3456::1",
            "::ffff:10.0.0.1",
            "64:ff9b::a00:1",
            "64:ff9b:1::1",
        ];
        for raw in private {
            assert!(!strict().permits(None, ip(raw)), "{raw}");
            assert!(!strict().permits(Some("apprise.lan"), ip(raw)), "{raw}");
        }
        let by_net = AddressPolicy::from_allowed_hosts(&[
            "10.0.0.0/8".into(),
            "192.168.1.10".into(),
            "fd00::/8".into(),
        ])
        .unwrap();
        for raw in ["10.0.0.1", "::ffff:10.9.9.9", "192.168.1.10", "fd12::1"] {
            assert!(by_net.permits(None, ip(raw)), "{raw}");
        }
        for raw in ["192.168.1.11", "172.16.0.1", "fc00::1"] {
            assert!(!by_net.permits(None, ip(raw)), "{raw}");
        }
        let by_name = AddressPolicy::from_allowed_hosts(&[" Apprise.LAN. ".into()]).unwrap();
        assert!(by_name.permits(Some("apprise.lan"), ip("192.168.1.10")));
        assert!(by_name.permits(Some("APPRISE.lan."), ip("fd00::1")));
        assert!(!by_name.permits(Some("other.lan"), ip("192.168.1.10")));
        assert!(!by_name.permits(None, ip("192.168.1.10")));
    }

    #[test]
    fn metadata_addresses_are_refused_under_any_allow_list() {
        let covering = AddressPolicy::from_allowed_hosts(&[
            "0.0.0.0/0".into(),
            "::/0".into(),
            "100.64.0.0/10".into(),
            "fd00::/8".into(),
            "metadata.internal".into(),
        ])
        .unwrap();
        for ip in METADATA_ADDRESSES {
            assert!(!strict().permits(None, ip), "{ip}");
            assert!(!covering.permits(None, ip), "{ip}");
            assert!(!covering.permits(Some("metadata.internal"), ip), "{ip}");
        }
        // Their IPv4-mapped and NAT64 forms reach the same service.
        for raw in ["::ffff:100.100.100.200", "64:ff9b::6464:64c8"] {
            assert!(!covering.permits(None, ip(raw)), "{raw}");
        }
        // The blocks around them are still the allow-list's to open.
        for raw in ["100.100.100.201", "fd00:ec2::253", "fd20:ce::253"] {
            assert!(!strict().permits(None, ip(raw)), "{raw}");
            assert!(covering.permits(None, ip(raw)), "{raw}");
        }
        let url = |raw: &str| reqwest::Url::parse(raw).unwrap();
        assert!(!covering.permits_url(&url("http://[fd00:ec2::254]/latest/")));
        assert!(!covering.permits_url(&url("http://[fd20:ce::254]/computeMetadata/v1")));
        assert!(!covering.permits_url(&url("http://100.100.100.200/latest/")));
    }

    #[test]
    fn public_addresses_are_permitted() {
        for raw in [
            "93.184.216.34",
            "1.1.1.1",
            "2606:4700::1111",
            "64:ff9b::808:808",
        ] {
            assert!(strict().permits(None, ip(raw)), "{raw}");
        }
    }

    #[test]
    fn an_allow_list_entry_is_a_host_name_or_a_cidr() {
        for bad in [
            "10.0.0.0/33",
            "fd00::/129",
            "10.0.0.0/x",
            "not a host",
            "a/b",
            // A port, a wildcard, a bracketed address, a user name, a
            // scheme, and malformed labels: none would ever match a host.
            "apprise:8000",
            "*.lan",
            "[fd00::1]",
            "user@apprise",
            "http://apprise",
            "-apprise",
            "apprise-",
            "a..b",
            ".",
            "under_score",
            "10.0.0.256",
            "0x7f.1",
        ] {
            assert!(
                AddressPolicy::from_allowed_hosts(&[bad.into()]).is_err(),
                "{bad}"
            );
        }
        let long_label = "a".repeat(64);
        let long_name = ["a"; 128].join(".");
        for bad in [long_label.as_str(), long_name.as_str()] {
            let err = AddressPolicy::from_allowed_hosts(&[bad.into()]).unwrap_err();
            assert!(err.contains(bad), "{err}");
        }
        let err = AddressPolicy::from_allowed_hosts(&["apprise:8000".into()]).unwrap_err();
        assert!(err.contains("`apprise:8000`"), "{err}");
        for good in [
            "apprise",
            "ntfy.lan.",
            "my-host.example",
            "1password.internal",
            &"a".repeat(63),
        ] {
            assert!(
                AddressPolicy::from_allowed_hosts(&[good.into()]).is_ok(),
                "{good}"
            );
        }
        assert_eq!(
            AddressPolicy::from_allowed_hosts(&[String::new(), " ".into()]).unwrap(),
            strict()
        );
    }

    #[test]
    fn an_address_literal_url_is_judged_without_resolving() {
        let url = |raw: &str| reqwest::Url::parse(raw).unwrap();
        for raw in [
            "http://127.0.0.1:9095/metrics",
            "http://[::ffff:7f00:1]/",
            // WHATWG URL parsing folds these spellings of loopback.
            "http://2130706433/",
            "http://0x7f.1/",
            "http://169.254.169.254/latest/meta-data/",
            "http://10.0.0.1/",
        ] {
            assert!(!strict().permits_url(&url(raw)), "{raw}");
        }
        assert!(strict().permits_url(&url("https://hooks.example/x")));
        assert!(strict().permits_url(&url("http://localhost/")));
        assert!(strict().permits_url(&url("https://1.1.1.1/")));
    }

    #[tokio::test]
    async fn the_resolver_keeps_only_permitted_addresses() {
        let resolver = GuardedResolver {
            policy: Arc::new(strict()),
        };
        let refused = resolver
            .resolve("localhost".parse().unwrap())
            .await
            .err()
            .expect("localhost resolves to loopback only");
        assert!(
            refused.downcast_ref::<RefusedAddress>().is_some(),
            "{refused}"
        );

        let exempt = GuardedResolver {
            policy: Arc::new(strict().exempting("127.0.0.0/8").exempting("::1")),
        };
        let addrs: Vec<SocketAddr> = exempt
            .resolve("localhost".parse().unwrap())
            .await
            .unwrap()
            .collect();
        assert!(!addrs.is_empty());
        assert!(addrs.iter().all(|a| a.ip().is_loopback()));
    }
}
