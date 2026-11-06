#[tokio::main]
async fn main() -> anyhow::Result<()> {
    proxy_forward::cli::run_cli(std::env::args().skip(1).collect()).await
}
