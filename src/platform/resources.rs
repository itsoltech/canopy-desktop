//! CPU and resident memory of Canopy and the processes it started (terminals,
//! agents and their tools). Sampling blocks: call it off the UI thread.
use std::collections::{HashMap, HashSet};
use sysinfo::{CpuRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

/// One process, or a sum of processes. `cpu` is a percentage where 100 is one
/// fully used core, as in Activity Monitor; `memory` is resident bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    pub cpu: f32,
    pub memory: u64,
}
impl std::ops::Add for Usage {
    type Output = Self;
    fn add(self, other: Self) -> Self {
        Self {
            cpu: self.cpu + other.cpu,
            memory: self.memory.saturating_add(other.memory),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sample {
    /// Canopy's own process.
    pub app: Usage,
    /// Every process descending from Canopy.
    pub children: Usage,
    pub child_processes: usize,
}
impl Sample {
    pub fn total(&self) -> Usage {
        self.app + self.children
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent: Option<u32>,
    pub usage: Usage,
}

/// Sums `root` and its whole process tree; unrelated processes are ignored.
/// Parent links that loop (PID reuse) are visited once.
pub fn aggregate(root: u32, processes: impl IntoIterator<Item = ProcessInfo>) -> Sample {
    let mut usage = HashMap::new();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for process in processes {
        usage.insert(process.pid, process.usage);
        if let Some(parent) = process.parent.filter(|parent| *parent != process.pid) {
            children.entry(parent).or_default().push(process.pid);
        }
    }
    let mut sample = Sample {
        app: usage.get(&root).copied().unwrap_or_default(),
        ..Sample::default()
    };
    let mut seen = HashSet::from([root]);
    let mut pending = children.get(&root).cloned().unwrap_or_default();
    while let Some(pid) = pending.pop() {
        if !seen.insert(pid) {
            continue;
        }
        if let Some(process) = usage.get(&pid) {
            sample.children = sample.children + *process;
            sample.child_processes += 1;
        }
        if let Some(next) = children.get(&pid) {
            pending.extend(next);
        }
    }
    sample
}

/// Keeps the previous process times, so CPU is measured between two calls.
pub struct Sampler {
    system: System,
    root: Pid,
}
impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}
impl Sampler {
    pub fn new() -> Self {
        Self {
            // The CPU list scales per-process CPU on some platforms.
            system: System::new_with_specifics(RefreshKind::new().with_cpu(CpuRefreshKind::new())),
            root: Pid::from_u32(std::process::id()),
        }
    }
    /// Blocking. CPU covers the time since the previous call (0 on the first one).
    pub fn sample(&mut self) -> Sample {
        self.system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            ProcessRefreshKind::new().with_cpu().with_memory(),
        );
        aggregate(
            self.root.as_u32(),
            self.system
                .processes()
                .iter()
                // Linux lists threads as processes of their own; they share memory.
                .filter(|(_, process)| process.thread_kind().is_none())
                .map(|(pid, process)| ProcessInfo {
                    pid: pid.as_u32(),
                    parent: process.parent().map(Pid::as_u32),
                    usage: Usage {
                        cpu: process.cpu_usage(),
                        memory: process.memory(),
                    },
                }),
        )
    }
}

/// `3%`: whole percents keep the readout steady.
pub fn format_cpu(cpu: f32) -> String {
    format!("{:.0}%", cpu.max(0.))
}

/// `412 MB` below a gigabyte, then `1.2 GB` (binary units, as Activity Monitor).
pub fn format_memory(bytes: u64) -> String {
    const MB: f64 = 1024. * 1024.;
    let megabytes = bytes as f64 / MB;
    if megabytes < 1000. {
        format!("{megabytes:.0} MB")
    } else {
        format!("{:.1} GB", megabytes / 1024.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process(pid: u32, parent: Option<u32>, cpu: f32, memory: u64) -> ProcessInfo {
        ProcessInfo {
            pid,
            parent,
            usage: Usage { cpu, memory },
        }
    }

    #[test]
    fn sums_the_whole_tree_below_canopy_and_nothing_else() {
        let sample = aggregate(
            10,
            [
                process(1, None, 50., 1000),
                process(10, Some(1), 5., 100),
                // A terminal's shell, its agent and a tool the agent runs.
                process(20, Some(10), 1., 10),
                process(21, Some(20), 2., 20),
                process(22, Some(21), 3., 30),
                // A sibling of Canopy and its child are not ours.
                process(30, Some(1), 40., 400),
                process(31, Some(30), 40., 400),
            ],
        );
        assert_eq!(
            sample.app,
            Usage {
                cpu: 5.,
                memory: 100
            }
        );
        assert_eq!(
            sample.children,
            Usage {
                cpu: 6.,
                memory: 60
            }
        );
        assert_eq!(sample.child_processes, 3);
        assert_eq!(
            sample.total(),
            Usage {
                cpu: 11.,
                memory: 160
            }
        );
    }

    #[test]
    fn a_parent_loop_from_pid_reuse_is_counted_once() {
        let sample = aggregate(
            10,
            [
                process(10, Some(21), 1., 1),
                process(20, Some(10), 1., 1),
                process(21, Some(20), 1., 1),
            ],
        );
        assert_eq!(sample.child_processes, 2);
        assert_eq!(sample.total().memory, 3);
    }

    #[test]
    fn readout_units() {
        assert_eq!(format_cpu(2.6), "3%");
        assert_eq!(format_cpu(-0.1), "0%");
        assert_eq!(format_memory(412 * 1024 * 1024), "412 MB");
        assert_eq!(format_memory(1536 * 1024 * 1024), "1.5 GB");
    }

    #[test]
    fn samples_this_process() {
        let mut sampler = Sampler::new();
        let sample = sampler.sample();
        assert!(sample.app.memory > 0);
    }
}
