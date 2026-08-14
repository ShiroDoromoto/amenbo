//! One module per domain the scenario vocabulary names, each holding that domain's actions and
//! asserts — the arms that turn a step into a call on the shipped binary, and its answer into a
//! verdict.
//!
//! The split is by domain and nothing else: `lib.rs` keeps the machinery every arm stands on (the
//! isolated session, the invocation, the bindings, the report) and hands each step to the domain it
//! names. What lives here is what a reader looking for "how is `task depend` driven?" came for, at a
//! size where the answer can be found by opening one file.

pub(crate) mod attachment;
pub(crate) mod comment;
pub(crate) mod decision;
pub(crate) mod dimension;
pub(crate) mod folder;
pub(crate) mod mcp;
pub(crate) mod plugin;
pub(crate) mod project;
pub(crate) mod repo;
pub(crate) mod store;
pub(crate) mod task;
