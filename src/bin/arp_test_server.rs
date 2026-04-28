use awesometree::acp_supervisor;
use awesometree::agent_supervisor;
use awesometree::log as dlog;
use awesometree::server;
use std::time::Duration;

fn main() {
    let port: u16 = std::env::var("ARP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(server::DEFAULT_PORT);

    eprintln!("arp-test-server starting on port {port}");

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    acp_supervisor::init(rt.handle().clone());
    agent_supervisor::init(rt.handle().clone());

    rt.block_on(async {
        acp_supervisor::start_active_workspaces();
        acp_supervisor::start_sync_loop(Duration::from_secs(5));
        dlog::log(format!("ARP test server listening on port {port}"));
        server::run(port).await;
    });
}
