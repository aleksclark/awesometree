#![allow(
    clippy::collapsible_if,
    clippy::ptr_arg,
    clippy::single_match,
    clippy::result_large_err,
    clippy::redundant_pattern_matching,
    clippy::redundant_locals
)]

pub mod paths;
#[cfg(feature = "gui")]
pub mod theme;
#[cfg(feature = "gui")]
pub mod ui_helpers;
pub mod interop;
pub mod state;
pub mod workspace;
pub mod wm;
#[cfg(feature = "gui")]
pub mod picker;
#[cfg(feature = "gui")]
pub mod projects_ui;
#[cfg(feature = "gui")]
pub mod agents_ui;
#[cfg(feature = "gui")]
pub mod text_input;
#[cfg(feature = "gui")]
pub mod tray;
#[cfg(feature = "gui")]
pub mod daemon;
#[cfg(feature = "gui")]
pub mod notify;
pub mod log;
pub mod auth;
#[cfg(feature = "gui")]
pub mod qr;
pub mod server;
pub mod acp_supervisor;
pub mod agent_supervisor;
pub mod a2a_proxy;
pub mod mcp;
