use crate::auth;
use crate::interop::{self, Project};
use crate::log as dlog;
use crate::state::{self, Store};
use crate::workspace;
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Json, Response};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub const DEFAULT_PORT: u16 = 9099;

#[derive(Clone)]
struct AppState {
    client: Arc<Client<hyper_util::client::legacy::connect::HttpConnector, Body>>,
}

#[derive(Serialize, ToSchema)]
struct WorkspaceInfo {
    name: String,
    project: String,
    active: bool,
    tag_index: i32,
    dir: String,
    acp_port: Option<u16>,
}

#[derive(Serialize, ToSchema)]
struct ProjectInfo {
    name: String,
    repo: Option<String>,
    branch: Option<String>,
}

#[derive(Deserialize, ToSchema)]
struct CreateWorkspaceReq {
    name: String,
    project: String,
}

#[derive(Serialize, ToSchema)]
struct ErrorBody {
    error: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (status, Json(ErrorBody { error: msg.into() })).into_response()
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "awesometree",
        description = "Workspace manager REST API and Agent Control Protocol proxy",
        version = "0.1.0"
    ),
    tags(
        (name = "workspaces", description = "Workspace CRUD operations"),
        (name = "projects", description = "Project configuration CRUD"),
        (name = "acp", description = "Agent Control Protocol proxy")
    )
)]
struct ApiDoc;

fn build_api_router() -> (axum::Router<AppState>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(list_workspaces))
        .routes(routes!(create_workspace))
        .routes(routes!(get_workspace))
        .routes(routes!(delete_workspace))
        .routes(routes!(list_projects))
        .routes(routes!(create_project))
        .routes(routes!(get_project))
        .routes(routes!(update_project))
        .routes(routes!(delete_project))
        .split_for_parts()
}

pub fn openapi_spec() -> String {
    let (_, api) = build_api_router();
    api.to_pretty_json().expect("OpenAPI JSON serialization")
}

async fn auth_middleware(req: Request, next: Next) -> Result<Response, Response> {
    let is_local = req
        .extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().is_loopback())
        .unwrap_or(false);

    if is_local {
        return Ok(next.run(req).await);
    }

    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match auth_header {
        Some(token) if auth::validate_token(token) => Ok(next.run(req).await),
        _ => Err(err(StatusCode::UNAUTHORIZED, "invalid or missing token")),
    }
}

pub async fn run(port: u16) {
    let client = Client::builder(TokioExecutor::new()).build_http();
    let state = AppState {
        client: Arc::new(client),
    };

    let (router, api) = build_api_router();

    let spec = api.to_pretty_json().expect("OpenAPI JSON");

    let app = router
        .route(
            "/api/openapi.json",
            axum::routing::get(move || {
                let spec = spec.clone();
                async move {
                    (
                        [(axum::http::header::CONTENT_TYPE, "application/json")],
                        spec,
                    )
                }
            }),
        )
        .route(
            "/acp/{workspace}",
            axum::routing::any(acp_proxy),
        )
        .route(
            "/acp/{workspace}/{*rest}",
            axum::routing::any(acp_proxy_path),
        )
        .layer(middleware::from_fn(auth_middleware))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    dlog::log(format!("HTTP server listening on {addr}"));

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            dlog::log(format!("HTTP bind failed: {e}"));
            return;
        }
    };

    if let Err(e) = axum::serve(listener, app).await {
        dlog::log(format!("HTTP server error: {e}"));
    }
}

fn load_state() -> Result<Store, Response> {
    state::load().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))
}

fn ws_to_info(name: &str, ws: &state::WorkspaceState) -> WorkspaceInfo {
    WorkspaceInfo {
        name: name.to_string(),
        project: ws.project.clone(),
        active: ws.active,
        tag_index: ws.tag_index,
        dir: ws.dir.clone(),
        acp_port: ws.acp_port,
    }
}

#[utoipa::path(
    get,
    path = "/api/workspaces",
    tag = "workspaces",
    responses(
        (status = 200, description = "List all workspaces", body = Vec<WorkspaceInfo>),
        (status = 500, description = "Internal error", body = ErrorBody),
    )
)]
async fn list_workspaces() -> Result<Json<Vec<WorkspaceInfo>>, Response> {
    let st = load_state()?;
    let mut list: Vec<WorkspaceInfo> = st
        .workspaces
        .iter()
        .map(|(name, ws)| ws_to_info(name, ws))
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Json(list))
}

#[utoipa::path(
    get,
    path = "/api/workspaces/{name}",
    tag = "workspaces",
    params(
        ("name" = String, Path, description = "Workspace name"),
    ),
    responses(
        (status = 200, description = "Workspace details", body = WorkspaceInfo),
        (status = 404, description = "Workspace not found", body = ErrorBody),
    )
)]
async fn get_workspace(Path(name): Path<String>) -> Result<Json<WorkspaceInfo>, Response> {
    let st = load_state()?;
    let ws = st
        .workspace(&name)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, format!("workspace not found: {name}")))?;
    Ok(Json(ws_to_info(&name, ws)))
}

#[utoipa::path(
    post,
    path = "/api/workspaces",
    tag = "workspaces",
    request_body = CreateWorkspaceReq,
    responses(
        (status = 201, description = "Workspace created", body = WorkspaceInfo),
        (status = 400, description = "Invalid project", body = ErrorBody),
        (status = 409, description = "Workspace already exists", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    )
)]
async fn create_workspace(
    Json(req): Json<CreateWorkspaceReq>,
) -> Result<(StatusCode, Json<WorkspaceInfo>), Response> {
    let mut st = load_state()?;

    if st.workspace(&req.name).is_some() {
        return Err(err(
            StatusCode::CONFLICT,
            format!("workspace already exists: {}", req.name),
        ));
    }

    let project = interop::load(&req.project)
        .map_err(|e| err(StatusCode::BAD_REQUEST, e))?;

    let dir = workspace::resolve_dir(&req.name, &project);
    workspace::ensure_worktree(&req.name, &project, &dir)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let tag_idx = st.allocate_tag_index(&req.name);
    let acp_port = st.allocate_acp_port(&req.name);
    let dir_str = dir.to_string_lossy().into_owned();
    st.set_active(&req.name, &req.project, tag_idx, &dir_str, acp_port);
    state::save(&st).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    dlog::log(format!(
        "API: created workspace {} (project: {}, acp_port: {:?})",
        req.name, req.project, acp_port
    ));

    let ws = st.workspace(&req.name).unwrap();
    Ok((StatusCode::CREATED, Json(ws_to_info(&req.name, ws))))
}

#[utoipa::path(
    delete,
    path = "/api/workspaces/{name}",
    tag = "workspaces",
    params(
        ("name" = String, Path, description = "Workspace name"),
    ),
    responses(
        (status = 204, description = "Workspace deleted"),
        (status = 404, description = "Workspace not found", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    )
)]
async fn delete_workspace(Path(name): Path<String>) -> Result<StatusCode, Response> {
    let mut st = load_state()?;

    if st.workspace(&name).is_none() {
        return Err(err(
            StatusCode::NOT_FOUND,
            format!("workspace not found: {name}"),
        ));
    }

    st.remove(&name);
    state::save(&st).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;

    dlog::log(format!("API: deleted workspace {name}"));
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/projects",
    tag = "projects",
    responses(
        (status = 200, description = "List all projects", body = Vec<ProjectInfo>),
        (status = 500, description = "Internal error", body = ErrorBody),
    )
)]
async fn list_projects() -> Result<Json<Vec<ProjectInfo>>, Response> {
    let projects = interop::list().map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let list: Vec<ProjectInfo> = projects
        .iter()
        .map(|p| ProjectInfo {
            name: p.name.clone(),
            repo: p.repo.clone(),
            branch: p.branch.clone(),
        })
        .collect();
    Ok(Json(list))
}

#[utoipa::path(
    get,
    path = "/api/projects/{name}",
    tag = "projects",
    params(
        ("name" = String, Path, description = "Project name"),
    ),
    responses(
        (status = 200, description = "Full project configuration", body = Project),
        (status = 404, description = "Project not found", body = ErrorBody),
    )
)]
async fn get_project(Path(name): Path<String>) -> Result<Json<Project>, Response> {
    let project =
        interop::load(&name).map_err(|e| err(StatusCode::NOT_FOUND, e))?;
    Ok(Json(project))
}

#[utoipa::path(
    post,
    path = "/api/projects",
    tag = "projects",
    request_body = Project,
    responses(
        (status = 201, description = "Project created", body = Project),
        (status = 400, description = "Invalid request", body = ErrorBody),
        (status = 409, description = "Project already exists", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    )
)]
async fn create_project(
    Json(project): Json<Project>,
) -> Result<(StatusCode, Json<Project>), Response> {
    if project.name.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "project name is required"));
    }
    if interop::load(&project.name).is_ok() {
        return Err(err(
            StatusCode::CONFLICT,
            format!("project already exists: {}", project.name),
        ));
    }
    interop::save(&project).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    dlog::log(format!("API: created project {}", project.name));
    Ok((StatusCode::CREATED, Json(project)))
}

#[utoipa::path(
    put,
    path = "/api/projects/{name}",
    tag = "projects",
    params(
        ("name" = String, Path, description = "Project name"),
    ),
    request_body = Project,
    responses(
        (status = 200, description = "Project updated", body = Project),
        (status = 404, description = "Project not found", body = ErrorBody),
        (status = 500, description = "Internal error", body = ErrorBody),
    )
)]
async fn update_project(
    Path(name): Path<String>,
    Json(mut project): Json<Project>,
) -> Result<Json<Project>, Response> {
    interop::load(&name).map_err(|e| err(StatusCode::NOT_FOUND, e))?;
    project.name = name;
    interop::save(&project).map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e))?;
    dlog::log(format!("API: updated project {}", project.name));
    Ok(Json(project))
}

#[utoipa::path(
    delete,
    path = "/api/projects/{name}",
    tag = "projects",
    params(
        ("name" = String, Path, description = "Project name"),
    ),
    responses(
        (status = 204, description = "Project deleted"),
        (status = 404, description = "Project not found", body = ErrorBody),
    )
)]
async fn delete_project(Path(name): Path<String>) -> Result<StatusCode, Response> {
    interop::delete(&name).map_err(|e| err(StatusCode::NOT_FOUND, e))?;
    dlog::log(format!("API: deleted project {name}"));
    Ok(StatusCode::NO_CONTENT)
}

async fn acp_proxy(
    Path(workspace): Path<String>,
    State(state): State<AppState>,
    req: Request,
) -> Result<Response, Response> {
    proxy_to_acp(&workspace, "", req, &state).await
}

async fn acp_proxy_path(
    Path((workspace, rest)): Path<(String, String)>,
    State(state): State<AppState>,
    req: Request,
) -> Result<Response, Response> {
    proxy_to_acp(&workspace, &rest, req, &state).await
}

async fn proxy_to_acp(
    workspace: &str,
    rest: &str,
    req: Request,
    state: &AppState,
) -> Result<Response, Response> {
    let st = load_state()?;
    let (_, ws) = st
        .workspace_name_for_route(workspace)
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                format!("no active workspace: {workspace}"),
            )
        })?;

    let port = ws.acp_port.ok_or_else(|| {
        err(
            StatusCode::BAD_GATEWAY,
            format!("workspace {workspace} has no ACP port"),
        )
    })?;

    let path = if rest.is_empty() {
        String::new()
    } else {
        format!("/{rest}")
    };

    let query = req
        .uri()
        .query()
        .map(|q| format!("?{q}"))
        .unwrap_or_default();

    let target_uri = format!("http://127.0.0.1:{port}{path}{query}");

    let (parts, body) = req.into_parts();
    let mut builder = hyper::Request::builder()
        .method(parts.method)
        .uri(&target_uri);

    for (key, value) in &parts.headers {
        if key != "host" {
            builder = builder.header(key, value);
        }
    }

    let proxy_req = builder
        .body(body)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, format!("build request: {e}")))?;

    let resp = state
        .client
        .request(proxy_req)
        .await
        .map_err(|e| {
            err(
                StatusCode::BAD_GATEWAY,
                format!("ACP backend ({workspace} @ port {port}): {e}"),
            )
        })?;

    let (parts, body) = resp.into_parts();
    Ok(Response::from_parts(parts, Body::new(body)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_body_serializes() {
        let body = ErrorBody {
            error: "test".into(),
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"error\":\"test\""));
    }

    #[test]
    fn workspace_info_serializes() {
        let info = WorkspaceInfo {
            name: "feat-1".into(),
            project: "proj".into(),
            active: true,
            tag_index: 10,
            dir: "/tmp".into(),
            acp_port: Some(9100),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"acp_port\":9100"));
        assert!(json.contains("\"active\":true"));
    }

    #[test]
    fn project_info_serializes() {
        let info = ProjectInfo {
            name: "proj".into(),
            repo: Some("/repo".into()),
            branch: Some("main".into()),
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("\"name\":\"proj\""));
    }

    #[test]
    fn create_workspace_req_deserializes() {
        let json = r#"{"name":"feat-1","project":"proj"}"#;
        let req: CreateWorkspaceReq = serde_json::from_str(json).unwrap();
        assert_eq!(req.name, "feat-1");
        assert_eq!(req.project, "proj");
    }

    #[test]
    fn default_port_is_expected() {
        assert_eq!(DEFAULT_PORT, 9099);
    }

    fn test_api() -> utoipa::openapi::OpenApi {
        let (_, api) = build_api_router();
        api
    }

    #[test]
    fn openapi_spec_generates() {
        let api = test_api();
        let json = api.to_pretty_json().unwrap();
        assert!(json.contains("\"openapi\""));
        assert!(json.contains("awesometree"));
        assert!(json.contains("/api/workspaces"));
        assert!(json.contains("/api/projects"));
        assert!(json.contains("WorkspaceInfo"));
        assert!(json.contains("CreateWorkspaceReq"));
        assert!(json.contains("ProjectInfo"));
        assert!(json.contains("ErrorBody"));
    }

    #[test]
    fn openapi_spec_has_all_paths() {
        let api = test_api();
        let json = api.to_pretty_json().unwrap();
        let spec: serde_json::Value = serde_json::from_str(&json).unwrap();
        let paths = spec["paths"].as_object().unwrap();
        assert!(paths.contains_key("/api/workspaces"));
        assert!(paths.contains_key("/api/workspaces/{name}"));
        assert!(paths.contains_key("/api/projects"));
        assert!(paths.contains_key("/api/projects/{name}"));
    }

    #[test]
    fn openapi_spec_has_tags() {
        let api = test_api();
        let json = api.to_pretty_json().unwrap();
        assert!(json.contains("\"workspaces\""));
        assert!(json.contains("\"projects\""));
    }

    #[test]
    fn openapi_spec_has_correct_http_methods() {
        let api = test_api();
        let json = api.to_pretty_json().unwrap();
        let spec: serde_json::Value = serde_json::from_str(&json).unwrap();
        let paths = spec["paths"].as_object().unwrap();

        let ws_coll = paths["/api/workspaces"].as_object().unwrap();
        assert!(ws_coll.contains_key("get"), "list workspaces");
        assert!(ws_coll.contains_key("post"), "create workspace");

        let ws_item = paths["/api/workspaces/{name}"].as_object().unwrap();
        assert!(ws_item.contains_key("get"), "get workspace");
        assert!(ws_item.contains_key("delete"), "delete workspace");

        let proj_coll = paths["/api/projects"].as_object().unwrap();
        assert!(proj_coll.contains_key("get"), "list projects");
        assert!(proj_coll.contains_key("post"), "create project");

        let proj_item = paths["/api/projects/{name}"].as_object().unwrap();
        assert!(proj_item.contains_key("get"), "get project");
        assert!(proj_item.contains_key("put"), "update project");
        assert!(proj_item.contains_key("delete"), "delete project");
    }

    #[test]
    fn openapi_spec_has_all_schemas() {
        let api = test_api();
        let json = api.to_pretty_json().unwrap();
        let spec: serde_json::Value = serde_json::from_str(&json).unwrap();
        let schemas = spec["components"]["schemas"].as_object().unwrap();
        for expected in &[
            "WorkspaceInfo",
            "CreateWorkspaceReq",
            "ProjectInfo",
            "ErrorBody",
            "Project",
            "Launch",
            "ContextConfig",
        ] {
            assert!(schemas.contains_key(*expected), "missing schema: {expected}");
        }
    }

    #[test]
    fn openapi_spec_workspace_info_fields() {
        let api = test_api();
        let json = api.to_pretty_json().unwrap();
        let spec: serde_json::Value = serde_json::from_str(&json).unwrap();
        let props = spec["components"]["schemas"]["WorkspaceInfo"]["properties"]
            .as_object()
            .unwrap();
        for field in &["name", "project", "active", "tag_index", "dir", "acp_port"] {
            assert!(props.contains_key(*field), "WorkspaceInfo missing field: {field}");
        }
    }

    #[test]
    fn openapi_public_fn_matches_router() {
        let from_fn = openapi_spec();
        let from_router = test_api().to_pretty_json().unwrap();
        assert_eq!(from_fn, from_router);
    }
}
