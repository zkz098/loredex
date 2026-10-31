#[tokio::main]
async fn main() -> anyhow::Result<()> {
    loredex_mcp::run(<loredex_mcp::Args as clap::Parser>::parse()).await
}
