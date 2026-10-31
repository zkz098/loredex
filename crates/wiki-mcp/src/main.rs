#[tokio::main]
async fn main() -> anyhow::Result<()> {
    wiki_mcp::run(<wiki_mcp::Args as clap::Parser>::parse()).await
}
