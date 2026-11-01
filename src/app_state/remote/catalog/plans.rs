//! Close/delete preflight plans: what the device confirmed, bound to that device, the
//! resource and the intent, valid for a short time and for one command. The owner
//! rechecks the conditions right before the effect; a plan is only proof of what was
//! shown, never of what is true now.
use crate::app_state::{chats::DeletePreflight, project_commands::ProjectClosePreflight};
use canopy_desktop::{
    api::proto,
    remote::auth::{RpcAuth, storage_status},
};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};
use tonic::Status;

/// How long a confirmation stays valid.
pub(super) const TTL: Duration = Duration::from_secs(5 * 60);
/// Plans kept for the whole host and per device; the oldest of a device goes first.
const PLANS: usize = 256;
const DEVICE_PLANS: usize = 16;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum PlanKind {
    Chat(DeletePreflight),
    Project {
        preflight: ProjectClosePreflight,
        /// Canonical checkout, the authority root the device was checked against.
        root: String,
        /// Unsaved buffers devices reported for its files (APP-04), as shown.
        dirty: Vec<Dirty>,
    },
    Terminal {
        id: String,
        root: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum PlanResource {
    Thread(String),
    Project(String),
    Terminal(String),
}
struct Plan {
    device: String,
    resource: PlanResource,
    delete: bool,
    created: Instant,
    kind: PlanKind,
    /// The command that used it; only a retry of that command may use it again.
    consumed: Option<String>,
}

#[derive(Default)]
pub(super) struct Plans {
    plans: HashMap<String, Plan>,
}
impl Plans {
    fn prune(&mut self, now: Instant) {
        self.plans
            .retain(|_, plan| now.duration_since(plan.created) < TTL);
    }
    pub(super) fn insert(
        &mut self,
        device: &str,
        resource: PlanResource,
        delete: bool,
        kind: PlanKind,
    ) -> Result<String, Status> {
        self.insert_at(device, resource, delete, kind, Instant::now())
    }
    fn insert_at(
        &mut self,
        device: &str,
        resource: PlanResource,
        delete: bool,
        kind: PlanKind,
        now: Instant,
    ) -> Result<String, Status> {
        self.prune(now);
        let mine: Vec<_> = self
            .plans
            .iter()
            .filter(|(_, p)| p.device == device)
            .map(|(id, p)| (p.created, id.clone()))
            .collect();
        if mine.len() >= DEVICE_PLANS
            && let Some((_, oldest)) = mine.into_iter().min()
        {
            self.plans.remove(&oldest);
        }
        if self.plans.len() >= PLANS {
            return Err(Status::resource_exhausted(
                "Too many pending close confirmations on the host; retry shortly.",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.plans.insert(
            id.clone(),
            Plan {
                device: device.into(),
                resource,
                delete,
                created: now,
                kind,
                consumed: None,
            },
        );
        Ok(id)
    }
    /// The plan `id` for `command` of `device` on `resource`. It is consumed by that
    /// command: another command needs a new preflight.
    pub(super) fn take(
        &mut self,
        id: &str,
        device: &str,
        resource: &PlanResource,
        command: &str,
    ) -> Result<PlanKind, Status> {
        self.take_at(id, device, resource, command, Instant::now())
    }
    fn take_at(
        &mut self,
        id: &str,
        device: &str,
        resource: &PlanResource,
        command: &str,
        now: Instant,
    ) -> Result<PlanKind, Status> {
        self.prune(now);
        let expired = || Status::failed_precondition("plan_expired: confirm the close again.");
        // Another device's plan is not disclosed as existing.
        let plan = self
            .plans
            .get_mut(id)
            .filter(|plan| plan.device == device)
            .ok_or_else(expired)?;
        if &plan.resource != resource {
            return Err(Status::invalid_argument(
                "The confirmation belongs to another resource.",
            ));
        }
        if !plan.delete && matches!(plan.kind, PlanKind::Chat(_)) {
            return Err(Status::invalid_argument(
                "The confirmation does not cover deleting this chat.",
            ));
        }
        match &plan.consumed {
            Some(other) if other != command => Err(Status::failed_precondition(
                "plan_consumed: this confirmation was used; confirm the close again.",
            )),
            _ => {
                plan.consumed = Some(command.into());
                Ok(plan.kind.clone())
            }
        }
    }
}

/// One device's unsaved buffer of a checkout file, without its text.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Dirty {
    device: String,
    path: String,
    base_hash: [u8; 32],
    last_seen: i64,
    online: bool,
}
impl Dirty {
    pub(super) fn wire(&self, project: &proto::ProjectRef) -> proto::DirtyBufferPresence {
        proto::DirtyBufferPresence {
            device_id: self.device.clone(),
            file: Some(proto::FileRef {
                target: Some(proto::ResourceTarget {
                    resource: Some(proto::resource_target::Resource::Project(project.clone())),
                }),
                relative_path: self.path.clone(),
            }),
            base_hash: self.base_hash.to_vec(),
            last_seen_unix: self.last_seen,
            online: self.online,
        }
    }
}

/// Unsaved buffers devices reported under `root`, in a stable order.
pub(super) async fn presence(
    auth: &RpcAuth,
    root: &str,
    state: &canopy_desktop::remote::host::HostState,
) -> Result<Vec<Dirty>, Status> {
    let mut found: Vec<_> = auth
        .client()
        .runtime_root_presence(root.into())
        .await
        .map_err(storage_status)?
        .into_iter()
        .map(|(file, p)| Dirty {
            device: p.device_id().into(),
            path: file.relative_path().into(),
            base_hash: *p.base_hash(),
            last_seen: p.last_seen(),
            online: state.device_online(p.device_id()),
        })
        .collect();
    found.sort_by(|a, b| (&a.device, &a.path).cmp(&(&b.device, &b.path)));
    Ok(found)
}
pub(super) fn active_dirty(dirty: &[Dirty]) -> bool {
    dirty.iter().any(|d| d.online)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat(id: &str) -> PlanResource {
        PlanResource::Thread(id.into())
    }
    fn kind() -> PlanKind {
        PlanKind::Terminal {
            id: "t".into(),
            root: "/r".into(),
        }
    }

    #[test]
    fn a_plan_serves_one_command_of_its_device_until_it_expires() {
        let mut plans = Plans::default();
        let start = Instant::now();
        let id = plans
            .insert_at("phone", chat("a"), true, kind(), start)
            .unwrap();
        // Another device can't use (or learn of) it; another resource is refused.
        assert_eq!(
            plans
                .take_at(&id, "laptop", &chat("a"), "c1", start)
                .unwrap_err()
                .code(),
            tonic::Code::FailedPrecondition
        );
        assert!(
            plans
                .take_at(&id, "phone", &chat("b"), "c1", start)
                .is_err()
        );
        assert_eq!(
            plans
                .take_at(&id, "phone", &chat("a"), "c1", start)
                .unwrap(),
            kind()
        );
        // The same command may retry; a second command needs a new preflight.
        assert!(plans.take_at(&id, "phone", &chat("a"), "c1", start).is_ok());
        let second = plans
            .take_at(&id, "phone", &chat("a"), "c2", start)
            .unwrap_err();
        assert!(second.message().starts_with("plan_consumed"));
        let late = plans
            .take_at(&id, "phone", &chat("a"), "c1", start + TTL)
            .unwrap_err();
        assert!(late.message().starts_with("plan_expired"));
    }

    #[test]
    fn plans_stay_bounded_per_device_and_host() {
        let mut plans = Plans::default();
        let now = Instant::now();
        let first = plans
            .insert_at("phone", chat("a"), true, kind(), now)
            .unwrap();
        for _ in 1..=DEVICE_PLANS {
            plans
                .insert_at(
                    "phone",
                    chat("a"),
                    true,
                    kind(),
                    now + Duration::from_millis(1),
                )
                .unwrap();
        }
        assert!(
            plans
                .take_at(&first, "phone", &chat("a"), "c", now)
                .is_err()
        );
        for n in 0..PLANS {
            let device = format!("d{n}");
            if plans
                .insert_at(&device, chat("a"), true, kind(), now)
                .is_err()
            {
                assert!(plans.plans.len() <= PLANS);
                return;
            }
        }
        panic!("the host-wide limit refuses new plans");
    }
}
