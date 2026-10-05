//! Background work: a job queue, recurring tasks and the workers that run them.
//!
//! * [`queue`]: jobs in each organization's database (a plugin function to call, or a message to
//!   deliver), claimed atomically so any number of workers can share them.
//! * [`tasks`]: recurring tasks (`[[schedule]]` in a plugin manifest, or `scheduler::register`)
//!   that put a job on the queue when they are due.
//! * [`worker`]: the loop that finds due work in every organization and runs it. It runs inside
//!   `aether --serve` (embedded) or on its own with `aether --start-scheduler` (standalone, with a
//!   control API on a port of its own, see [`control`]).
//! * [`nodes`]: each worker announces itself in the core database, so the HTTP server can find a
//!   standalone one and an embedded one stands down while a standalone one is alive.
//!
//! The database is the source of truth. The HTTP server tells the scheduler about new work only
//! to make it start sooner; a scheduler that never hears anything still finds everything on its
//! next look.

use std::{
    collections::HashSet,
    sync::{
        Arc, Mutex, PoisonError, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub mod control;
pub mod nodes;
pub mod queue;
pub mod schedule;
pub mod tasks;
pub mod worker;

pub use queue::{Enqueued, Job, NewJob, QueueError};
pub use schedule::{Recurrence, ScheduleError};
pub use tasks::{CatchUp, TaskDef, TaskError};

/// Where a standalone scheduler's control API can be reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    /// `host:port`.
    pub address: String,
    pub token: String,
}

#[derive(Default)]
struct LinkInner {
    wake: tokio::sync::Notify,
    hot: Mutex<HashSet<String>>,
    reload: AtomicBool,
    remote: RwLock<Option<Remote>>,
    paused: AtomicBool,
}

/// How a process reaches the scheduler: a way to say "there is work in this organization" or
/// "plugins changed", and to learn that someone said so. Cloning shares the same link.
#[derive(Clone, Default)]
pub struct SchedulerLink {
    inner: Arc<LinkInner>,
}

impl SchedulerLink {
    /// Work has been added to `org`'s queue: wake the scheduler in this process and any
    /// standalone one.
    pub fn wake(&self, org: &str) {
        self.wake_local(org);
        if let Some(remote) = self.remote() {
            let org = org.to_string();
            tokio::spawn(async move {
                control::push(&remote, "wake", &serde_json::json!({ "org": org })).await;
            });
        }
    }

    /// Wake only the scheduler in this process.
    pub fn wake_local(&self, org: &str) {
        self.inner.hot.lock().unwrap_or_else(PoisonError::into_inner).insert(org.to_string());
        self.inner.wake.notify_one();
    }

    /// Plugins or their schedules changed: look at everything now.
    pub fn reload(&self) {
        self.reload_local();
        if let Some(remote) = self.remote() {
            tokio::spawn(async move {
                control::push(&remote, "reload", &serde_json::json!({})).await;
            });
        }
    }

    pub fn reload_local(&self) {
        self.inner.reload.store(true, Ordering::SeqCst);
        self.inner.wake.notify_one();
    }

    /// Organizations that were woken since the last call.
    pub fn take_hot(&self) -> Vec<String> {
        self.inner.hot.lock().unwrap_or_else(PoisonError::into_inner).drain().collect()
    }

    /// Whether a reload was requested since the last call.
    pub fn take_reload(&self) -> bool {
        self.inner.reload.swap(false, Ordering::SeqCst)
    }

    /// Sleep until woken or `timeout` passes.
    pub async fn wait(&self, timeout: Duration) {
        let _ = tokio::time::timeout(timeout, self.inner.wake.notified()).await;
    }

    pub fn set_remote(&self, remote: Option<Remote>) {
        *self.inner.remote.write().unwrap_or_else(PoisonError::into_inner) = remote;
    }

    pub fn remote(&self) -> Option<Remote> {
        self.inner.remote.read().unwrap_or_else(PoisonError::into_inner).clone()
    }

    pub fn set_paused(&self, paused: bool) {
        self.inner.paused.store(paused, Ordering::SeqCst);
        self.inner.wake.notify_one();
    }

    pub fn is_paused(&self) -> bool {
        self.inner.paused.load(Ordering::SeqCst)
    }
}

/// What plugins' scheduling and messaging commands use to reach the scheduler and the settings.
pub struct AppScheduler {
    pub state: crate::state::AppState,
}

#[async_trait::async_trait]
impl crate::kernel::SchedulerHandle for AppScheduler {
    fn wake(&self, org: &str) {
        self.state.scheduler.wake(org);
    }

    fn reload(&self) {
        self.state.scheduler.reload();
    }

    async fn defaults(&self, org: &str) -> crate::kernel::JobDefaults {
        let org = crate::tenancy::OrgRef { slug: org.to_string(), db_name: org.to_string() };
        crate::kernel::JobDefaults {
            max_attempts: worker::number_setting(&self.state, &org, "scheduler.max_attempts", 3).await.clamp(1, 20),
            backoff_secs: worker::number_setting(&self.state, &org, "scheduler.retry_backoff_secs", 30).await.clamp(1, 3600),
        }
    }

    async fn check_messaging(&self, org: &str, kind: &str) -> Result<(), String> {
        crate::messaging::check_configured(&self.state, org, kind).await.map_err(|error| error.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn waking_marks_the_organization_and_ends_the_wait() {
        let link = SchedulerLink::default();
        link.wake_local("acme");
        link.wake_local("acme");
        link.wake_local("beta");
        // A wake that happened before the wait is not lost.
        tokio::time::timeout(Duration::from_secs(1), link.wait(Duration::from_secs(30))).await.unwrap();
        let mut hot = link.take_hot();
        hot.sort();
        assert_eq!(hot, ["acme", "beta"]);
        assert!(link.take_hot().is_empty());
    }

    #[tokio::test]
    async fn reload_is_reported_once() {
        let link = SchedulerLink::default();
        assert!(!link.take_reload());
        link.reload_local();
        assert!(link.take_reload());
        assert!(!link.take_reload());
    }

    #[test]
    fn pausing_is_shared_between_clones() {
        let link = SchedulerLink::default();
        let other = link.clone();
        other.set_paused(true);
        assert!(link.is_paused());
    }
}
