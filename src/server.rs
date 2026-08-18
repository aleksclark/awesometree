use crate::auth;
use crate::log as dlog;
use crate::model::error::{ErrorCode, SwitchboardError};
use crate::model::lifecycle::WorkSessionState;
use crate::model::project::{definition_for_create, ProjectEnvelope};
use crate::model::work_session::{
    CreateWorkSessionRequest, CreateWorkSessionResponse, RealizationOptions, WorkSessionView,
};
use crate::model::WorkProfile;
use crate::service_access;
use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Json, Response};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::net::SocketAddr;
use std::sync::Arc;
use utoipa::{OpenApi, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

pub const DEFAULT_PORT: u16 = 9099;
pub const DEFAULT_GRPC_PORT: u16 = 9098;

#[derive(Clone)]
struct AppState {
    client: Arc<Client<hyper_util::client::legacy::connect::HttpConnector, Body>>,
}

#[derive(Serialize, ToSchema)]
struct ErrorBody {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

fn err(status: StatusCode, msg: impl Into<String>) -> Response {
    (
        status,
        Json(ErrorBody {
            error: msg.into(),
            code: None,
        }),
    )
        .into_response()
}

fn map_err(e: SwitchboardError) -> Response {
    let status = match e.code {
        ErrorCode::NotFound | ErrorCode::MissingDefaultProfile => StatusCode::NOT_FOUND,
        ErrorCode::AlreadyExists | ErrorCode::Conflict => StatusCode::CONFLICT,
        ErrorCode::InvalidInput
        | ErrorCode::InvalidReference
        | ErrorCode::InvalidTransition
        | ErrorCode::PolicyBroadening
        | ErrorCode::Referenced => StatusCode::BAD_REQUEST,
        ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
        ErrorCode::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    };
    (
        status,
        Json(ErrorBody {
            error: e.to_string(),
            code: Some(e.code.as_str().into()),
        }),
    )
        .into_response()
}

#[derive(Deserialize, ToSchema)]
struct CreateWorkSessionHttpReq {
    work_session_id: String,
    project_id: String,
    #[serde(default)]
    work_profile_id: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    headless: bool,
    #[serde(default)]
    no_tag: bool,
    #[serde(default)]
    no_launch: bool,
}

#[derive(Deserialize, ToSchema)]
struct TransitionReq {
    state: String,
}

#[derive(Deserialize, ToSchema)]
struct CreateProjectHttpReq {
    project_id: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    repo: Option<String>,
    #[serde(default)]
    branch: Option<String>,
    #[serde(default)]
    definition: Option<Value>,
}

#[derive(Deserialize, ToSchema)]
struct UpdateProjectHttpReq {
    expected_source_revision: String,
    #[serde(default)]
    patch: Option<Value>,
    #[serde(default)]
    definition: Option<Value>,
}

#[derive(Deserialize)]
struct ListFilter {
    state: Option<String>,
    project_id: Option<String>,
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "awesometree",
        description = "Agent Work Model host: Switchboard-backed Projects/WorkProfiles/WorkSessions + local Workspace realization",
        version = "0.2.0"
    ),
    tags(
        (name = "work-sessions", description = "WorkSession lifecycle and local realization"),
        (name = "work-profiles", description = "WorkProfile blueprints from Switchboard"),
        (name = "projects", description = "Project Catalog via Switchboard"),
        (name = "acp", description = "Agent Control Protocol proxy")
    )
)]
struct ApiDoc;

fn build_api_router() -> (axum::Router<AppState>, utoipa::openapi::OpenApi) {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(list_work_sessions))
        .routes(routes!(create_work_session))
        .routes(routes!(get_work_session))
        .routes(routes!(delete_work_session))
        .routes(routes!(transition_work_session))
        .routes(routes!(list_work_profiles))
        .routes(routes!(get_work_profile))
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

async fn auth_middleware(mut req: Request, next: Next) -> Result<Response, Response> {
    if std::env::var("ARP_DISABLE_AUTH").is_ok() {
        // Attach synthetic admin token for handlers that need it
        req.extensions_mut().insert(auth::localhost_admin_token());
        return Ok(next.run(req).await);
    }

    let is_local = req
        .extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip().is_loopback())
        .unwrap_or(false);

    if is_local {
        // Localhost callers get a synthetic admin/* token
        req.extensions_mut().insert(auth::localhost_admin_token());
        return Ok(next.run(req).await);
    }

    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    // Try scoped token first
    if let Some(scoped) = auth::resolve_token_from_header(auth_header) {
        req.extensions_mut().insert(scoped);
        return Ok(next.run(req).await);
    }

    // Fall back to legacy simple token
    let bearer = auth_header.and_then(|v| v.strip_prefix("Bearer "));
    match bearer {
        Some(token) if auth::validate_token(token) => {
            // Legacy token gets a synthetic admin/* token for backward compat
            req.extensions_mut().insert(auth::localhost_admin_token());
            Ok(next.run(req).await)
        }
        _ => Err(err(StatusCode::UNAUTHORIZED, "invalid or missing token")),
    }
}

pub async fn run(port: u16) {
    let client = Client::builder(TokioExecutor::new()).build_http();
    let client = Arc::new(client);
    let state = AppState {
        client: client.clone(),
    };

    let (router, api) = build_api_router();

    let spec = api.to_pretty_json().expect("OpenAPI JSON");

    let a2a_state = crate::a2a_proxy::A2aProxyState::with_client(client);
    let a2a_router = crate::a2a_proxy::router().with_state(a2a_state);

    // HTTP bridge for gRPC /v1/* routes (transcoding)
    let grpc_bridge = crate::grpc::http_bridge::router();

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
            "/api/acp/{workspace}/health",
            axum::routing::get(acp_health),
        )
        .route(
            "/api/acp/{workspace}/send",
            axum::routing::post(acp_send),
        )
        .route(
            "/api/acp/{workspace}/messages",
            axum::routing::get(acp_messages),
        )
        .route(
            "/api/acp/{workspace}/history",
            axum::routing::get(acp_history),
        )
        .route(
            "/api/acp/{workspace}/stream",
            axum::routing::post(acp_stream),
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
        .with_state(state)
        // Mount a2a and gRPC bridge routes outside the auth middleware layer.
        // These routes handle their own authentication: a2a handlers use
        // extract_token() which falls back to localhost_admin_token() for
        // unauthenticated local requests.
        .merge(a2a_router)
        .merge(grpc_bridge);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    dlog::log(format!("HTTP server listening on {addr}"));

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            dlog::log(format!("HTTP bind failed: {e}"));
            return;
        }
    };

    if let Err(e) = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    {
        dlog::log(format!("HTTP server error: {e}"));
    }
}

/// Start the gRPC server on the given port.
pub async fn run_grpc(port: u16) {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    dlog::log(format!("gRPC server listening on {addr}"));
    if let Err(e) = crate::grpc::grpc_router()
        .serve(addr)
        .await
    {
        dlog::log(format!("gRPC server error: {e}"));
    }
}

fn redact_runtime_secrets(mut view: WorkSessionView) -> WorkSessionView {
    // Never return raw bezalel tokens from list/detail APIs.
    if let Some(ref mut rt) = view.runtime {
        // token lives only in secrets store; token_ref is safe.
        let _ = rt;
    }
    view
}

#[utoipa::path(
    get,
    path = "/api/work-sessions",
    tag = "work-sessions",
    responses(
        (status = 200, description = "List WorkSessions"),
        (status = 503, description = "Switchboard unavailable", body = ErrorBody),
    )
)]
async fn list_work_sessions(
    Query(q): Query<ListFilter>,
) -> Result<Json<Vec<WorkSessionView>>, Response> {
    let svc = service_access::service().await;
    let list = svc
        .list_work_sessions(q.state.as_deref(), q.project_id.as_deref())
        .await
        .map_err(map_err)?;
    Ok(Json(
        list.into_iter().map(redact_runtime_secrets).collect(),
    ))
}

#[utoipa::path(
    get,
    path = "/api/work-sessions/{id}",
    tag = "work-sessions",
    params(("id" = String, Path, description = "work_session_id")),
    responses(
        (status = 200, description = "WorkSession detail"),
        (status = 404, description = "Not found", body = ErrorBody),
    )
)]
async fn get_work_session(Path(id): Path<String>) -> Result<Json<WorkSessionView>, Response> {
    let svc = service_access::service().await;
    let view = svc.get_work_session(&id).await.map_err(map_err)?;
    Ok(Json(redact_runtime_secrets(view)))
}

#[utoipa::path(
    post,
    path = "/api/work-sessions",
    tag = "work-sessions",
    request_body = CreateWorkSessionHttpReq,
    responses(
        (status = 201, description = "WorkSession created"),
        (status = 400, description = "Invalid request", body = ErrorBody),
        (status = 409, description = "Conflict", body = ErrorBody),
        (status = 503, description = "Switchboard unavailable", body = ErrorBody),
    )
)]
async fn create_work_session(
    Json(req): Json<CreateWorkSessionHttpReq>,
) -> Result<(StatusCode, Json<CreateWorkSessionResponse>), Response> {
    let svc = service_access::service().await;
    let create = CreateWorkSessionRequest {
        work_session_id: req.work_session_id,
        project_id: req.project_id,
        work_profile_id: req.work_profile_id,
        display_name: req.display_name,
        realization: RealizationOptions {
            create_tag: !req.no_tag && !req.headless,
            launch_apps: !req.no_launch && !req.headless,
            headless: req.headless,
            no_wm: req.headless,
        },
    };
    let resp = svc.create_work_session(create).await.map_err(map_err)?;
    dlog::log(format!(
        "API: created work_session {} profile={} state={}",
        resp.work_session.work_session_id,
        resp.work_profile_id,
        resp.work_session.state
    ));
    Ok((StatusCode::CREATED, Json(resp)))
}

#[utoipa::path(
    delete,
    path = "/api/work-sessions/{id}",
    tag = "work-sessions",
    params(("id" = String, Path, description = "work_session_id")),
    responses(
        (status = 204, description = "Deleted"),
        (status = 404, description = "Not found", body = ErrorBody),
    )
)]
async fn delete_work_session(Path(id): Path<String>) -> Result<StatusCode, Response> {
    let svc = service_access::service().await;
    svc.destroy(&id, false).await.map_err(map_err)?;
    dlog::log(format!("API: deleted work_session {id}"));
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/work-sessions/{id}/transition",
    tag = "work-sessions",
    params(("id" = String, Path, description = "work_session_id")),
    request_body = TransitionReq,
    responses(
        (status = 200, description = "Transitioned"),
        (status = 400, description = "Invalid transition", body = ErrorBody),
    )
)]
async fn transition_work_session(
    Path(id): Path<String>,
    Json(req): Json<TransitionReq>,
) -> Result<Json<WorkSessionView>, Response> {
    let state = WorkSessionState::parse(&req.state).ok_or_else(|| {
        err(
            StatusCode::BAD_REQUEST,
            format!("invalid state {:?}", req.state),
        )
    })?;
    let svc = service_access::service().await;
    let view = svc.transition(&id, state).await.map_err(map_err)?;
    Ok(Json(redact_runtime_secrets(view)))
}

#[utoipa::path(
    get,
    path = "/api/work-profiles",
    tag = "work-profiles",
    responses((status = 200, description = "List WorkProfiles"))
)]
async fn list_work_profiles() -> Result<Json<Vec<WorkProfile>>, Response> {
    let svc = service_access::service().await;
    svc.list_work_profiles().await.map(Json).map_err(map_err)
}

#[utoipa::path(
    get,
    path = "/api/work-profiles/{id}",
    tag = "work-profiles",
    params(("id" = String, Path, description = "work_profile_id")),
    responses((status = 200, description = "WorkProfile"))
)]
async fn get_work_profile(Path(id): Path<String>) -> Result<Json<WorkProfile>, Response> {
    let svc = service_access::service().await;
    svc.get_work_profile(&id).await.map(Json).map_err(map_err)
}

#[utoipa::path(
    get,
    path = "/api/projects",
    tag = "projects",
    responses((status = 200, description = "List projects from Switchboard"))
)]
async fn list_projects() -> Result<Json<Vec<crate::model::ProjectSummary>>, Response> {
    let svc = service_access::service().await;
    svc.list_projects(None).await.map(Json).map_err(map_err)
}

#[utoipa::path(
    get,
    path = "/api/projects/{id}",
    tag = "projects",
    params(("id" = String, Path, description = "project_id")),
    responses((status = 200, description = "Project envelope"))
)]
async fn get_project(Path(id): Path<String>) -> Result<Json<ProjectEnvelope>, Response> {
    let svc = service_access::service().await;
    svc.get_project(&id).await.map(Json).map_err(map_err)
}

#[utoipa::path(
    post,
    path = "/api/projects",
    tag = "projects",
    request_body = CreateProjectHttpReq,
    responses((status = 201, description = "Created"))
)]
async fn create_project(
    Json(req): Json<CreateProjectHttpReq>,
) -> Result<(StatusCode, Json<crate::model::ProjectSummary>), Response> {
    let def = req.definition.unwrap_or_else(|| {
        definition_for_create(
            &req.project_id,
            req.description.as_deref(),
            req.repo.as_deref(),
            req.branch.as_deref(),
            None,
        )
    });
    let svc = service_access::service().await;
    let summary = svc.create_project(def).await.map_err(map_err)?;
    dlog::log(format!("API: created project {}", summary.project_id));
    Ok((StatusCode::CREATED, Json(summary)))
}

#[utoipa::path(
    put,
    path = "/api/projects/{id}",
    tag = "projects",
    params(("id" = String, Path, description = "project_id")),
    request_body = UpdateProjectHttpReq,
    responses((status = 200, description = "Updated"))
)]
async fn update_project(
    Path(id): Path<String>,
    Json(req): Json<UpdateProjectHttpReq>,
) -> Result<Json<crate::model::ProjectSummary>, Response> {
    let patch = req
        .patch
        .or(req.definition)
        .ok_or_else(|| err(StatusCode::BAD_REQUEST, "patch or definition required"))?;
    let svc = service_access::service().await;
    svc.update_project(&id, &req.expected_source_revision, patch)
        .await
        .map(Json)
        .map_err(map_err)
}

#[utoipa::path(
    delete,
    path = "/api/projects/{id}",
    tag = "projects",
    params(("id" = String, Path, description = "project_id")),
    responses((status = 204, description = "Deleted"))
)]
async fn delete_project(
    Path(id): Path<String>,
    Query(q): Query<UpdateProjectHttpReq>,
) -> Result<StatusCode, Response> {
    if q.expected_source_revision.is_empty() {
        return Err(err(
            StatusCode::BAD_REQUEST,
            "expected_source_revision query param required",
        ));
    }
    let svc = service_access::service().await;
    svc.delete_project(&id, &q.expected_source_revision)
        .await
        .map_err(map_err)?;
    dlog::log(format!("API: deleted project {id}"));
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

fn resolve_acp_url(work_session_id: &str) -> Result<String, Response> {
    let rt = crate::runtime_store::get(work_session_id)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                format!("no local runtime for work_session: {work_session_id}"),
            )
        })?;

    if let Some(ref url) = rt.acp_url {
        return Ok(url.clone());
    }

    let port = rt.acp_port.ok_or_else(|| {
        err(
            StatusCode::BAD_GATEWAY,
            format!("work_session {work_session_id} has no ACP endpoint"),
        )
    })?;
    Ok(format!("http://127.0.0.1:{port}"))
}

fn acp_client(workspace: &str) -> Result<crush_acp_sdk::Client, Response> {
    let url = resolve_acp_url(workspace)?;
    Ok(crush_acp_sdk::Client::new(&url))
}

async fn proxy_to_acp(
    workspace: &str,
    rest: &str,
    req: Request,
    state: &AppState,
) -> Result<Response, Response> {
    let base_url = resolve_acp_url(workspace)?;

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

    let target_uri = format!("{base_url}{path}{query}");

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
                format!("ACP backend ({workspace}): {e}"),
            )
        })?;

    let (parts, body) = resp.into_parts();
    Ok(Response::from_parts(parts, Body::new(body)))
}

#[derive(Deserialize)]
struct AcpSendReq {
    message: String,
    #[serde(default)]
    session_id: Option<String>,
}

async fn acp_health(Path(workspace): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    let client = acp_client(&workspace)?;
    client
        .ping()
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("ACP ping failed: {e}")))?;
    Ok(Json(serde_json::json!({"status": "ok"})))
}

async fn acp_send(
    Path(workspace): Path<String>,
    Json(req): Json<AcpSendReq>,
) -> Result<Json<serde_json::Value>, Response> {
    let client = acp_client(&workspace)?;

    let result = if let Some(ref sid) = req.session_id {
        client.resume(sid, &req.message).await
    } else {
        client.new_session(&req.message).await
    };

    let session_result = result.map_err(|e| err(StatusCode::BAD_GATEWAY, format!("ACP error: {e}")))?;

    let session_id = session_result.run.as_ref().map(|r| r.session_id.clone());
    let text = session_result.text();
    let status = session_result.run.as_ref().map(|r| r.status.to_string());

    if let Some(ref sid) = session_id {
        let _ = save_session_id(&workspace, sid);
    }

    Ok(Json(serde_json::json!({
        "session_id": session_id,
        "text": text,
        "status": status,
    })))
}

async fn acp_messages(Path(workspace): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    let rt = crate::runtime_store::get(&workspace)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                format!("no local runtime for work_session: {workspace}"),
            )
        })?;

    let session_id = rt.acp_session_id.as_ref().ok_or_else(|| {
        err(
            StatusCode::NOT_FOUND,
            format!("no ACP session for work_session {workspace}"),
        )
    })?;

    let client = acp_client(&workspace)?;
    let snapshot = client
        .dump(session_id)
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("ACP dump failed: {e}")))?;

    Ok(Json(serde_json::to_value(&snapshot).unwrap_or_default()))
}

async fn acp_history(Path(workspace): Path<String>) -> Result<Json<serde_json::Value>, Response> {
    let rt = crate::runtime_store::get(&workspace)
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| {
            err(
                StatusCode::NOT_FOUND,
                format!("no local runtime for work_session: {workspace}"),
            )
        })?;

    let session_id = match rt.acp_session_id.as_ref() {
        Some(sid) => sid.clone(),
        None => return Ok(Json(serde_json::json!([]))),
    };

    let client = acp_client(&workspace)?;
    let snapshot = client
        .dump(&session_id)
        .await
        .map_err(|e| err(StatusCode::BAD_GATEWAY, format!("ACP dump failed: {e}")))?;

    let messages: Vec<serde_json::Value> = snapshot
        .messages
        .iter()
        .filter(|m| !m.is_summary_message)
        .filter_map(|m| {
            let parts: serde_json::Value = serde_json::from_str(&m.parts).ok()?;
            let text: String = parts
                .as_array()?
                .iter()
                .filter_map(|p| {
                    if p.get("type")?.as_str()? == "text" {
                        p.get("data")?.get("text")?.as_str().map(String::from)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("");
            if text.is_empty() {
                return None;
            }
            let role = match m.role.as_str() {
                "assistant" => "agent",
                other => other,
            };
            Some(serde_json::json!({"role": role, "content": text}))
        })
        .collect();

    Ok(Json(serde_json::json!(messages)))
}

async fn acp_stream(
    Path(workspace): Path<String>,
    Json(req): Json<AcpSendReq>,
) -> Result<Response, Response> {
    let client = acp_client(&workspace)?;

    let stream_result = if let Some(ref sid) = req.session_id {
        client.resume_stream(sid, &req.message).await
    } else {
        client.new_session_stream(&req.message).await
    };

    let mut acp_stream =
        stream_result.map_err(|e| err(StatusCode::BAD_GATEWAY, format!("ACP stream: {e}")))?;

    let ws_name = workspace.clone();
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(64);

    tokio::spawn(async move {
        use crush_acp_sdk::EventType;
        while let Some(event) = acp_stream.next().await {
            if let Some(ref run) = event.run {
                if !run.session_id.is_empty() {
                    let _ = save_session_id(&ws_name, &run.session_id);
                }
            }
            match event.event_type {
                EventType::SessionMessage | EventType::SessionSnapshot => continue,
                _ => {}
            }
            let line = serde_json::to_string(&event).unwrap_or_default();
            if tx.send(Ok(format!("{line}\n"))).await.is_err() {
                break;
            }
        }
    });

    let body_stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    let body = Body::from_stream(body_stream);

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/x-ndjson")
        .header("cache-control", "no-cache")
        .body(body)
        .unwrap())
}

fn save_session_id(work_session_id: &str, session_id: &str) -> Result<(), String> {
    crate::runtime_store::modify(work_session_id, |rt| {
        rt.acp_session_id = Some(session_id.to_string());
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_body_serializes() {
        let body = ErrorBody {
            error: "test".into(),
            code: Some("not_found".into()),
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"error\":\"test\""));
        assert!(json.contains("not_found"));
    }

    #[test]
    fn create_work_session_req_deserializes() {
        let json = r#"{"work_session_id":"ws-1","project_id":"proj"}"#;
        let req: CreateWorkSessionHttpReq = serde_json::from_str(json).unwrap();
        assert_eq!(req.work_session_id, "ws-1");
        assert_eq!(req.project_id, "proj");
        assert!(req.work_profile_id.is_none());
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
        assert!(json.contains("/api/work-sessions"));
        assert!(json.contains("/api/work-profiles"));
        assert!(json.contains("/api/projects"));
        assert!(json.contains("ErrorBody"));
        assert!(!json.contains("/api/workspaces"));
    }

    #[test]
    fn openapi_spec_has_all_paths() {
        let api = test_api();
        let json = api.to_pretty_json().unwrap();
        let spec: serde_json::Value = serde_json::from_str(&json).unwrap();
        let paths = spec["paths"].as_object().unwrap();
        assert!(paths.contains_key("/api/work-sessions"));
        assert!(paths.contains_key("/api/work-sessions/{id}"));
        assert!(paths.contains_key("/api/work-profiles"));
        assert!(paths.contains_key("/api/projects"));
        assert!(paths.contains_key("/api/projects/{id}"));
        assert!(!paths.contains_key("/api/workspaces"));
    }

    #[test]
    fn openapi_spec_has_tags() {
        let api = test_api();
        let json = api.to_pretty_json().unwrap();
        assert!(json.contains("work-sessions"));
        assert!(json.contains("work-profiles"));
        assert!(json.contains("projects"));
    }

    #[test]
    fn openapi_spec_has_correct_http_methods() {
        let api = test_api();
        let json = api.to_pretty_json().unwrap();
        let spec: serde_json::Value = serde_json::from_str(&json).unwrap();
        let paths = spec["paths"].as_object().unwrap();

        let ws_coll = paths["/api/work-sessions"].as_object().unwrap();
        assert!(ws_coll.contains_key("get"), "list work sessions");
        assert!(ws_coll.contains_key("post"), "create work session");

        let ws_item = paths["/api/work-sessions/{id}"].as_object().unwrap();
        assert!(ws_item.contains_key("get"), "get work session");
        assert!(ws_item.contains_key("delete"), "delete work session");

        let proj_coll = paths["/api/projects"].as_object().unwrap();
        assert!(proj_coll.contains_key("get"), "list projects");
        assert!(proj_coll.contains_key("post"), "create project");

        let proj_item = paths["/api/projects/{id}"].as_object().unwrap();
        assert!(proj_item.contains_key("get"), "get project");
        assert!(proj_item.contains_key("put"), "update project");
        assert!(proj_item.contains_key("delete"), "delete project");
    }

    #[test]
    fn openapi_spec_has_error_schema() {
        let api = test_api();
        let json = api.to_pretty_json().unwrap();
        let spec: serde_json::Value = serde_json::from_str(&json).unwrap();
        let schemas = spec["components"]["schemas"].as_object().unwrap();
        assert!(schemas.contains_key("ErrorBody"));
        assert!(schemas.contains_key("CreateWorkSessionHttpReq"));
    }

    #[test]
    fn openapi_public_fn_matches_router() {
        let from_fn = openapi_spec();
        let from_router = test_api().to_pretty_json().unwrap();
        assert_eq!(from_fn, from_router);
    }
}
