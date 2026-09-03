import { describe, expect, it } from 'vitest'
import {
  RemoteBackgroundExecutionController,
  remoteSessionNeedsBackgroundExecution,
  type BackgroundThrottlingTarget,
  type PowerSaveBlockerLike,
} from './RemoteBackgroundExecutionController'
import type { RemoteSessionStatus } from './types'

class FakePowerSaveBlocker implements PowerSaveBlockerLike {
  starts: Array<'prevent-app-suspension'> = []
  stops: number[] = []
  private active = new Set<number>()

  start(type: 'prevent-app-suspension'): number {
    const id = this.starts.length + 1
    this.starts.push(type)
    this.active.add(id)
    return id
  }

  stop(id: number): boolean {
    this.stops.push(id)
    return this.active.delete(id)
  }

  isStarted(id: number): boolean {
    return this.active.has(id)
  }
}

class FakeWebContents implements BackgroundThrottlingTarget {
  calls: boolean[] = []

  constructor(private backgroundThrottling = true) {}

  isDestroyed(): boolean {
    return false
  }

  getBackgroundThrottling(): boolean {
    return this.backgroundThrottling
  }

  setBackgroundThrottling(allowed: boolean): void {
    this.calls.push(allowed)
    this.backgroundThrottling = allowed
  }
}

const activeStatuses: RemoteSessionStatus[] = [
  { kind: 'starting' },
  { kind: 'listening', hostname: 'mac', lanIp: '100.64.0.1', port: 1234 },
  {
    kind: 'waiting',
    pairingUrl: 'http://100.64.0.1:1234/remote/',
    hostname: 'mac',
    lanIp: '100.64.0.1',
    port: 1234,
    expiresAt: 1,
  },
  {
    kind: 'peerArrived',
    pairingUrl: 'http://100.64.0.1:1234/remote/',
    hostname: 'mac',
    lanIp: '100.64.0.1',
    port: 1234,
    device: { deviceId: 'device', deviceName: 'Phone', fingerprint: 'device' },
  },
  {
    kind: 'paired',
    hostname: 'mac',
    lanIp: '100.64.0.1',
    port: 1234,
    deviceName: 'Phone',
    connectedAt: 1,
  },
  {
    kind: 'reconnecting',
    hostname: 'mac',
    lanIp: '100.64.0.1',
    port: 1234,
    deviceName: 'Phone',
    reconnectingSince: 1,
  },
]

describe('remoteSessionNeedsBackgroundExecution', () => {
  it('keeps every running remote-control phase active', () => {
    for (const status of activeStatuses) {
      expect(remoteSessionNeedsBackgroundExecution(status)).toBe(true)
    }
  })

  it('allows normal suspension when remote control is inactive', () => {
    expect(remoteSessionNeedsBackgroundExecution({ kind: 'idle' })).toBe(false)
    expect(remoteSessionNeedsBackgroundExecution({ kind: 'error', message: 'failed' })).toBe(false)
  })
})

describe('RemoteBackgroundExecutionController', () => {
  it('holds one suspension blocker for the active lifecycle', () => {
    const blocker = new FakePowerSaveBlocker()
    const controller = new RemoteBackgroundExecutionController({
      powerSaveBlocker: blocker,
      findWebContents: () => null,
    })

    controller.sync(activeStatuses[0], null)
    controller.sync(activeStatuses[1], null)
    controller.sync({ kind: 'idle' }, null)

    expect(blocker.starts).toEqual(['prevent-app-suspension'])
    expect(blocker.stops).toEqual([1])
  })

  it('unthrottles only the current host renderer and restores it afterward', () => {
    const blocker = new FakePowerSaveBlocker()
    const first = new FakeWebContents()
    const second = new FakeWebContents()
    const targets = new Map<number, FakeWebContents>([
      [1, first],
      [2, second],
    ])
    const controller = new RemoteBackgroundExecutionController({
      powerSaveBlocker: blocker,
      findWebContents: (id) => targets.get(id) ?? null,
    })

    controller.sync(activeStatuses[0], 1)
    controller.sync(activeStatuses[1], 2)
    controller.dispose()

    expect(first.calls).toEqual([false, true])
    expect(second.calls).toEqual([false, true])
    expect(blocker.stops).toEqual([1])
  })

  it('preserves an existing request to keep the renderer unthrottled', () => {
    const blocker = new FakePowerSaveBlocker()
    const target = new FakeWebContents(false)
    const controller = new RemoteBackgroundExecutionController({
      powerSaveBlocker: blocker,
      findWebContents: () => target,
    })

    controller.sync(activeStatuses[0], 1)
    controller.dispose()

    expect(target.calls).toEqual([false, false])
  })
})
