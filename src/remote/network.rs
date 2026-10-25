//! Local interface discovery for connection setup. Call on a worker; no packets
//! are sent and no external service or command is needed.
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
mod routing;
pub use routing::{reachable_address, reachable_addresses};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalAddress {
    interface: String,
    ip: Ipv4Addr,
}
impl LocalAddress {
    pub fn interface(&self) -> &str {
        &self.interface
    }
    pub fn ip(&self) -> Ipv4Addr {
        self.ip
    }
}

pub fn local_addresses() -> Result<Vec<LocalAddress>, String> {
    let mut addresses = platform_addresses()?;
    addresses.retain(|a| {
        !a.ip.is_loopback()
            && !a.ip.is_unspecified()
            && !a.ip.is_link_local()
            && !a.ip.is_multicast()
            && !a.ip.is_broadcast()
    });
    addresses.sort_by_key(|a| (!vpn(a.ip), !a.ip.is_private(), a.ip, a.interface.clone()));
    addresses.dedup_by_key(|a| a.ip);
    addresses.truncate(32);
    Ok(addresses)
}
fn vpn(ip: Ipv4Addr) -> bool {
    ip.octets()[0] == 100 && ip.octets()[1] & 0xc0 == 0x40
}

/// Validate the address embedded in a direct invitation against the real listener.
/// A local-only listener cannot be made reachable by advertising a LAN address.
pub fn invitation_address(listener: SocketAddr, address: &str) -> Result<String, String> {
    let address = address.trim();
    let host = url::Host::parse(address)
        .map_err(|_| "Choose a computer address, without https:// or a port.".to_owned())?;
    let ip = match &host {
        url::Host::Ipv4(ip) => Some(IpAddr::V4(*ip)),
        url::Host::Ipv6(ip) => Some(IpAddr::V6(*ip)),
        url::Host::Domain(_) => None,
    };
    if ip.is_some_and(|ip| {
        ip.is_unspecified()
            || ip.is_multicast()
            || matches!(ip, IpAddr::V4(ip) if ip.is_broadcast())
    }) {
        return Err("Choose this computer's network address. A listen-on-all address cannot be used by another device.".into());
    }
    let localhost = matches!(&host, url::Host::Domain(name) if name == "localhost");
    if listener.ip().is_loopback() && !localhost && !ip.is_some_and(|ip| ip.is_loopback()) {
        return Err("This computer currently accepts local or SSH connections only. Enable network access before creating a network invitation.".into());
    }
    if let Some(ip) = ip {
        if !listener.ip().is_unspecified() && !listener.ip().is_loopback() && listener.ip() != ip {
            return Err("This address does not match the active listener. Choose its address or change Advanced settings.".into());
        }
        if listener.is_ipv4() != ip.is_ipv4() {
            return Err(
                "Choose an address matching the listener’s IP version, or change Advanced settings."
                    .into(),
            );
        }
    }
    Ok(host.to_string())
}

#[cfg(unix)]
fn platform_addresses() -> Result<Vec<LocalAddress>, String> {
    use std::{ffi::CStr, ptr};
    let mut first = ptr::null_mut();
    // SAFETY: getifaddrs initializes the pointer, retained until freeifaddrs.
    if unsafe { libc::getifaddrs(&mut first) } != 0 {
        return Err(
            "Could not read network addresses. Enter an address in Advanced settings.".into(),
        );
    }
    struct Interfaces(*mut libc::ifaddrs);
    impl Drop for Interfaces {
        fn drop(&mut self) {
            unsafe {
                libc::freeifaddrs(self.0);
            }
        }
    }
    let _interfaces = Interfaces(first);
    let mut result = vec![];
    let mut next = first;
    while !next.is_null() && result.len() < 128 {
        // SAFETY: each address and name belongs to the live getifaddrs allocation.
        let item = unsafe { &*next };
        if !item.ifa_addr.is_null()
            && !item.ifa_name.is_null()
            && item.ifa_flags & libc::IFF_UP as u32 != 0
            && unsafe { (*item.ifa_addr).sa_family as i32 } == libc::AF_INET
        {
            let address = unsafe { &*item.ifa_addr.cast::<libc::sockaddr_in>() };
            let ip = Ipv4Addr::from(address.sin_addr.s_addr.to_ne_bytes());
            let interface = unsafe { CStr::from_ptr(item.ifa_name) }
                .to_string_lossy()
                .into_owned();
            result.push(LocalAddress { interface, ip });
        }
        next = item.ifa_next;
    }
    Ok(result)
}

#[cfg(not(unix))]
fn platform_addresses() -> Result<Vec<LocalAddress>, String> {
    // UDP connect selects the OS route without sending a datagram. This gives the
    // preferred interface; extra VPN addresses can still be entered explicitly.
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")
        .map_err(|_| "Could not read the network address.")?;
    socket
        .connect("192.0.2.1:9")
        .map_err(|_| "No network route is available. Connect to Wi-Fi or VPN and refresh.")?;
    match socket
        .local_addr()
        .map_err(|_| "Could not read the network address.")?
        .ip()
    {
        IpAddr::V4(ip) => Ok(vec![LocalAddress {
            interface: "Network".into(),
            ip,
        }]),
        _ => Ok(vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invitations_cannot_advertise_an_unreachable_listener() {
        let ssh = "127.0.0.1:44330".parse().unwrap();
        assert!(invitation_address(ssh, "192.168.1.25").is_err());
        assert_eq!(invitation_address(ssh, "127.0.0.1").unwrap(), "127.0.0.1");
        assert_eq!(invitation_address(ssh, "localhost").unwrap(), "localhost");
        let lan = "0.0.0.0:44330".parse().unwrap();
        for bad in [
            "0.0.0.0",
            "224.0.0.1",
            "255.255.255.255",
            "https://192.168.1.25",
            "192.168.1.25:44330",
        ] {
            assert!(invitation_address(lan, bad).is_err(), "{bad}");
        }
        assert_eq!(
            invitation_address(lan, "192.168.1.25").unwrap(),
            "192.168.1.25"
        );
        assert!(invitation_address("192.168.1.25:44330".parse().unwrap(), "192.168.1.26").is_err());
        assert!(invitation_address(lan, "[::1]").is_err());
    }
}
