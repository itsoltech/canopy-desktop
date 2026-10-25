//! Bounded, content-free diagnostics. Every duration is measured by one process's
//! monotonic clock. This recorder never schedules UI work or writes per-delta logs.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

const RECORD_LIMIT: usize = 256;
const PENDING_LIMIT: usize = 256;
const HOST_LIMIT: usize = 32;
const STAGE_COUNT: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[repr(usize)]
pub enum Stage {
    WorkerQueue,
    Queue,
    Commit,
    Dispatch,
    ProviderTtft,
    ParseEnqueue,
    Fanout,
    ClientRoundTrip,
    ClientReceiveApply,
    Apply,
    Render,
    CommandTransition,
}
impl Stage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WorkerQueue => "worker_queue",
            Self::Queue => "queue",
            Self::Commit => "commit",
            Self::Dispatch => "dispatch",
            Self::ProviderTtft => "provider_ttft",
            Self::ParseEnqueue => "parse_enqueue",
            Self::Fanout => "fanout",
            Self::ClientRoundTrip => "client_round_trip",
            Self::ClientReceiveApply => "client_receive_apply",
            Self::Apply => "apply",
            Self::Render => "render",
            Self::CommandTransition => "command_transition",
        }
    }
}
const STAGES: [Stage; STAGE_COUNT] = [
    Stage::WorkerQueue,
    Stage::Queue,
    Stage::Commit,
    Stage::Dispatch,
    Stage::ProviderTtft,
    Stage::ParseEnqueue,
    Stage::Fanout,
    Stage::ClientRoundTrip,
    Stage::ClientReceiveApply,
    Stage::Apply,
    Stage::Render,
    Stage::CommandTransition,
];

/// Fixed vocabulary: raw provider, SQLite and transport error text is never accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Accepted,
    Attempting,
    Succeeded,
    Failed,
    Rejected,
    DeliveryUnknown,
    Retry,
    Unauthorized,
    Conflict,
    Retired,
    Overflow,
    Storage,
    Closed,
    Invalid,
    Cancelled,
}
impl Outcome {
    pub fn state(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Attempting => "attempting",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Rejected => "rejected",
            Self::DeliveryUnknown => "delivery_unknown",
            Self::Retry => "retry",
            Self::Ok => "ok",
            _ => "rejected",
        }
    }
    pub fn error_code(self) -> &'static str {
        match self {
            Self::Unauthorized => "permission_denied",
            Self::Conflict => "conflict",
            Self::Retired => "command_retired",
            Self::Overflow => "resource_exhausted",
            Self::Storage => "storage_fault",
            Self::Closed => "closed",
            Self::Invalid => "invalid",
            Self::Cancelled => "cancelled",
            Self::Failed => "effect_failed",
            Self::DeliveryUnknown => "delivery_unknown",
            _ => "",
        }
    }
    pub fn storage(error: &crate::settings::Error) -> Self {
        use crate::settings::Error;
        match error {
            Error::NotAuthorized => Self::Unauthorized,
            Error::CommandConflict | Error::ResourceConflict => Self::Conflict,
            Error::CommandRetired => Self::Retired,
            Error::ResourceLimit | Error::QueueFull | Error::BatchTooLarge => Self::Overflow,
            Error::WorkerStopped => Self::Closed,
            Error::Sqlite(_) | Error::Io(_) | Error::ReadOnly => Self::Storage,
            _ => Self::Invalid,
        }
    }
    pub fn command(state: crate::host::CommandState) -> Self {
        use crate::host::CommandState;
        match state {
            CommandState::Accepted => Self::Accepted,
            CommandState::Attempting => Self::Attempting,
            CommandState::Succeeded => Self::Succeeded,
            CommandState::Failed => Self::Failed,
            CommandState::Rejected => Self::Rejected,
            CommandState::DeliveryUnknown => Self::DeliveryUnknown,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct TimingRecord {
    sequence: u64,
    host_id: String,
    resource_id: String,
    command_id: String,
    device_id: String,
    command_epoch: u64,
    generation: u64,
    stage: Stage,
    duration_us: u64,
    state: &'static str,
    error_code: &'static str,
}
impl TimingRecord {
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn host_id(&self) -> &str {
        &self.host_id
    }
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }
    pub fn command_id(&self) -> &str {
        &self.command_id
    }
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
    pub fn command_epoch(&self) -> u64 {
        self.command_epoch
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
    pub fn stage(&self) -> Stage {
        self.stage
    }
    pub fn duration_us(&self) -> u64 {
        self.duration_us
    }
    pub fn state(&self) -> &str {
        self.state
    }
    pub fn error_code(&self) -> &str {
        self.error_code
    }
}
#[derive(Clone, Serialize)]
pub struct StageSummary {
    stage: Stage,
    count: u64,
    total_us: u64,
    max_us: u64,
    p95_upper_us: u64,
}
impl StageSummary {
    pub fn stage(&self) -> Stage {
        self.stage
    }
    pub fn count(&self) -> u64 {
        self.count
    }
    pub fn total_us(&self) -> u64 {
        self.total_us
    }
    pub fn max_us(&self) -> u64 {
        self.max_us
    }
    pub fn p95_upper_us(&self) -> u64 {
        self.p95_upper_us
    }
}
#[derive(Clone, Serialize)]
pub struct DiagnosticsSnapshot {
    schema_version: u32,
    process_instance_id: String,
    summaries: Vec<StageSummary>,
    records: Vec<TimingRecord>,
    dropped_records: u64,
    rejected_keys: u64,
}
impl DiagnosticsSnapshot {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
    pub fn process_instance_id(&self) -> &str {
        &self.process_instance_id
    }
    pub fn summaries(&self) -> &[StageSummary] {
        &self.summaries
    }
    pub fn records(&self) -> &[TimingRecord] {
        &self.records
    }
    pub fn dropped_records(&self) -> u64 {
        self.dropped_records
    }
    pub fn rejected_keys(&self) -> u64 {
        self.rejected_keys
    }
}
struct Histogram {
    count: u64,
    total: u64,
    max: u64,
    buckets: [u64; 64],
}
impl Default for Histogram {
    fn default() -> Self {
        Self {
            count: 0,
            total: 0,
            max: 0,
            buckets: [0; 64],
        }
    }
}
impl Histogram {
    fn add(&mut self, us: u64) {
        self.count = self.count.saturating_add(1);
        self.total = self.total.saturating_add(us);
        self.max = self.max.max(us);
        let bucket = if us <= 1 {
            0
        } else {
            (64 - (us - 1).leading_zeros()) as usize
        };
        self.buckets[bucket.min(63)] = self.buckets[bucket.min(63)].saturating_add(1);
    }
    fn summary(&self, stage: Stage) -> StageSummary {
        let rank = self.count.saturating_sub(self.count / 20);
        let mut n = 0u64;
        let mut p95 = 0;
        for (ix, count) in self.buckets.iter().enumerate() {
            n = n.saturating_add(*count);
            if n >= rank && self.count > 0 {
                p95 = if ix == 63 { u64::MAX } else { 1u64 << ix };
                break;
            }
        }
        StageSummary {
            stage,
            count: self.count,
            total_us: self.total,
            max_us: self.max,
            p95_upper_us: p95,
        }
    }
}
struct HostMetrics {
    histograms: [Histogram; STAGE_COUNT],
    last_sample: [Option<Instant>; STAGE_COUNT],
}
impl Default for HostMetrics {
    fn default() -> Self {
        Self {
            histograms: std::array::from_fn(|_| Histogram::default()),
            last_sample: [None; STAGE_COUNT],
        }
    }
}
#[derive(Default)]
struct Recorder {
    hosts: HashMap<String, HostMetrics>,
    records: VecDeque<TimingRecord>,
    pending: HashMap<(String, String, String, u64), (Instant, Outcome)>,
    sequence: u64,
    dropped: u64,
    rejected: u64,
}
fn valid_uuid(id: &str) -> bool {
    id.len() == 36 && uuid::Uuid::parse_str(id).is_ok()
}
fn valid_resource(id: &str) -> bool {
    valid_uuid(id)
        || id.len() == 71
            && id.starts_with("opaque:")
            && id[7..].bytes().all(|b| b.is_ascii_hexdigit())
}
fn instance() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| uuid::Uuid::new_v4().to_string())
}
fn recorder() -> &'static Mutex<Recorder> {
    static RECORDER: OnceLock<Mutex<Recorder>> = OnceLock::new();
    RECORDER.get_or_init(Mutex::default)
}
/// Preserve UUID resources. Other domain IDs (including path-based resources) are
/// salted per process before entering the recorder; the salt is never exported.
pub fn resource_id(raw: &str) -> String {
    if valid_uuid(raw) {
        return raw.to_owned();
    }
    static SALT: OnceLock<[u8; 16]> = OnceLock::new();
    let salt = SALT.get_or_init(|| *uuid::Uuid::new_v4().as_bytes());
    let mut hash = Sha256::new();
    hash.update(salt);
    hash.update(raw.as_bytes());
    format!("opaque:{:x}", hash.finalize())
}
impl Recorder {
    fn push(&mut self, mut record: TimingRecord, now: Instant) {
        if !valid_uuid(&record.host_id)
            || !valid_resource(&record.resource_id)
            || !record.command_id.is_empty() && !valid_uuid(&record.command_id)
            || !record.device_id.is_empty()
                && record.device_id != "local"
                && !valid_uuid(&record.device_id)
        {
            self.rejected = self.rejected.saturating_add(1);
            return;
        }
        if !self.hosts.contains_key(&record.host_id) && self.hosts.len() >= HOST_LIMIT {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        let host = self.hosts.entry(record.host_id.clone()).or_default();
        let ix = record.stage as usize;
        host.histograms[ix].add(record.duration_us);
        // Counts include every observation. Only content-free recent samples are
        // rate limited; command transitions are retained until ring eviction.
        if record.stage != Stage::CommandTransition
            && host.last_sample[ix].is_some_and(|last| {
                now.saturating_duration_since(last) < Duration::from_millis(100)
            })
        {
            self.dropped = self.dropped.saturating_add(1);
            return;
        }
        host.last_sample[ix] = Some(now);
        self.sequence = self.sequence.saturating_add(1);
        record.sequence = self.sequence;
        if self.records.len() == RECORD_LIMIT {
            self.records.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.records.push_back(record);
    }
    fn snapshot(&self, host: Option<&str>) -> DiagnosticsSnapshot {
        let mut aggregate: [Histogram; STAGE_COUNT] = std::array::from_fn(|_| Histogram::default());
        for (id, metrics) in &self.hosts {
            if host.is_some_and(|host| host != id) {
                continue;
            }
            for (to, from) in aggregate.iter_mut().zip(&metrics.histograms) {
                to.count = to.count.saturating_add(from.count);
                to.total = to.total.saturating_add(from.total);
                to.max = to.max.max(from.max);
                for (a, b) in to.buckets.iter_mut().zip(from.buckets) {
                    *a = a.saturating_add(b);
                }
            }
        }
        DiagnosticsSnapshot {
            schema_version: 1,
            process_instance_id: instance().into(),
            summaries: aggregate
                .iter()
                .zip(STAGES)
                .map(|(h, s)| h.summary(s))
                .collect(),
            records: self
                .records
                .iter()
                .filter(|r| host.is_none_or(|host| host == r.host_id))
                .cloned()
                .collect(),
            dropped_records: self.dropped,
            rejected_keys: self.rejected,
        }
    }
}
fn timing(
    host: &str,
    resource: &str,
    command: Option<&str>,
    generation: u64,
    stage: Stage,
    duration: Duration,
    outcome: Outcome,
) -> TimingRecord {
    TimingRecord {
        sequence: 0,
        host_id: host.into(),
        resource_id: resource.into(),
        command_id: command.unwrap_or_default().into(),
        device_id: String::new(),
        command_epoch: 0,
        generation,
        stage,
        duration_us: duration.as_micros().min(u64::MAX as u128) as u64,
        state: outcome.state(),
        error_code: outcome.error_code(),
    }
}
/// `started` must originate at the beginning of this stage in this process.
/// No remote wall/monotonic timestamp is accepted by this API.
pub fn record(
    host: &str,
    resource: &str,
    command: Option<&str>,
    generation: u64,
    stage: Stage,
    started: Instant,
    outcome: Outcome,
) {
    // Reject before allocating: callers cannot turn invalid labels into retained
    // plaintext or an arbitrarily large temporary diagnostic record.
    if !valid_uuid(host) || !valid_resource(resource) || command.is_some_and(|id| !valid_uuid(id)) {
        let mut recorder = recorder().lock().unwrap_or_else(|e| e.into_inner());
        recorder.rejected = recorder.rejected.saturating_add(1);
        return;
    }
    let now = Instant::now();
    if let Some(duration) = now.checked_duration_since(started) {
        recorder().lock().unwrap_or_else(|e| e.into_inner()).push(
            timing(
                host, resource, command, generation, stage, duration, outcome,
            ),
            now,
        );
    }
}
/// Record a committed state, never a proposed SQL transition. Duplicate receipts
/// use Retry and do not create another dispatch interval. Pending correlation is
/// bounded; evicted/restarted operations have no invented queue/dispatch duration.
pub fn command_transition(
    host: &str,
    resource: &str,
    key: &crate::host::CommandKey,
    outcome: Outcome,
) {
    recorder()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .transition(host, resource, key, outcome, Instant::now());
}
impl Recorder {
    fn transition(
        &mut self,
        host: &str,
        resource: &str,
        key: &crate::host::CommandKey,
        outcome: Outcome,
        now: Instant,
    ) {
        if !valid_uuid(host)
            || !valid_resource(resource)
            || !valid_uuid(key.id())
            || key.device() != "local" && !valid_uuid(key.device())
        {
            self.rejected = self.rejected.saturating_add(1);
            return;
        }
        let mut record = timing(
            host,
            resource,
            Some(key.id()),
            0,
            Stage::CommandTransition,
            Duration::ZERO,
            outcome,
        );
        record.device_id = key.device().into();
        record.command_epoch = key.epoch();
        let id = (
            host.to_owned(),
            key.device().to_owned(),
            key.id().to_owned(),
            key.epoch(),
        );
        if matches!(
            outcome,
            Outcome::Accepted
                | Outcome::Attempting
                | Outcome::Succeeded
                | Outcome::Failed
                | Outcome::Rejected
                | Outcome::DeliveryUnknown
        ) {
            if let Some((start, prior)) = self.pending.remove(&id) {
                let stage = match (prior, outcome) {
                    (Outcome::Accepted, Outcome::Attempting) => Some(Stage::Queue),
                    (
                        Outcome::Attempting,
                        Outcome::Succeeded | Outcome::Failed | Outcome::DeliveryUnknown,
                    ) => Some(Stage::Dispatch),
                    _ => None,
                };
                if let Some(stage) = stage {
                    let mut interval = record.clone();
                    interval.stage = stage;
                    interval.duration_us = now
                        .saturating_duration_since(start)
                        .as_micros()
                        .min(u64::MAX as u128) as u64;
                    self.push(interval, now);
                }
            }
            if matches!(outcome, Outcome::Accepted | Outcome::Attempting) {
                if self.pending.len() >= PENDING_LIMIT
                    && let Some(oldest) = self
                        .pending
                        .iter()
                        .min_by_key(|(_, v)| v.0)
                        .map(|(k, _)| k.clone())
                {
                    self.pending.remove(&oldest);
                    self.dropped = self.dropped.saturating_add(1);
                }
                self.pending.insert(id, (now, outcome));
            }
        }
        self.push(record, now);
    }
}

pub fn snapshot() -> DiagnosticsSnapshot {
    recorder()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .snapshot(None)
}
pub fn snapshot_for(host: &str) -> DiagnosticsSnapshot {
    recorder()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .snapshot(Some(host))
}
pub fn snapshot_json() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&snapshot())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    #[test]
    fn causal_intervals_use_only_local_instants_and_retry_never_restarts_dispatch() {
        let mut recorder = Recorder::default();
        let host = id();
        let resource = id();
        let key = crate::host::CommandKey::local(id()).unwrap();
        let base = Instant::now();
        recorder.transition(&host, &resource, &key, Outcome::Accepted, base);
        recorder.transition(
            &host,
            &resource,
            &key,
            Outcome::Retry,
            base + Duration::from_millis(8),
        );
        recorder.transition(
            &host,
            &resource,
            &key,
            Outcome::Attempting,
            base + Duration::from_millis(10),
        );
        recorder.transition(
            &host,
            &resource,
            &key,
            Outcome::Conflict,
            base + Duration::from_millis(15),
        );
        recorder.transition(
            &host,
            &resource,
            &key,
            Outcome::Succeeded,
            base + Duration::from_millis(30),
        );
        recorder.transition(
            &host,
            &resource,
            &key,
            Outcome::Retry,
            base + Duration::from_millis(40),
        );
        let snapshot = recorder.snapshot(Some(&host));
        let stage = |stage| {
            snapshot
                .summaries()
                .iter()
                .find(|s| s.stage() == stage)
                .unwrap()
        };
        assert_eq!(
            (stage(Stage::Queue).count(), stage(Stage::Queue).total_us()),
            (1, 10_000)
        );
        assert_eq!(
            (
                stage(Stage::Dispatch).count(),
                stage(Stage::Dispatch).total_us()
            ),
            (1, 20_000)
        );
        assert_eq!(stage(Stage::Dispatch).p95_upper_us(), 32768);
        assert!(recorder.pending.is_empty());
        // Restarted recorder cannot manufacture an interval from a wall timestamp.
        let mut restarted = Recorder::default();
        restarted.transition(&host, &resource, &key, Outcome::Succeeded, base);
        assert_eq!(
            restarted.snapshot(None).summaries()[Stage::Dispatch as usize].count(),
            0
        );
    }

    #[test]
    fn diagnostics_bound_retention_and_reject_content_in_keys() {
        let mut recorder = Recorder::default();
        let host = id();
        let resource = id();
        let now = Instant::now();
        for _ in 0..(RECORD_LIMIT + 20) {
            let key = crate::host::CommandKey::local(id()).unwrap();
            recorder.transition(&host, &resource, &key, Outcome::Accepted, now);
        }
        assert_eq!(recorder.records.len(), RECORD_LIMIT);
        assert_eq!(recorder.pending.len(), PENDING_LIMIT);
        for raw in [
            "secret prompt",
            "/home/private/key",
            "token=secret",
            "opaque:plaintext",
        ] {
            recorder.push(
                timing(
                    &host,
                    raw,
                    None,
                    0,
                    Stage::Apply,
                    Duration::ZERO,
                    Outcome::Ok,
                ),
                now,
            );
            recorder.push(
                timing(
                    raw,
                    &resource,
                    None,
                    0,
                    Stage::Apply,
                    Duration::ZERO,
                    Outcome::Ok,
                ),
                now,
            );
        }
        let encoded = serde_json::to_string(&recorder.snapshot(None)).unwrap();
        assert!(!encoded.contains("secret"));
        assert!(!encoded.contains("/home"));
        assert!(!encoded.contains("plaintext"));
        assert_eq!(recorder.rejected, 8);
        let opaque = resource_id("/home/private/key");
        assert!(valid_resource(&opaque));
        assert_eq!(opaque, resource_id("/home/private/key"));
        for _ in 0..1000 {
            recorder.push(
                timing(
                    &host,
                    &resource,
                    None,
                    1,
                    Stage::Apply,
                    Duration::from_micros(10),
                    Outcome::Ok,
                ),
                now,
            );
        }
        let snapshot = recorder.snapshot(Some(&host));
        assert_eq!(snapshot.summaries()[Stage::Apply as usize].count(), 1000);
        assert_eq!(
            snapshot
                .records()
                .iter()
                .filter(|r| r.stage() == Stage::Apply)
                .count(),
            1
        );
        assert!(snapshot.dropped_records() >= 999);
        assert!(recorder.snapshot(Some(&id())).records().is_empty());
    }
    #[test]
    fn sqlite_commit_failure_and_retry_report_one_effect_without_payloads() {
        futures_lite::future::block_on(async {
            use crate::{
                host::{CommandInput, CommandKey, CommandState},
                settings::SettingsClient,
            };
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("metrics.db");
            let client = SettingsClient::create(&path).await.unwrap();
            let host = client.initialize_runtime().await.unwrap();
            let resource = id();
            let key = CommandKey::local(id()).unwrap();
            let input = CommandInput::new(
                key.clone(),
                "files.save".into(),
                resource.clone(),
                b"private prompt and tool output".to_vec(),
                vec![9; 32],
            )
            .unwrap()
            .with_root("private".into())
            .unwrap();
            client
                .accept_runtime_command(input.clone(), 8)
                .await
                .unwrap();
            let db = rusqlite::Connection::open(&path).unwrap();
            db.execute_batch("CREATE TRIGGER fail_attempt BEFORE UPDATE OF state ON _canopy_command_inbox WHEN NEW.state='attempting' BEGIN SELECT RAISE(ABORT, 'sensitive database detail'); END;").unwrap();
            assert!(
                client
                    .transition_runtime_command(
                        key.clone(),
                        CommandState::Accepted,
                        CommandState::Attempting,
                        None
                    )
                    .await
                    .is_err()
            );
            let failed = snapshot_for(host.id());
            assert!(!failed.records().iter().any(|r| r.state() == "attempting"));
            assert!(
                failed
                    .records()
                    .iter()
                    .any(|r| r.error_code() == "storage_fault")
            );
            assert_eq!(
                client
                    .get_runtime_command(key.clone(), 8)
                    .await
                    .unwrap()
                    .unwrap()
                    .state(),
                CommandState::Accepted
            );
            db.execute_batch("DROP TRIGGER fail_attempt").unwrap();
            let effect = dir.path().join("effect");
            for _ in 0..2 {
                let receipt = client
                    .accept_runtime_command(input.clone(), 8)
                    .await
                    .unwrap();
                if receipt.state() == CommandState::Accepted {
                    client
                        .transition_runtime_command(
                            key.clone(),
                            CommandState::Accepted,
                            CommandState::Attempting,
                            None,
                        )
                        .await
                        .unwrap();
                    use std::io::Write;
                    std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&effect)
                        .unwrap()
                        .write_all(b"one effect\n")
                        .unwrap();
                    client
                        .transition_runtime_command(
                            key.clone(),
                            CommandState::Attempting,
                            CommandState::Failed,
                            Some(b"private result".to_vec()),
                        )
                        .await
                        .unwrap();
                }
            }
            assert_eq!(std::fs::read_to_string(effect).unwrap(), "one effect\n");
            let snapshot = snapshot_for(host.id());
            let transitions: Vec<_> = snapshot
                .records()
                .iter()
                .filter(|r| r.stage() == Stage::CommandTransition)
                .collect();
            assert_eq!(
                transitions
                    .iter()
                    .filter(|r| r.state() == "accepted")
                    .count(),
                1
            );
            assert_eq!(
                transitions
                    .iter()
                    .filter(|r| r.state() == "attempting")
                    .count(),
                1
            );
            assert_eq!(
                transitions.iter().filter(|r| r.state() == "failed").count(),
                1
            );
            assert!(transitions.iter().all(|r| r.command_id() == key.id()
                && r.host_id() == host.id()
                && r.resource_id() == resource));
            assert_eq!(snapshot.summaries()[Stage::Dispatch as usize].count(), 1);
            let json = serde_json::to_string(&snapshot).unwrap();
            for secret in [
                "private prompt",
                "tool output",
                "sensitive database",
                "private result",
            ] {
                assert!(!json.contains(secret));
            }
            client.shutdown().await.unwrap();
        });
    }
}
