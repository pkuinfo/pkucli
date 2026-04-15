#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pku_cwfw::run().await
}
