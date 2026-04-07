#![allow(
    clippy::collapsible_if,
    clippy::ptr_arg,
    clippy::single_match,
    clippy::result_large_err,
    clippy::redundant_pattern_matching,
    clippy::redundant_locals
)]

pub mod paths;
pub mod theme;
pub mod ui_helpers;
pub mod interop;
pub mod state;
pub mod workspace;
pub mod wm;
pub mod picker;
pub mod projects_ui;
pub mod text_input;
pub mod tray;
pub mod daemon;
pub mod notify;
pub mod log;
pub mod auth;
pub mod qr;
pub mod server;
pub mod acp_supervisor;
