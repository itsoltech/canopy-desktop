import { networkInterfaces } from 'node:os'

/**
 * Detect LAN-facing IPv4 interfaces so the QR code can advertise a host that
 * a phone on the same WiFi can actually reach.
 *
 * `127.0.0.1` is unusable because the remote peer is not on the same loopback,
 * and tunnel/virtualization interfaces (`vmnet`, `docker`, `tailscale`, …) are
 * filtered out because they advertise addresses that aren't reachable from the
 * phone's WiFi side. The remaining list is what real Ethernet/WiFi adapters
 * report.
 */

export interface NetworkInterfaceInfo {
  name: string
  address: string
  virtual: boolean
}

const VIRTUAL_INTERFACE_PATTERN =
  /^(vboxnet|vmnet|docker|br-|lo|utun|tun|tap|tailscale|zerotier|wg|cni|virbr|awdl|llw|anpi|bridge)/i

function isLinkLocalIPv4(address: string): boolean {
  return address.startsWith('169.254.')
}

export function listAllInterfaces(): NetworkInterfaceInfo[] {
  const ifaces = networkInterfaces()
  const result: NetworkInterfaceInfo[] = []
  for (const [name, addrs] of Object.entries(ifaces)) {
    if (!addrs) continue
    const virtual = VIRTUAL_INTERFACE_PATTERN.test(name)
    for (const addr of addrs) {
      if (addr.family === 'IPv4' && !addr.internal) {
        result.push({ name, address: addr.address, virtual })
      }
    }
  }
  return result
}

export function listSelectableInterfaces(): NetworkInterfaceInfo[] {
  const byName = new Map<string, NetworkInterfaceInfo>()
  for (const iface of listAllInterfaces()) {
    if (isLinkLocalIPv4(iface.address)) continue
    if (!byName.has(iface.name)) {
      byName.set(iface.name, iface)
    }
  }
  return [...byName.values()]
}

export function selectPrimaryInterface(preferredName?: string): NetworkInterfaceInfo | null {
  // Remote QR codes must advertise a concrete address that the peer can reach.
  // There is no safe auto-detect fallback here: the OS interface order may pick
  // an adapter that is not on the phone's network.
  if (!preferredName) return null
  // Explicit user choice: match by interface name across ALL selectable
  // interfaces (including virtual ones like Tailscale/WireGuard — the user
  // opted in). No match means the named interface is gone (DHCP renew, adapter
  // unplug, VPN down). Caller treats null as NoNetworkInterface.
  return listSelectableInterfaces().find((i) => i.name === preferredName) ?? null
}
