//! gRPC service implementations for the ARP protocol.

pub mod arp_proto {
    tonic::include_proto!("arp.v1");
}

pub mod convert;
pub mod project;
pub mod workspace;
pub mod agent;

// Re-export service impl structs for convenience.
pub use project::ProjectServiceImpl;
pub use workspace::WorkspaceServiceImpl;
pub use agent::AgentServiceImpl;

use crate::auth;

/// Extract a scoped bearer token from gRPC request metadata.
///
/// Looks for an `authorization` metadata key with a `Bearer <token>` value.
/// If the token is missing or invalid, falls back to a localhost admin token.
pub fn extract_token<T>(req: &tonic::Request<T>) -> auth::ScopedToken {
    req.metadata()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .and_then(auth::validate_scoped_token)
        .unwrap_or_else(auth::localhost_admin_token)
}
