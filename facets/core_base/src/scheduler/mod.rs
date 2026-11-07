//! The parts of the scheduler that need nothing from the rest of the kernel: the job queue,
//! recurring task definitions, the schedule arithmetic and the worker registry. The workers and
//! the link to them live in `aether_core::scheduler`, which re-exports these.

pub mod nodes;
pub mod queue;
pub mod schedule;
pub mod tasks;

pub use queue::{Enqueued, Job, NewJob, QueueError};
pub use schedule::{Recurrence, ScheduleError};
pub use tasks::{CatchUp, TaskDef, TaskError};
