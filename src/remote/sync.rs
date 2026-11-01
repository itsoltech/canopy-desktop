//! Bounded replay and independent subscriber queues. Payloads are already encoded
//! protocol messages, not executable commands. A snapshot is taken by the state
//! owner at the returned boundary, after the tail subscription is registered.
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU8, AtomicUsize, Ordering},
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cursor {
    epoch: String,
    stream: String,
    sequence: u64,
}
impl Cursor {
    pub fn epoch(&self) -> &str {
        &self.epoch
    }
    pub fn stream(&self) -> &str {
        &self.stream
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn from_parts(epoch: String, stream: String, sequence: u64) -> Self {
        Self {
            epoch,
            stream,
            sequence,
        }
    }
}

#[derive(Clone, Copy)]
pub struct Limits {
    frame: usize,
    ring: usize,
    device: usize,
    global: usize,
    subscriptions: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            frame: 1024 * 1024,
            ring: 16 * 1024 * 1024,
            device: 4 * 1024 * 1024,
            global: 32 * 1024 * 1024,
            subscriptions: 256,
        }
    }
}
impl Limits {
    pub fn new(
        frame: usize,
        ring: usize,
        device: usize,
        global: usize,
        subscriptions: usize,
    ) -> Option<Self> {
        (frame > 0 && ring >= frame && device >= frame && global >= device && subscriptions > 0)
            .then_some(Self {
                frame,
                ring,
                device,
                global,
                subscriptions,
            })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    UnknownStream,
    FrameTooLarge,
    ResourceExhausted,
    SequenceExhausted,
    ResyncRequired,
    Revoked,
    Closed,
}

struct Frame {
    cursor: Cursor,
    payload: Arc<[u8]>,
}
pub struct ByteReservation {
    global: Arc<AtomicUsize>,
    device: Arc<AtomicUsize>,
    bytes: usize,
}
impl ByteReservation {
    fn reserve(
        global: &Arc<AtomicUsize>,
        device: &Arc<AtomicUsize>,
        bytes: usize,
        limits: Limits,
    ) -> Option<Self> {
        device
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                v.checked_add(bytes).filter(|v| *v <= limits.device)
            })
            .ok()?;
        if global
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| {
                v.checked_add(bytes).filter(|v| *v <= limits.global)
            })
            .is_err()
        {
            device.fetch_sub(bytes, Ordering::AcqRel);
            return None;
        }
        Some(Self {
            global: global.clone(),
            device: device.clone(),
            bytes,
        })
    }
    /// Transfer part of an admitted workspace to its retained projection.
    pub(crate) fn split(&mut self, bytes: usize) -> Self {
        assert!(bytes <= self.bytes);
        self.bytes -= bytes;
        Self {
            global: self.global.clone(),
            device: self.device.clone(),
            bytes,
        }
    }
    /// Release temporary workspace after conversion, without a second admission.
    pub(crate) fn shrink_to(&mut self, bytes: usize) {
        assert!(bytes <= self.bytes);
        let released = self.bytes - bytes;
        self.bytes = bytes;
        self.global.fetch_sub(released, Ordering::AcqRel);
        self.device.fetch_sub(released, Ordering::AcqRel);
    }
}
impl Drop for ByteReservation {
    fn drop(&mut self) {
        self.global.fetch_sub(self.bytes, Ordering::AcqRel);
        self.device.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}
pub struct Delivery {
    frame: Arc<Frame>,
    _budget: ByteReservation,
}
impl Delivery {
    pub fn cursor(&self) -> &Cursor {
        &self.frame.cursor
    }
    pub fn payload(&self) -> &[u8] {
        &self.frame.payload
    }
}
struct Listener {
    stream: String,
    device: String,
    sender: async_channel::Sender<Delivery>,
    drain: async_channel::Receiver<Delivery>,
    failed: Arc<AtomicU8>,
    budget: Arc<AtomicUsize>,
}
impl Listener {
    fn close(&self, reason: u8) {
        self.failed.store(reason, Ordering::Release);
        self.sender.close();
        // Closed receivers can still contain data. Free it immediately and require
        // explicit resync, rather than retaining a slow client's entire backlog.
        while self.drain.try_recv().is_ok() {}
    }
}
#[derive(Default)]
struct Stream {
    head: u64,
    evicted: u64,
    closed: bool,
}
struct State {
    streams: HashMap<String, Stream>,
    ring: VecDeque<Arc<Frame>>,
    ring_bytes: usize,
    listeners: HashMap<u64, Listener>,
    budgets: HashMap<String, Arc<AtomicUsize>>,
    next: u64,
}
struct Inner {
    epoch: String,
    limits: Limits,
    state: Mutex<State>,
    queued: Arc<AtomicUsize>,
    assemblies: Arc<AtomicUsize>,
}
#[derive(Clone)]
pub struct ReplayHub {
    inner: Arc<Inner>,
}

pub enum Start {
    /// Replay is complete through `boundary`; `Subscription` holds the registered tail.
    Replay {
        boundary: Cursor,
        events: Vec<Delivery>,
    },
    /// The owner must snapshot at this boundary, then drain the registered tail.
    Snapshot { boundary: Cursor },
}
pub struct Subscription {
    id: u64,
    device: String,
    receiver: async_channel::Receiver<Delivery>,
    failed: Arc<AtomicU8>,
    owner: Weak<Inner>,
}
impl Subscription {
    pub async fn recv(&self) -> Result<Delivery, Error> {
        self.check()?;
        let event = self
            .receiver
            .recv()
            .await
            .map_err(|_| self.check().err().unwrap_or(Error::Closed))?;
        self.check()?;
        Ok(event)
    }
    pub fn try_recv(&self) -> Result<Option<Delivery>, Error> {
        self.check()?;
        match self.receiver.try_recv() {
            Ok(event) => {
                self.check()?;
                Ok(Some(event))
            }
            Err(async_channel::TryRecvError::Empty) => Ok(None),
            Err(async_channel::TryRecvError::Closed) => {
                Err(self.check().err().unwrap_or(Error::Closed))
            }
        }
    }
    pub fn check(&self) -> Result<(), Error> {
        match self.failed.load(Ordering::Acquire) {
            0 => Ok(()),
            1 => Err(Error::ResyncRequired),
            2 => Err(Error::Revoked),
            _ => Err(Error::Closed),
        }
    }
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(owner) = self.owner.upgrade() {
            let mut state = owner.state.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(listener) = state.listeners.remove(&self.id) {
                listener.close(3);
            }
            if !state.listeners.values().any(|l| l.device == self.device)
                && state
                    .budgets
                    .get(&self.device)
                    .is_none_or(|budget| Arc::strong_count(budget) == 1)
            {
                state.budgets.remove(&self.device);
            }
        }
    }
}

impl ReplayHub {
    pub fn epoch(&self) -> &str {
        &self.inner.epoch
    }
    pub fn new(limits: Limits) -> Self {
        Self {
            inner: Arc::new(Inner {
                epoch: uuid::Uuid::new_v4().to_string(),
                limits,
                state: Mutex::new(State {
                    streams: HashMap::new(),
                    ring: VecDeque::new(),
                    ring_bytes: 0,
                    listeners: HashMap::new(),
                    budgets: HashMap::new(),
                    next: 1,
                }),
                queued: Arc::new(AtomicUsize::new(0)),
                assemblies: Arc::new(AtomicUsize::new(0)),
            }),
        }
    }
    /// Non-replayed terminal output shares the same device/global queue budget
    /// as transcript fanout. Hold the reservation until the transport polls it.
    pub fn reserve_output(&self, device: &str, bytes: usize) -> Result<ByteReservation, Error> {
        if device.is_empty() || device.len() > 256 {
            return Err(Error::ResourceExhausted);
        }
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state
            .budgets
            .retain(|_, budget| Arc::strong_count(budget) > 1);
        let budget = state.budgets.entry(device.to_owned()).or_default().clone();
        ByteReservation::reserve(&self.inner.queued, &budget, bytes, self.inner.limits)
            .ok_or(Error::ResourceExhausted)
    }
    /// Terminal material has a separate bounded workspace: raw projection,
    /// retained baseline, conversion and encoding are admitted before allocation.
    /// Charging it to the 4 MiB wire queue would prevent fragmented delivery.
    /// At most 32 MiB is retained across the host (and therefore per device).
    pub fn reserve_assembly(&self, bytes: usize) -> Result<ByteReservation, Error> {
        let limit = 32 * 1024 * 1024;
        if bytes > limit {
            return Err(Error::ResourceExhausted);
        }
        ByteReservation::reserve(
            &self.inner.assemblies,
            &Arc::default(),
            bytes,
            Limits {
                device: limit,
                global: limit,
                ..self.inner.limits
            },
        )
        .ok_or(Error::ResourceExhausted)
    }
    /// Called by the state owner after resource creation, never for arbitrary client IDs.
    pub fn open_stream(&self, id: &str) -> Result<Cursor, Error> {
        if id.is_empty() || id.len() > 256 {
            return Err(Error::UnknownStream);
        }
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        let stream = state.streams.entry(id.to_owned()).or_default();
        if stream.closed {
            return Err(Error::Closed);
        }
        Ok(Cursor {
            epoch: self.inner.epoch.clone(),
            stream: id.to_owned(),
            sequence: stream.head,
        })
    }
    pub fn publish(&self, id: &str, payload: Vec<u8>) -> Result<Cursor, Error> {
        if payload.len() > self.inner.limits.frame {
            return Err(Error::FrameTooLarge);
        }
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        let stream = state.streams.get_mut(id).ok_or(Error::UnknownStream)?;
        if stream.closed {
            return Err(Error::Closed);
        }
        stream.head = stream.head.checked_add(1).ok_or(Error::SequenceExhausted)?;
        let cursor = Cursor {
            epoch: self.inner.epoch.clone(),
            stream: id.to_owned(),
            sequence: stream.head,
        };
        let frame = Arc::new(Frame {
            cursor: cursor.clone(),
            payload: payload.into(),
        });
        let bytes = retained_bytes(&frame);
        state.ring_bytes += bytes;
        state.ring.push_back(frame.clone());
        while state.ring_bytes > self.inner.limits.ring {
            let Some(old) = state.ring.pop_front() else {
                break;
            };
            state.ring_bytes -= retained_bytes(&old);
            if let Some(stream) = state.streams.get_mut(&old.cursor.stream) {
                stream.evicted = old.cursor.sequence;
            }
        }
        let mut failed = Vec::new();
        for (&key, listener) in state.listeners.iter().filter(|(_, l)| l.stream == id) {
            let Some(budget) = ByteReservation::reserve(
                &self.inner.queued,
                &listener.budget,
                bytes,
                self.inner.limits,
            ) else {
                listener.close(1);
                failed.push(key);
                continue;
            };
            if listener
                .sender
                .try_send(Delivery {
                    frame: frame.clone(),
                    _budget: budget,
                })
                .is_err()
            {
                listener.close(1);
                failed.push(key);
            }
        }
        for key in failed {
            state.listeners.remove(&key);
        }
        Ok(cursor)
    }
    /// Authorization and existence checks belong to the caller. Registration and
    /// replay boundary are atomic with publish; taking a snapshot first is unsafe.
    pub fn subscribe(
        &self,
        id: &str,
        device: &str,
        since: Option<&Cursor>,
    ) -> Result<(Start, Subscription), Error> {
        if device.is_empty() || device.len() > 256 {
            return Err(Error::ResourceExhausted);
        }
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.listeners.len() >= self.inner.limits.subscriptions {
            return Err(Error::ResourceExhausted);
        }
        let stream = state.streams.get(id).ok_or(Error::UnknownStream)?;
        if stream.closed {
            return Err(Error::Closed);
        }
        let boundary = Cursor {
            epoch: self.inner.epoch.clone(),
            stream: id.to_owned(),
            sequence: stream.head,
        };
        let replay = since
            .filter(|c| {
                c.epoch == self.inner.epoch
                    && c.stream == id
                    && c.sequence >= stream.evicted
                    && c.sequence <= stream.head
            })
            .map(|c| {
                state
                    .ring
                    .iter()
                    .filter(|f| f.cursor.stream == id && f.cursor.sequence > c.sequence)
                    .cloned()
                    .collect::<Vec<_>>()
            });
        state
            .budgets
            .retain(|_, budget| Arc::strong_count(budget) > 1);
        let budget = state
            .budgets
            .entry(device.to_owned())
            .or_insert_with(|| Arc::new(AtomicUsize::new(0)))
            .clone();
        let mut deliveries = Vec::new();
        let mut replay_ok = replay.is_some();
        if let Some(frames) = replay {
            for frame in frames {
                if let Some(charge) = ByteReservation::reserve(
                    &self.inner.queued,
                    &budget,
                    retained_bytes(&frame),
                    self.inner.limits,
                ) {
                    deliveries.push(Delivery {
                        frame,
                        _budget: charge,
                    });
                } else {
                    replay_ok = false;
                    deliveries.clear();
                    break;
                }
            }
        }
        let start = if replay_ok {
            Start::Replay {
                boundary,
                events: deliveries,
            }
        } else {
            Start::Snapshot { boundary }
        };
        let (sender, receiver) = async_channel::bounded(256);
        let failed = Arc::new(AtomicU8::new(0));
        let key = state.next;
        state.next = state.next.checked_add(1).ok_or(Error::SequenceExhausted)?;
        state.listeners.insert(
            key,
            Listener {
                stream: id.to_owned(),
                device: device.to_owned(),
                sender,
                drain: receiver.clone(),
                failed: failed.clone(),
                budget,
            },
        );
        Ok((
            start,
            Subscription {
                id: key,
                device: device.to_owned(),
                receiver,
                failed,
                owner: Arc::downgrade(&self.inner),
            },
        ))
    }
    pub fn revoke_device(&self, device: &str) {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.listeners.retain(|_, listener| {
            if listener.device == device {
                listener.close(2);
                false
            } else {
                true
            }
        });
        if state
            .budgets
            .get(device)
            .is_none_or(|budget| Arc::strong_count(budget) == 1)
        {
            state.budgets.remove(device);
        }
    }
    pub fn close_stream(&self, id: &str) {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.listeners.retain(|_, listener| {
            if listener.stream == id {
                listener.close(3);
                false
            } else {
                true
            }
        });
        // Resource IDs never become a fresh stream in the same runtime epoch.
        // Retain its boundary so a stale cursor cannot attach to another lifetime.
        if let Some(stream) = state.streams.get_mut(id) {
            stream.closed = true;
        }
        let mut freed = 0;
        state.ring.retain(|frame| {
            if frame.cursor.stream == id {
                freed += retained_bytes(frame);
                false
            } else {
                true
            }
        });
        state.ring_bytes -= freed;
    }
    pub fn queued_bytes(&self) -> usize {
        self.inner.queued.load(Ordering::Acquire)
    }
    pub fn ring_bytes(&self) -> usize {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .ring_bytes
    }
}
fn retained_bytes(frame: &Frame) -> usize {
    frame.payload.len() + frame.cursor.epoch.len() + frame.cursor.stream.len() + 64
}

#[cfg(test)]
mod tests {
    use super::*;
    fn new_hub() -> ReplayHub {
        ReplayHub::new(Limits::new(256, 2048, 512, 4096, 16).unwrap())
    }

    #[test]
    fn snapshot_boundary_registers_tail_before_the_next_update() {
        let hub = new_hub();
        hub.open_stream("chat").unwrap();
        let first = hub.publish("chat", b"first".to_vec()).unwrap();
        let (start, client) = hub.subscribe("chat", "phone", None).unwrap();
        assert!(matches!(start,Start::Snapshot{boundary} if boundary==first));
        let after = hub
            .publish("chat", b"after snapshot boundary".to_vec())
            .unwrap();
        let tail = client.try_recv().unwrap().unwrap();
        assert_eq!(tail.cursor(), &after);
        let (_, other) = hub.subscribe("chat", "desktop", Some(&first)).unwrap();
        // Registration does not consume the first subscriber's tail.
        assert!(other.try_recv().unwrap().is_none());
    }
    #[test]
    fn reconnect_replays_exactly_the_missing_events_or_requires_a_snapshot() {
        let hub = new_hub();
        let initial = hub.open_stream("chat").unwrap();
        let first = hub.publish("chat", vec![1; 100]).unwrap();
        hub.publish("chat", vec![2; 100]).unwrap();
        let (start, _) = hub.subscribe("chat", "phone", Some(&first)).unwrap();
        match start {
            Start::Replay { events, .. } => {
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].payload(), vec![2; 100]);
            }
            _ => panic!("replay"),
        }
        for _ in 0..30 {
            hub.publish("chat", vec![3; 100]).unwrap();
        }
        assert!(hub.ring_bytes() <= 2048);
        assert!(matches!(
            hub.subscribe("chat", "phone", Some(&initial)).unwrap().0,
            Start::Snapshot { .. }
        ));
        let restarted = new_hub();
        restarted.open_stream("chat").unwrap();
        assert!(matches!(
            restarted
                .subscribe("chat", "phone", Some(&first))
                .unwrap()
                .0,
            Start::Snapshot { .. }
        ));
    }
    #[test]
    fn a_slow_client_resyncs_without_blocking_other_clients_and_frees_its_queue() {
        let hub = new_hub();
        hub.open_stream("chat").unwrap();
        let (_, slow) = hub.subscribe("chat", "slow", None).unwrap();
        let (_, fast) = hub.subscribe("chat", "fast", None).unwrap();
        for n in 0..20 {
            hub.publish("chat", vec![n; 100]).unwrap();
            let event = fast.try_recv().unwrap().unwrap();
            assert_eq!(event.payload(), vec![n; 100]);
        }
        assert!(matches!(slow.try_recv(), Err(Error::ResyncRequired)));
        assert_eq!(hub.queued_bytes(), 0);
        let (_, fresh) = hub.subscribe("chat", "slow", None).unwrap();
        hub.publish("chat", b"recovered".to_vec()).unwrap();
        assert_eq!(fresh.try_recv().unwrap().unwrap().payload(), b"recovered");
    }
    #[test]
    fn device_budget_is_shared_across_subscriptions_and_revoke_discards_pending_data() {
        let hub = new_hub();
        hub.open_stream("a").unwrap();
        hub.open_stream("b").unwrap();
        let (_, a) = hub.subscribe("a", "phone", None).unwrap();
        let (_, b) = hub.subscribe("b", "phone", None).unwrap();
        hub.publish("a", vec![1; 100]).unwrap();
        hub.publish("b", vec![2; 100]).unwrap();
        assert!(hub.queued_bytes() <= 512);
        hub.revoke_device("phone");
        assert!(matches!(a.try_recv(), Err(Error::Revoked)));
        assert!(matches!(b.try_recv(), Err(Error::Revoked)));
        assert_eq!(hub.queued_bytes(), 0);
    }
    #[test]
    fn external_terminal_charges_survive_subscription_drop_revoke_and_reentry() {
        let hub = new_hub();
        hub.open_stream("chat").unwrap();
        let terminal = hub.reserve_output("phone", 300).unwrap();
        let (_, subscription) = hub.subscribe("chat", "phone", None).unwrap();
        hub.publish("chat", vec![1; 100]).unwrap();
        let delivery = subscription.try_recv().unwrap().unwrap();
        drop(subscription);
        hub.revoke_device("phone");
        // Both the externally held terminal event and polled chat delivery must
        // still share the same device counter after its last listener disappears.
        assert!(matches!(
            hub.reserve_output("phone", 100),
            Err(Error::ResourceExhausted)
        ));
        let (_, fresh) = hub.subscribe("chat", "phone", None).unwrap();
        hub.publish("chat", vec![2; 100]).unwrap();
        assert_eq!(fresh.check(), Err(Error::ResyncRequired));
        drop(delivery);
        let remaining = hub.reserve_output("phone", 212).unwrap();
        assert!(matches!(
            hub.reserve_output("phone", 1),
            Err(Error::ResourceExhausted)
        ));
        drop((terminal, remaining, fresh));
        assert_eq!(hub.queued_bytes(), 0);
        // Repeated teardown cannot leave stale counters or grow accounting keys.
        for n in 0..100 {
            let charge = hub.reserve_output(&format!("retired-{n}"), 512).unwrap();
            hub.revoke_device(&format!("retired-{n}"));
            drop(charge);
        }
        let charge = hub.reserve_output("phone", 512).unwrap();
        assert!(hub.inner.state.lock().unwrap().budgets.len() <= 1);
        drop(charge);
        assert_eq!(hub.queued_bytes(), 0);
    }

    #[test]
    fn global_rejection_rolls_back_device_charge_and_material_ownership_transfers() {
        let hub = new_hub();
        let mut held = Vec::new();
        for n in 0..8 {
            held.push(hub.reserve_output(&format!("device-{n}"), 512).unwrap());
        }
        assert!(matches!(
            hub.reserve_output("other", 512),
            Err(Error::ResourceExhausted)
        ));
        held.pop();
        let recovered = hub.reserve_output("other", 512).unwrap();
        assert_eq!(hub.queued_bytes(), 4096);
        drop((held, recovered));
        assert_eq!(hub.queued_bytes(), 0);
        let mut material = hub.reserve_assembly(32 * 1024 * 1024).unwrap();
        let installed = material.split(1024);
        material.shrink_to(2048);
        let rest = hub.reserve_assembly(32 * 1024 * 1024 - 3072).unwrap();
        assert!(hub.reserve_assembly(1).is_err());
        drop((material, installed, rest));
        assert!(hub.reserve_assembly(32 * 1024 * 1024).is_ok());
    }

    #[test]
    fn closed_resource_cannot_reuse_a_cursor_in_the_same_runtime() {
        let hub = new_hub();
        let cursor = hub.open_stream("chat").unwrap();
        let (_, client) = hub.subscribe("chat", "phone", None).unwrap();
        hub.close_stream("chat");
        assert!(matches!(client.try_recv(), Err(Error::Closed)));
        assert_eq!(hub.open_stream("chat"), Err(Error::Closed));
        assert_eq!(hub.publish("chat", vec![]), Err(Error::Closed));
        assert!(matches!(
            hub.subscribe("chat", "phone", Some(&cursor)),
            Err(Error::Closed)
        ));
    }
}
