//! Read the kernel's route selection before opening a socket. Private networks
//! require a specific route; an Internet default does not make a LAN reachable.
use std::net::{IpAddr, ToSocketAddrs};

pub fn reachable_address(host: &str, port: u16) -> Result<IpAddr, String> {
    Ok(reachable_addresses(host, port)?[0])
}
pub fn reachable_addresses(host: &str, port: u16) -> Result<Vec<IpAddr>, String> {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    let addresses = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![ip]
    } else {
        (host, port)
            .to_socket_addrs()
            .map_err(|_| "The computer address could not be resolved.")?
            .map(|a| a.ip())
            .take(16)
            .collect()
    };
    select_address(addresses, route_prefix)
}
fn select_address(
    addresses: Vec<IpAddr>,
    lookup: impl Fn(IpAddr) -> Result<Option<u8>, ()>,
) -> Result<Vec<IpAddr>, String> {
    let mut lookup_failed = false;
    let mut available = vec![];
    for ip in addresses {
        if ip.is_unspecified() || ip.is_multicast() {
            continue;
        }
        if ip.is_loopback() {
            available.push(ip);
            continue;
        }
        match lookup(ip) {
            Ok(Some(prefix)) if prefix > 0 || !private_network(ip) => available.push(ip),
            Ok(_) => {}
            Err(_) => lookup_failed = true,
        }
    }
    if !available.is_empty() {
        return Ok(available);
    }
    Err(if lookup_failed {
        "Skipped: the system could not verify a route to this network. Check your network or VPN."
    } else {
        "Skipped: no active route to this network. Connect to its Wi-Fi or VPN."
    }
    .into())
}
fn private_network(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_link_local()
                || ip.octets()[0] == 100 && ip.octets()[1] & 0xc0 == 0x40
        }
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}

#[cfg(target_os = "linux")]
fn route_prefix(ip: IpAddr) -> Result<Option<u8>, ()> {
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    // RTM_GETROUTE + FIB_MATCH performs a read-only lookup through policy rules,
    // including VPN tables. /proc/net/route contains only the main table.
    let descriptor = unsafe {
        libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_RAW | libc::SOCK_CLOEXEC,
            libc::NETLINK_ROUTE,
        )
    };
    if descriptor < 0 {
        return Err(());
    }
    let socket = unsafe { OwnedFd::from_raw_fd(descriptor) };
    let timeout = libc::timeval {
        tv_sec: 0,
        tv_usec: 250_000,
    };
    if unsafe {
        libc::setsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            (&timeout as *const libc::timeval).cast(),
            std::mem::size_of_val(&timeout) as _,
        )
    } != 0
    {
        return Err(());
    }
    let (family, destination) = match ip {
        IpAddr::V4(ip) => (libc::AF_INET, ip.octets().to_vec()),
        IpAddr::V6(ip) => (libc::AF_INET6, ip.octets().to_vec()),
    };
    let mut request = vec![0u8; 32 + destination.len()];
    let length = request.len() as u32;
    request[..4].copy_from_slice(&length.to_ne_bytes());
    request[4..6].copy_from_slice(&libc::RTM_GETROUTE.to_ne_bytes());
    request[6..8].copy_from_slice(&(libc::NLM_F_REQUEST as u16).to_ne_bytes());
    request[8..12].copy_from_slice(&1u32.to_ne_bytes());
    request[16] = family as u8;
    request[17] = (destination.len() * 8) as u8;
    request[24..28].copy_from_slice(&0x2000u32.to_ne_bytes()); // RTM_F_FIB_MATCH
    request[28..30].copy_from_slice(&((4 + destination.len()) as u16).to_ne_bytes());
    request[30..32].copy_from_slice(&1u16.to_ne_bytes()); // RTA_DST
    request[32..].copy_from_slice(&destination);
    let mut kernel: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
    kernel.nl_family = libc::AF_NETLINK as _;
    let sent = unsafe {
        libc::sendto(
            socket.as_raw_fd(),
            request.as_ptr().cast(),
            request.len(),
            0,
            (&kernel as *const libc::sockaddr_nl).cast(),
            std::mem::size_of_val(&kernel) as _,
        )
    };
    if sent != request.len() as isize {
        return Err(());
    }
    let mut reply = [0u8; 16 * 1024];
    let count = unsafe {
        libc::recv(
            socket.as_raw_fd(),
            reply.as_mut_ptr().cast(),
            reply.len(),
            0,
        )
    };
    if count < 28 {
        return Err(());
    }
    let length = u32::from_ne_bytes(reply[..4].try_into().unwrap()) as usize;
    if length < 28 || length > count as usize {
        return Err(());
    }
    if u16::from_ne_bytes(reply[4..6].try_into().unwrap()) == libc::NLMSG_ERROR as u16 {
        return Ok(None);
    }
    if !matches!(reply[23], libc::RTN_UNICAST | libc::RTN_LOCAL) {
        return Ok(None);
    }
    let mut at = 28;
    while at + 4 <= length {
        let size = u16::from_ne_bytes(reply[at..at + 2].try_into().unwrap()) as usize;
        if size < 4 || at + size > length {
            return Err(());
        }
        let kind = u16::from_ne_bytes(reply[at + 2..at + 4].try_into().unwrap());
        if kind == 4 && size >= 8 {
            // RTA_OIF
            let interface = u32::from_ne_bytes(reply[at + 4..at + 8].try_into().unwrap());
            if !interface_active(interface)? {
                return Ok(None);
            }
        }
        at += (size + 3) & !3;
    }
    Ok(Some(reply[17]))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn interface_active(index: u32) -> Result<bool, ()> {
    let mut first = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut first) } != 0 {
        return Err(());
    }
    let mut current = first;
    let mut active = false;
    while !current.is_null() {
        let interface = unsafe { &*current };
        if !interface.ifa_name.is_null()
            && unsafe { libc::if_nametoindex(interface.ifa_name) } == index
        {
            let flags = interface.ifa_flags as i32;
            active = flags & libc::IFF_UP != 0 && flags & libc::IFF_RUNNING != 0;
            break;
        }
        current = interface.ifa_next;
    }
    unsafe {
        libc::freeifaddrs(first);
    }
    Ok(active)
}

#[cfg(target_os = "macos")]
fn route_prefix(ip: IpAddr) -> Result<Option<u8>, ()> {
    let mut mib = [
        libc::CTL_NET,
        libc::PF_ROUTE,
        0,
        if ip.is_ipv4() {
            libc::AF_INET
        } else {
            libc::AF_INET6
        },
        libc::NET_RT_DUMP,
        0,
    ];
    let mut length = 0;
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as _,
            std::ptr::null_mut(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    } != 0
        || length > 4 * 1024 * 1024
    {
        return Err(());
    }
    let mut bytes = vec![0u8; length];
    if unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            mib.len() as _,
            bytes.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(());
    }
    bytes.truncate(length);
    let mut best = None;
    let mut offset = 0;
    while offset + std::mem::size_of::<libc::rt_msghdr>() <= bytes.len() {
        // sysctl returns packed route records; never assume Vec<u8> alignment.
        let header = unsafe {
            std::ptr::read_unaligned(bytes.as_ptr().add(offset).cast::<libc::rt_msghdr>())
        };
        let end = offset + header.rtm_msglen as usize;
        if end > bytes.len() || header.rtm_msglen as usize <= std::mem::size_of::<libc::rt_msghdr>()
        {
            return Err(());
        }
        if header.rtm_version == libc::RTM_VERSION as u8 && header.rtm_flags & libc::RTF_UP != 0 {
            let mut at = offset + std::mem::size_of::<libc::rt_msghdr>();
            let mut destination = None;
            let mut mask = None;
            for index in 0..libc::RTAX_MAX {
                if header.rtm_addrs & (1 << index) == 0 {
                    continue;
                }
                if at + 2 > end {
                    return Err(());
                }
                let size = bytes[at] as usize;
                let padded = if size == 0 { 4 } else { (size + 3) & !3 };
                if at + padded > end {
                    return Err(());
                }
                if index == libc::RTAX_DST {
                    destination = Some(&bytes[at..at + size]);
                }
                if index == libc::RTAX_NETMASK {
                    mask = Some(&bytes[at..at + size]);
                }
                at += padded;
            }
            if let Some(destination) = destination {
                let prefix = bsd_prefix(
                    ip,
                    destination,
                    mask,
                    header.rtm_flags & libc::RTF_HOST != 0,
                );
                if let Some(prefix) = prefix {
                    let usable = header.rtm_flags & (libc::RTF_REJECT | libc::RTF_BLACKHOLE) == 0
                        && interface_active(u32::from(header.rtm_index))?;
                    if best.is_none_or(|(old, _)| prefix >= old) {
                        best = Some((prefix, usable));
                    }
                }
            }
        }
        offset = end;
    }
    Ok(best.and_then(|(prefix, usable)| usable.then_some(prefix)))
}

#[cfg(any(target_os = "macos", test))]
fn bsd_prefix(ip: IpAddr, destination: &[u8], mask: Option<&[u8]>, host: bool) -> Option<u8> {
    let (octets, offset) = match ip {
        IpAddr::V4(ip) => (ip.octets().to_vec(), 4),
        IpAddr::V6(ip) => (ip.octets().to_vec(), 8),
    };
    let mut prefix = 0;
    for (i, byte) in octets.iter().enumerate() {
        let mask = if host {
            255
        } else {
            mask.and_then(|m| m.get(offset + i)).copied().unwrap_or(0)
        };
        if byte & mask != destination.get(offset + i).copied().unwrap_or(0) & mask {
            return None;
        }
        prefix += mask.count_ones() as u8;
    }
    Some(prefix)
}

#[cfg(windows)]
fn route_prefix(ip: IpAddr) -> Result<Option<u8>, ()> {
    // Windows IP Helper ABI (IPv4 and IPv6), without a network request.
    // https://learn.microsoft.com/windows/win32/api/netioapi/nf-netioapi-getbestroute2
    // https://learn.microsoft.com/windows/win32/api/netioapi/ns-netioapi-mib_ipforward_row2
    #[repr(C, align(4))]
    struct SockAddrInet {
        bytes: [u8; 28],
    }
    #[repr(C)]
    struct AddressPrefix {
        address: SockAddrInet,
        length: u8,
    }
    #[repr(C)]
    struct ForwardRow {
        interface_luid: u64,
        interface_index: u32,
        destination: AddressPrefix,
        next_hop: SockAddrInet,
        site_prefix_length: u8,
        valid_lifetime: u32,
        preferred_lifetime: u32,
        metric: u32,
        protocol: u32,
        loopback: u8,
        autoconfigure_address: u8,
        publish: u8,
        immortal: u8,
        age: u32,
        origin: u32,
    }
    #[link(name = "iphlpapi")]
    unsafe extern "system" {
        fn GetBestRoute2(
            luid: *const u64,
            index: u32,
            source: *const SockAddrInet,
            destination: *const SockAddrInet,
            options: u32,
            row: *mut ForwardRow,
            best_source: *mut SockAddrInet,
        ) -> u32;
    }
    let mut destination = SockAddrInet { bytes: [0; 28] };
    match ip {
        IpAddr::V4(ip) => {
            destination.bytes[..2].copy_from_slice(&2u16.to_ne_bytes()); // AF_INET
            destination.bytes[4..8].copy_from_slice(&ip.octets());
        }
        IpAddr::V6(ip) => {
            destination.bytes[..2].copy_from_slice(&23u16.to_ne_bytes()); // AF_INET6
            destination.bytes[8..24].copy_from_slice(&ip.octets());
        }
    }
    let mut source = SockAddrInet { bytes: [0; 28] };
    let mut row: ForwardRow = unsafe { std::mem::zeroed() };
    let status = unsafe {
        GetBestRoute2(
            std::ptr::null(),
            0,
            std::ptr::null(),
            &destination,
            0,
            &mut row,
            &mut source,
        )
    };
    if status != 0 {
        return Ok(None);
    }
    Ok(Some(row.destination.length))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn route_prefix(_: IpAddr) -> Result<Option<u8>, ()> {
    Err(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    #[test]
    fn the_kernel_lookup_is_read_only_and_returns_the_local_host_route() {
        assert!(matches!(
            route_prefix("127.0.0.1".parse().unwrap()).unwrap(),
            Some(1..=32)
        ));
    }
    #[test]
    fn a_default_gateway_does_not_make_an_office_lan_available_at_home() {
        let home = [16, 2, 0, 0, 192, 168, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mask = [7, 0, 0, 0, 255, 255, 255]; // BSD trims trailing zeroes
        assert_eq!(
            bsd_prefix("192.168.1.30".parse().unwrap(), &home, Some(&mask), false),
            Some(24)
        );
        assert_eq!(
            bsd_prefix("192.168.128.30".parse().unwrap(), &home, Some(&mask), false),
            None
        );
        let vpn = [16, 2, 0, 0, 192, 168, 128, 30, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            bsd_prefix("192.168.128.30".parse().unwrap(), &vpn, None, true),
            Some(32)
        );
        assert!(private_network("192.168.128.30".parse().unwrap()));
        assert!(private_network("100.100.255.90".parse().unwrap()));
        assert!(!private_network("8.8.8.8".parse().unwrap()));
        let office = "192.168.128.30".parse().unwrap();
        let home = "192.168.1.30".parse().unwrap();
        // The production selector rejects the office address over an ordinary
        // Internet default, but accepts a specific route installed by a VPN.
        assert_eq!(
            select_address(vec![office, home], |ip| Ok(Some(if ip == home {
                24
            } else {
                0
            })))
            .unwrap(),
            vec![home]
        );
        assert!(select_address(vec![office], |_| Ok(Some(0))).is_err());
        assert_eq!(
            select_address(vec![office], |_| Ok(Some(32))).unwrap(),
            vec![office]
        );
    }
}
