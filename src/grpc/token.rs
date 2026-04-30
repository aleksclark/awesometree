//! gRPC TokenService implementation.

use crate::auth;
use crate::grpc::arp_proto::*;
use crate::grpc::arp_proto::token_service_server::TokenService;
use crate::grpc::extract_token;
use tonic::{Request, Response, Status};

/// Implements the `TokenService` gRPC trait.
#[derive(Debug, Default)]
pub struct TokenServiceImpl;

#[tonic::async_trait]
impl TokenService for TokenServiceImpl {
    async fn create_token(
        &self,
        request: Request<CreateTokenRequest>,
    ) -> Result<Response<CreateTokenResponse>, Status> {
        let caller_token = extract_token(&request);
        let req = request.into_inner();

        // Require admin permission to create tokens
        if !auth::permission_allows(&caller_token.permission, &auth::Permission::Admin) {
            return Err(Status::permission_denied("admin permission required to create tokens"));
        }

        // Parse scope from request
        let scope = match req.scope {
            Some(proto_scope) => {
                if proto_scope.global {
                    auth::TokenScope::Global
                } else if proto_scope.projects.is_empty() {
                    // No projects specified and not global — use caller's scope
                    caller_token.scope.clone()
                } else {
                    auth::TokenScope::Projects(proto_scope.projects)
                }
            }
            None => caller_token.scope.clone(),
        };

        // Parse permission from request
        let permission = match Permission::try_from(req.permission) {
            Ok(Permission::Admin) => auth::Permission::Admin,
            Ok(Permission::Project) => auth::Permission::Project,
            Ok(Permission::Session) => auth::Permission::Session,
            _ => {
                // Default to caller's permission level
                caller_token.permission.clone()
            }
        };

        // Validate that the new token doesn't escalate beyond caller's scope/permission
        // (create_scoped_token doesn't enforce this, so we check manually)
        if !auth::permission_allows(&caller_token.permission, &permission) {
            return Err(Status::permission_denied(
                "cannot create token with higher permission than caller",
            ));
        }

        // Validate scope doesn't widen beyond caller's scope
        match &caller_token.scope {
            auth::TokenScope::Global => {
                // Global scope allows any child scope
            }
            auth::TokenScope::Projects(caller_projects) => {
                if let auth::TokenScope::Global = scope {
                    return Err(Status::permission_denied(
                        "cannot create global-scope token from project-scoped caller",
                    ));
                }
                if let auth::TokenScope::Projects(ref requested_projects) = scope {
                    for p in requested_projects {
                        if !caller_projects.contains(p) {
                            return Err(Status::permission_denied(format!(
                                "cannot include project '{}' not in caller's scope",
                                p
                            )));
                        }
                    }
                }
            }
        }

        let subject = if req.subject.is_empty() {
            caller_token.subject.clone()
        } else {
            req.subject
        };

        let expires_in = if req.expires_in_seconds > 0 {
            Some(req.expires_in_seconds as u64)
        } else {
            None
        };

        let scoped_token = auth::create_scoped_token(&subject, scope, permission, expires_in);
        let bearer_token = auth::encode_scoped_token(&scoped_token);

        // Convert to proto Token
        let issued_at = chrono::DateTime::parse_from_rfc3339(&scoped_token.issued_at)
            .ok()
            .map(|dt| prost_types::Timestamp {
                seconds: dt.timestamp(),
                nanos: dt.timestamp_subsec_nanos() as i32,
            });

        let expires_at = scoped_token
            .expires_at
            .as_ref()
            .and_then(|ea| chrono::DateTime::parse_from_rfc3339(ea).ok())
            .map(|dt| prost_types::Timestamp {
                seconds: dt.timestamp(),
                nanos: dt.timestamp_subsec_nanos() as i32,
            });

        let proto_scope = match &scoped_token.scope {
            auth::TokenScope::Global => Scope {
                global: true,
                projects: Vec::new(),
            },
            auth::TokenScope::Projects(projects) => Scope {
                global: false,
                projects: projects.clone(),
            },
        };

        let proto_permission = match scoped_token.permission {
            auth::Permission::Admin => Permission::Admin as i32,
            auth::Permission::Project => Permission::Project as i32,
            auth::Permission::Session => Permission::Session as i32,
        };

        let proto_token = Token {
            id: scoped_token.id,
            subject: scoped_token.subject,
            scope: Some(proto_scope),
            permission: proto_permission,
            session_id: scoped_token.session_id.unwrap_or_default(),
            issued_at,
            expires_at,
            parent_token_id: scoped_token.parent_token_id.unwrap_or_default(),
        };

        Ok(Response::new(CreateTokenResponse {
            token: Some(proto_token),
            bearer_token,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proto_permission_values() {
        // Verify the proto enum values match what we expect
        assert_eq!(Permission::Session as i32, 1);
        assert_eq!(Permission::Project as i32, 2);
        assert_eq!(Permission::Admin as i32, 3);
    }

    #[test]
    fn proto_scope_global() {
        let scope = Scope {
            global: true,
            projects: vec![],
        };
        assert!(scope.global);
        assert!(scope.projects.is_empty());
    }

    #[test]
    fn proto_scope_projects() {
        let scope = Scope {
            global: false,
            projects: vec!["proj-a".into(), "proj-b".into()],
        };
        assert!(!scope.global);
        assert_eq!(scope.projects.len(), 2);
        assert_eq!(scope.projects[0], "proj-a");
    }

    #[test]
    fn proto_permission_try_from() {
        assert_eq!(Permission::try_from(1), Ok(Permission::Session));
        assert_eq!(Permission::try_from(2), Ok(Permission::Project));
        assert_eq!(Permission::try_from(3), Ok(Permission::Admin));
        assert!(Permission::try_from(99).is_err());
    }

    #[test]
    fn token_service_impl_is_default() {
        let _svc = TokenServiceImpl::default();
    }
}
