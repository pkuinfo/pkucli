#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pkuinfo_spider::run().await
}
