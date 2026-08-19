//! Process-wide WorkSessionService accessor used by CLI, REST, MCP, and gRPC.

use crate::switchboard::SwitchboardClient;
use crate::work_session_service::WorkSessionService;
#[cfg(feature = "gui")]
use crate::wm;
use std::sync::{Arc, OnceLock};
use tokio::sync::Mutex;

static SERVICE: OnceLock<Mutex<Option<Arc<WorkSessionService>>>> = OnceLock::new();

fn cell() -> &'static Mutex<Option<Arc<WorkSessionService>>> {
    SERVICE.get_or_init(|| Mutex::new(None))
}

/// Install a service instance (tests / custom wiring).
pub async fn set_service(svc: Arc<WorkSessionService>) {
    let mut g = cell().lock().await;
    *g = Some(svc);
}

/// Get or create the production service backed by Switchboard + platform WM.
pub async fn service() -> Arc<WorkSessionService> {
    let mut g = cell().lock().await;
    if let Some(svc) = g.as_ref() {
        return svc.clone();
    }
    let catalog = Arc::new(SwitchboardClient::from_env());
    #[cfg(feature = "gui")]
    let wm_box = Some(wm::platform_adapter());
    #[cfg(not(feature = "gui"))]
    let wm_box = None;
    let svc = Arc::new(WorkSessionService::new(catalog, wm_box));
    *g = Some(svc.clone());
    svc
}

/// Blocking helper for sync CLI paths.
pub fn service_blocking() -> Arc<WorkSessionService> {
    let rt = tokio::runtime::Handle::try_current();
    match rt {
        Ok(h) => tokio::task::block_in_place(|| h.block_on(service())),
        Err(_) => {
            let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
            rt.block_on(service())
        }
    }
}
