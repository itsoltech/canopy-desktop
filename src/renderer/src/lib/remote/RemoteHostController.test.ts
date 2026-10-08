import { beforeEach, describe, expect, it, vi } from 'vitest'

// The RPC server pulls in the whole workspace store graph; offer handling never reaches it.
vi.mock('./HostRpcServer', () => ({ HostRpcServer: class {} }))

let setRemoteDescription: () => Promise<void> = async () => {}

class FakePeerConnection {
  remoteDescription = null
  onicecandidate = null
  ondatachannel = null
  setRemoteDescription = (): Promise<void> => setRemoteDescription()
  createAnswer = async (): Promise<RTCSessionDescriptionInit> => ({ type: 'answer', sdp: 'v=0' })
  setLocalDescription = async (): Promise<void> => {}
  addIceCandidate = async (): Promise<void> => {}
  close = (): void => {}
}
vi.stubGlobal('RTCPeerConnection', FakePeerConnection)

import { RemoteHostController } from './RemoteHostController'

const offer = { type: 'offer', sdp: { type: 'offer', sdp: 'v=0' } }

describe('RemoteHostController offer handling', () => {
  beforeEach(() => {
    vi.spyOn(console, 'warn').mockImplementation(() => {})
  })

  it('answers a valid offer', async () => {
    setRemoteDescription = async () => {}
    const send = vi.fn()
    await new RemoteHostController(send).handleSignal(offer)
    expect(send).toHaveBeenCalledWith({ type: 'answer', sdp: { type: 'answer', sdp: 'v=0' } })
  })

  it('ends the attempt when the offer cannot be answered, instead of leaving the peer waiting', async () => {
    setRemoteDescription = () => Promise.reject(new Error('Failed to parse SessionDescription'))
    const send = vi.fn()
    await expect(new RemoteHostController(send).handleSignal(offer)).resolves.toBeUndefined()
    expect(send).toHaveBeenCalledWith({ type: 'bye', reason: 'offer rejected' })
  })

  it('stays silent when a newer offer already replaced this controller', async () => {
    let rejectPending!: (e: Error) => void
    setRemoteDescription = () => new Promise((_, reject) => (rejectPending = reject))
    const send = vi.fn()
    const ctl = new RemoteHostController(send)
    const handling = ctl.handleSignal(offer)
    ctl.dispose()
    rejectPending(new Error('InvalidStateError: the peer connection is closed'))
    await handling
    expect(send).not.toHaveBeenCalled()
  })
})
