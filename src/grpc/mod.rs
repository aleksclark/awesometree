//! gRPC service implementations for the ARP protocol.

pub mod arp_proto {
    tonic::include_proto!("arp.v1");
}

pub mod convert;
pub mod project;
pub mod workspace;
pub mod agent;
pub mod discovery;
pub mod token;

// Re-export service impl structs for convenience.
pub use project::ProjectServiceImpl;
pub use workspace::WorkspaceServiceImpl;
pub use agent::AgentServiceImpl;
pub use discovery::DiscoveryServiceImpl;
pub use token::TokenServiceImpl;

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

/// Build the tonic gRPC Router with all ARP services.
pub fn grpc_router() -> tonic::transport::server::Router {
    tonic::transport::Server::builder()
        .add_service(arp_proto::project_service_server::ProjectServiceServer::new(ProjectServiceImpl))
        .add_service(arp_proto::workspace_service_server::WorkspaceServiceServer::new(WorkspaceServiceImpl))
        .add_service(arp_proto::agent_service_server::AgentServiceServer::new(AgentServiceImpl))
        .add_service(arp_proto::discovery_service_server::DiscoveryServiceServer::new(DiscoveryServiceImpl))
        .add_service(arp_proto::token_service_server::TokenServiceServer::new(TokenServiceImpl))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_token_returns_admin_for_missing_auth() {
        let req = tonic::Request::new(());
        let token = extract_token(&req);
        assert_eq!(token.permission, auth::Permission::Admin);
    }

    #[test]
    fn extract_token_with_valid_bearer() {
        let scoped = auth::create_scoped_token(
            "test-user",
            auth::TokenScope::Projects(vec!["myproject".into()]),
            auth::Permission::Project,
            None,
        );
        let bearer = auth::encode_scoped_token(&scoped);

        let mut req = tonic::Request::new(());
        req.metadata_mut().insert(
            "authorization",
            format!("Bearer {bearer}").parse().unwrap(),
        );
        let token = extract_token(&req);
        assert_eq!(token.subject, "test-user");
        assert_eq!(token.permission, auth::Permission::Project);
    }

    #[test]
    fn extract_token_with_invalid_bearer_falls_back() {
        let mut req = tonic::Request::new(());
        req.metadata_mut().insert(
            "authorization",
            "Bearer invalid-token-data".parse().unwrap(),
        );
        let token = extract_token(&req);
        // Should fall back to admin
        assert_eq!(token.permission, auth::Permission::Admin);
    }

    #[test]
    fn extract_token_with_global_scope() {
        let scoped = auth::create_scoped_token(
            "admin-user",
            auth::TokenScope::Global,
            auth::Permission::Admin,
            None,
        );
        let bearer = auth::encode_scoped_token(&scoped);

        let mut req = tonic::Request::new(());
        req.metadata_mut().insert(
            "authorization",
            format!("Bearer {bearer}").parse().unwrap(),
        );
        let token = extract_token(&req);
        assert_eq!(token.subject, "admin-user");
        assert!(matches!(token.scope, auth::TokenScope::Global));
    }

    #[test]
    fn extract_token_no_bearer_prefix_falls_back() {
        let mut req = tonic::Request::new(());
        req.metadata_mut().insert(
            "authorization",
            "Basic dXNlcjpwYXNz".parse().unwrap(),
        );
        let token = extract_token(&req);
        // Not a Bearer token, should fall back to admin
        assert_eq!(token.permission, auth::Permission::Admin);
    }

    #[test]
    fn grpc_router_builds() {
        let _ = grpc_router();
    }
}
