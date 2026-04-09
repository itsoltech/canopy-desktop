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
}

const VIRTUAL_INTERFACE_PATTERN =
  /^(vboxnet|vmnet|docker|br-|lo|utun|tun|tap|tailscale|zerotier|wg|cni|virbr|awdl|llw|anpi|bridge)/i

export function listLanInterfaces(): NetworkInterfaceInfo[] {
  const ifaces = networkInterfaces()
  const result: NetworkInterfaceInfo[] = []
  for (const [name, addrs] of Object.entries(ifaces)) {
    if (!addrs) continue
    if (VIRTUAL_INTERFACE_PATTERN.test(name)) continue
    for (const addr of addrs) {
      if (addr.family === 'IPv4' && !addr.internal) {
        result.push({ name, address: addr.address })
      }
    }
  }
  return result
}

export function selectPrimaryInterface(): NetworkInterfaceInfo | null {
  const ifaces = listLanInterfaces()
  // Prefer interfaces named like WiFi on macOS (`en0`, `en1`) since that's
  // usually where a phone will be. Ethernet has higher numeric priority
  // but phones can't reach it unless they share the subnet. When nothing
  // matches the preferred names, fall back to the first non-virtual
  // interface in the os.networkInterfaces() order.
  const preferred = ifaces.find((i) => /^en\d+$/i.test(i.name))
  return preferred ?? ifaces[0] ?? null
}
