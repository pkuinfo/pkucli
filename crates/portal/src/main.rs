#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pku_portal::run().await
}
