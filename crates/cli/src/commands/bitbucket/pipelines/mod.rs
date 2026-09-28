//! Bitbucket Pipelines commands.
//!
//! Split by command: `list` (list, get, latest and pipeline lookup), `steps`,
//! `logs`, `trigger` (trigger, stop, rerun), `status` and `watch`. `model` holds
//! the API and output types, `state` the status derivation every command shares.

mod list;
mod logs;
#[cfg(test)]
mod mock_tests;
mod model;
mod rows;
mod state;
mod status;
mod steps;
mod trigger;
mod watch;

pub use list::{
    find_latest_pipeline_for_branch, get_pipeline, list_pipelines, resolve_pipeline_id,
};
pub use logs::get_pipeline_logs;
pub use state::status_to_exit_code;
pub use status::pipeline_status;
pub use steps::{list_steps, pipeline_has_failed_steps};
pub use trigger::{rerun_pipeline, stop_pipeline, trigger_pipeline};
pub use watch::watch_pipeline;
