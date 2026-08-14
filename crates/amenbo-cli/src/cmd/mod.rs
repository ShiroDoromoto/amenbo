//! What `run()` dispatches into. One module per command group, in the shape the dispatch already
//! has, plus the helpers those groups share.

pub(crate) mod activity;
pub(crate) mod arg;
pub(crate) mod attach;
pub(crate) mod binding;
pub(crate) mod comment;
pub(crate) mod config;
pub(crate) mod data;
pub(crate) mod decision;
pub(crate) mod dimension;
pub(crate) mod guard;
pub(crate) mod hard_erase;
pub(crate) mod labels;
pub(crate) mod lint;
pub(crate) mod outbox;
pub(crate) mod place;
pub(crate) mod plugin;
pub(crate) mod premise;
pub(crate) mod project;
pub(crate) mod setup;
pub(crate) mod status;
pub(crate) mod task;
pub(crate) mod update;
