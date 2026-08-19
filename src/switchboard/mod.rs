//! Production Switchboard MCP client (streamable HTTP).
//!
//! Never shells out or reads Switchboard files. Hard-fails when unavailable.

mod client;
mod config;

pub use client::SwitchboardClient;
pub use config::{switchboard_endpoint, SwitchboardConfig};

use crate::model::{
    ProjectEnvelope, ProjectSummary, SwitchboardError, WorkProfile, WorkSession, WorkSessionState,
};
use async_trait::async_trait;
use serde_json::Value;

/// Repository trait for Switchboard-backed authoritative entities.
/// Production always uses [`SwitchboardClient`].
#[async_trait]
pub trait Catalog: Send + Sync {
    async fn health(&self) -> Result<(), SwitchboardError>;

    async fn list_projects(&self, query: Option<&str>) -> Result<Vec<ProjectSummary>, SwitchboardError>;
    async fn get_project(&self, project_id: &str) -> Result<ProjectEnvelope, SwitchboardError>;
    async fn create_project(&self, definition: Value) -> Result<ProjectSummary, SwitchboardError>;
    /// `body` is a Switchboard field merge-patch (e.g. `{"description":"x"}`).
    /// Callers that have a full definition should use [`Catalog::replace_project_definition`].
    async fn update_project(
        &self,
        project_id: &str,
        expected_source_revision: &str,
        patch: Value,
    ) -> Result<ProjectSummary, SwitchboardError>;
    /// Full definition replace via Switchboard `project_update.definition`.
    async fn replace_project_definition(
        &self,
        project_id: &str,
        expected_source_revision: &str,
        definition: Value,
    ) -> Result<ProjectSummary, SwitchboardError>;
    async fn delete_project(
        &self,
        project_id: &str,
        expected_source_revision: &str,
    ) -> Result<(), SwitchboardError>;

    async fn list_work_profiles(&self) -> Result<Vec<WorkProfile>, SwitchboardError>;
    async fn get_work_profile(&self, id: &str) -> Result<WorkProfile, SwitchboardError>;
    async fn put_work_profile(&self, profile: WorkProfile) -> Result<WorkProfile, SwitchboardError>;
    async fn delete_work_profile(&self, id: &str) -> Result<(), SwitchboardError>;

    async fn list_work_sessions(
        &self,
        state: Option<&str>,
        project_id: Option<&str>,
    ) -> Result<Vec<WorkSession>, SwitchboardError>;
    async fn get_work_session(&self, id: &str) -> Result<WorkSession, SwitchboardError>;
    async fn create_work_session(&self, session: WorkSession) -> Result<WorkSession, SwitchboardError>;
    async fn transition_work_session(
        &self,
        id: &str,
        state: WorkSessionState,
    ) -> Result<WorkSession, SwitchboardError>;
    async fn patch_work_session(
        &self,
        id: &str,
        display_name: Option<String>,
        policy: Option<Value>,
    ) -> Result<WorkSession, SwitchboardError>;
    async fn delete_work_session(&self, id: &str) -> Result<(), SwitchboardError>;
}

/// Production catalog handle (Arc) for daemon/service wiring.
pub fn live_catalog() -> std::sync::Arc<dyn Catalog> {
    std::sync::Arc::new(SwitchboardClient::from_env())
}
