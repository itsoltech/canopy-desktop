//! One run of a terminal process and the single owner of its input and resize.
//! The local view and remote devices call the same checks: a holder writes only
//! with the run's current fencing token; observers never write. A lease is
//! explicit: focus or a free lease does not take input from another holder.
use super::session::{InputRefused, Session, Size};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

/// A device lease ends this long after its last renewal (or at disconnect).
pub const LEASE_TTL: Duration = Duration::from_secs(15);
/// Owners renew at least this often.
pub const RENEW_INTERVAL: Duration = Duration::from_secs(5);
/// Input IDs remembered per run, so a retried ID never writes twice.
const RECENT_INPUTS: usize = 256;

/// Who may write input and resize. The host's own view never expires.
#[derive(Clone, Debug)]
pub enum Holder {
    Local,
    Device { id: String, name: String },
}
impl PartialEq for Holder {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Local, Self::Local) => true,
            (Self::Device { id: a, .. }, Self::Device { id: b, .. }) => a == b,
            _ => false,
        }
    }
}
impl Eq for Holder {}
impl Holder {
    pub fn device(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self::Device {
            id: id.into(),
            name: name.into(),
        }
    }
    pub fn device_id(&self) -> Option<&str> {
        match self {
            Self::Device { id, .. } => Some(id),
            Self::Local => None,
        }
    }
    pub fn label(&self) -> &str {
        match self {
            Self::Local => "this computer",
            Self::Device { name, .. } => name,
        }
    }
}

/// The lease as one reader sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lease {
    holder: Option<Holder>,
    token: u64,
    expires_in: Option<Duration>,
}
impl Lease {
    pub fn holder(&self) -> Option<&Holder> {
        self.holder.as_ref()
    }
    pub fn held_by(&self, holder: &Holder) -> bool {
        self.holder.as_ref() == Some(holder)
    }
    /// Fencing token of the current holder; increases with every grant.
    pub fn token(&self) -> u64 {
        self.token
    }
    /// None for the local holder or a free lease.
    pub fn expires_in(&self) -> Option<Duration> {
        self.expires_in
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The caller does not hold the lease (an observer, or it expired).
    NotHolder,
    /// A token from an earlier grant; nothing was written.
    StaleToken,
    /// Another holder has it; taking over must be explicit.
    HeldBy(Holder),
    InvalidSize,
    Retired,
    Input(InputRefused),
}
impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotHolder => f.write_str("This client does not hold terminal input."),
            Self::StaleToken => f.write_str("Terminal input moved to another owner."),
            Self::HeldBy(holder) => write!(f, "Terminal input is held by {}.", holder.label()),
            Self::InvalidSize => f.write_str("The terminal size is outside the supported range."),
            Self::Retired => f.write_str("This terminal run has ended or restarted."),
            Self::Input(refused) => refused.fmt(f),
        }
    }
}

/// Device, fencing token and input ID of one write.
type InputKey = (String, u64, u64);
/// The lease state machine. Pure: time and the write are supplied by `Run`.
struct Leases {
    holder: Option<Holder>,
    token: u64,
    renewed: Instant,
    /// Open session streams per device; the last one closing is a confirmed disconnect.
    streams: HashMap<String, usize>,
    recent: VecDeque<(InputKey, Result<(), Refused>)>,
}
impl Leases {
    fn new(holder: Option<Holder>, now: Instant) -> Self {
        Self {
            token: u64::from(holder.is_some()),
            holder,
            renewed: now,
            streams: HashMap::new(),
            recent: VecDeque::new(),
        }
    }
    fn expire(&mut self, now: Instant) -> bool {
        let expired = matches!(self.holder, Some(Holder::Device { .. }))
            && now.saturating_duration_since(self.renewed) >= LEASE_TTL;
        if expired {
            self.holder = None;
        }
        expired
    }
    fn view(&self, now: Instant) -> Lease {
        Lease {
            holder: self.holder.clone(),
            token: self.token,
            expires_in: matches!(self.holder, Some(Holder::Device { .. }))
                .then(|| LEASE_TTL.saturating_sub(now.saturating_duration_since(self.renewed))),
        }
    }
    /// Returns the lease and whether it changed hands.
    fn acquire(
        &mut self,
        holder: Holder,
        take_over: bool,
        now: Instant,
    ) -> Result<(Lease, bool), Refused> {
        match &self.holder {
            Some(current) if *current == holder => {
                self.renewed = now;
                Ok((self.view(now), false))
            }
            Some(current) if !take_over => Err(Refused::HeldBy(current.clone())),
            _ => {
                self.token += 1;
                self.holder = Some(holder);
                self.renewed = now;
                Ok((self.view(now), true))
            }
        }
    }
    fn check(&self, holder: &Holder, token: u64) -> Result<(), Refused> {
        match &self.holder {
            Some(current) if current == holder && self.token == token => Ok(()),
            Some(current) if current == holder => Err(Refused::StaleToken),
            Some(_) if self.token != token => Err(Refused::StaleToken),
            _ => Err(Refused::NotHolder),
        }
    }
    fn release(&mut self, holder: &Holder, token: u64, now: Instant) -> Result<Lease, Refused> {
        self.check(holder, token)?;
        self.holder = None;
        Ok(self.view(now))
    }
    fn renew(&mut self, holder: &Holder, token: u64, now: Instant) -> Result<Lease, Refused> {
        self.check(holder, token)?;
        self.renewed = now;
        Ok(self.view(now))
    }
    /// A repeated `input_id` of the same holder and token returns the first outcome.
    fn input(
        &mut self,
        holder: &Holder,
        token: u64,
        input_id: u64,
        now: Instant,
        write: impl FnOnce() -> Result<(), InputRefused>,
    ) -> Result<(), Refused> {
        let key = (
            holder.device_id().unwrap_or_default().to_owned(),
            token,
            input_id,
        );
        if let Some((_, outcome)) = self.recent.iter().find(|(seen, _)| *seen == key) {
            return outcome.clone();
        }
        self.check(holder, token)?;
        let outcome = write().map_err(Refused::Input);
        if self.recent.len() == RECENT_INPUTS {
            self.recent.pop_front();
        }
        self.recent.push_back((key, outcome.clone()));
        if outcome.is_ok() {
            self.renewed = now;
        }
        outcome
    }
    /// Returns true when the device's last stream released its lease.
    fn detach(&mut self, device: &str) -> bool {
        let last = match self.streams.get_mut(device) {
            Some(count) if *count > 1 => {
                *count -= 1;
                false
            }
            _ => {
                self.streams.remove(device);
                true
            }
        };
        let holds = self.holder.as_ref().and_then(Holder::device_id) == Some(device);
        if last && holds {
            self.holder = None;
        }
        last && holds
    }
}

pub struct Run {
    id: String,
    session: Arc<Session>,
    leases: Mutex<Leases>,
    changes: tokio::sync::watch::Sender<u64>,
    retired: AtomicBool,
    superseded: AtomicBool,
}
impl Run {
    /// A new run identity is never reused, also after Restart of the same terminal.
    pub fn new(session: Arc<Session>, holder: Option<Holder>) -> Arc<Self> {
        Arc::new(Self {
            id: uuid::Uuid::new_v4().to_string(),
            session,
            leases: Mutex::new(Leases::new(holder, Instant::now())),
            changes: tokio::sync::watch::channel(0).0,
            retired: AtomicBool::new(false),
            superseded: AtomicBool::new(false),
        })
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn session(&self) -> &Arc<Session> {
        &self.session
    }
    fn leases(&self) -> std::sync::MutexGuard<'_, Leases> {
        self.leases.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Lease and retirement changes; coalesced, one receiver per reader.
    pub fn observe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changes.subscribe()
    }
    fn changed(&self) {
        self.changes.send_modify(|revision| *revision += 1);
        // The local view redraws its owner banner from the same state.
        self.session.notify();
    }
    /// Releases an expired device lease. Returns true when it changed.
    pub fn expire(&self) -> bool {
        let expired = self.leases().expire(Instant::now());
        if expired {
            self.changed();
        }
        expired
    }
    pub fn lease(&self) -> Lease {
        self.expire();
        self.leases().view(Instant::now())
    }
    /// Grants a free lease, or takes it from another holder when `take_over`.
    /// The current holder keeps its token (a retried acquire is not a new grant).
    pub fn acquire(&self, holder: Holder, take_over: bool) -> Result<Lease, Refused> {
        if self.retired() {
            return Err(Refused::Retired);
        }
        self.expire();
        let (lease, changed) = self.leases().acquire(holder, take_over, Instant::now())?;
        if changed {
            self.changed();
        }
        Ok(lease)
    }
    pub fn release(&self, holder: &Holder, token: u64) -> Result<Lease, Refused> {
        self.expire();
        let lease = self.leases().release(holder, token, Instant::now())?;
        self.changed();
        Ok(lease)
    }
    pub fn renew(&self, holder: &Holder, token: u64) -> Result<Lease, Refused> {
        if self.expire() {
            return Err(Refused::NotHolder);
        }
        self.leases().renew(holder, token, Instant::now())
    }
    /// Queues input of the holder. A repeated `input_id` returns its first outcome
    /// without writing again; success means queued, not executed by the shell.
    pub fn input(
        &self,
        holder: &Holder,
        token: u64,
        input_id: u64,
        bytes: Vec<u8>,
    ) -> Result<(), Refused> {
        if self.retired() {
            return Err(Refused::Retired);
        }
        self.expire();
        // Queued under the lease lock: a take-over cannot interleave with this write.
        self.leases()
            .input(holder, token, input_id, Instant::now(), || {
                self.session.queue_input(bytes)
            })
    }
    pub fn resize(&self, holder: &Holder, token: u64, size: Size) -> Result<(), Refused> {
        if Size::bounded(size.columns, size.rows) != size {
            return Err(Refused::InvalidSize);
        }
        self.expire();
        let leases = self.leases();
        leases.check(holder, token)?;
        // The emulator lock is busy only briefly; the owner's next resize retries.
        self.session.resize(size);
        Ok(())
    }
    /// Typing in the host window. A free lease becomes local (typing is explicit);
    /// a device's lease is never taken implicitly.
    pub fn local_input(&self, bytes: Vec<u8>) -> Result<(), Refused> {
        if self.retired() {
            return Err(Refused::Retired);
        }
        self.expire();
        let now = Instant::now();
        let (changed, queued) = {
            let mut leases = self.leases();
            let (_, changed) = leases.acquire(Holder::Local, false, now)?;
            (
                changed,
                self.session.queue_input(bytes).map_err(Refused::Input),
            )
        };
        if changed {
            self.changed();
        }
        queued?;
        self.session.reset_local_view();
        Ok(())
    }
    /// Layout of the host window. Applies unless a device owns the size.
    pub fn local_resize(&self, size: Size) -> Result<bool, Refused> {
        self.expire();
        let leases = self.leases();
        match &leases.holder {
            Some(holder @ Holder::Device { .. }) => Err(Refused::HeldBy(holder.clone())),
            _ => Ok(self.session.resize(size)),
        }
    }
    /// Registers an open stream of `device`. Dropping its last attachment releases
    /// that device's lease; the process keeps running.
    pub fn attach(self: &Arc<Self>, device: &str) -> Attachment {
        *self.leases().streams.entry(device.to_owned()).or_default() += 1;
        Attachment {
            run: self.clone(),
            device: device.to_owned(),
        }
    }
    /// The run was stopped or replaced; every owner and observer must resync.
    pub fn retire(&self) {
        if !self.retired.swap(true, Ordering::AcqRel) {
            self.leases().holder = None;
            self.changed();
        }
    }
    pub fn retired(&self) -> bool {
        self.retired.load(Ordering::Acquire)
    }
    /// A new run replaced this one (Restart); observers of this run must resync.
    pub fn supersede(&self) {
        self.superseded.store(true, Ordering::Release);
        self.retired.store(true, Ordering::Release);
        self.leases().holder = None;
        self.changed();
    }
    pub fn superseded(&self) -> bool {
        self.superseded.load(Ordering::Acquire)
    }
}

pub struct Attachment {
    run: Arc<Run>,
    device: String,
}
impl Drop for Attachment {
    fn drop(&mut self) {
        if self.run.leases().detach(&self.device) {
            self.run.changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn phone() -> Holder {
        Holder::device("phone", "Phone")
    }
    fn tablet() -> Holder {
        Holder::device("tablet", "Tablet")
    }
    #[test]
    fn one_owner_take_over_fences_the_previous_token() {
        let now = Instant::now();
        let mut leases = Leases::new(Some(Holder::Local), now);
        assert_eq!(
            leases.acquire(phone(), false, now),
            Err(Refused::HeldBy(Holder::Local))
        );
        let (lease, changed) = leases.acquire(phone(), true, now).unwrap();
        assert!(changed && lease.held_by(&phone()) && lease.token() == 2);
        // The host and an observer device are refused without a write.
        let mut writes = 0;
        for (holder, token) in [(Holder::Local, 1), (tablet(), 2), (phone(), 1)] {
            let outcome = leases.input(&holder, token, 1, now, || {
                writes += 1;
                Ok(())
            });
            assert!(outcome.is_err());
        }
        assert_eq!(writes, 0);
        // A retried acquire by the holder is not a new grant.
        assert_eq!(leases.acquire(phone(), true, now).unwrap().0.token(), 2);
        assert_eq!(leases.renew(&phone(), 1, now), Err(Refused::StaleToken));
    }
    #[test]
    fn duplicate_input_id_returns_the_first_outcome_without_writing_again() {
        let now = Instant::now();
        let mut leases = Leases::new(Some(phone()), now);
        let mut writes = 0;
        for _ in 0..2 {
            leases
                .input(&phone(), 1, 7, now, || {
                    writes += 1;
                    Ok(())
                })
                .unwrap();
        }
        assert_eq!(writes, 1);
        let full = leases.input(&phone(), 1, 8, now, || Err(InputRefused::QueueFull));
        assert_eq!(full, Err(Refused::Input(InputRefused::QueueFull)));
        // The same refused ID is not retried into a later, emptier queue.
        assert_eq!(leases.input(&phone(), 1, 8, now, || Ok(())), full);
    }
    #[test]
    fn device_lease_expires_without_renewal_and_on_last_disconnect() {
        let now = Instant::now();
        let mut leases = Leases::new(Some(phone()), now);
        leases.renew(&phone(), 1, now + RENEW_INTERVAL).unwrap();
        assert!(!leases.expire(now + LEASE_TTL));
        assert!(leases.expire(now + RENEW_INTERVAL + LEASE_TTL));
        assert_eq!(leases.renew(&phone(), 1, now), Err(Refused::NotHolder));
        // The host's own lease never expires.
        let mut local = Leases::new(Some(Holder::Local), now);
        assert!(!local.expire(now + LEASE_TTL * 10));
        // Two streams of one device: only the last one closing releases it.
        let (lease, _) = leases.acquire(phone(), false, now).unwrap();
        assert_eq!(lease.token(), 2);
        leases.streams.insert("phone".into(), 2);
        assert!(!leases.detach("phone"));
        assert!(leases.detach("phone"));
        assert!(leases.holder.is_none());
    }
}
