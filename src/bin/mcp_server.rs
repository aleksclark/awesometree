use awesometree::mcp;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    mcp::run_stdio().await
}
