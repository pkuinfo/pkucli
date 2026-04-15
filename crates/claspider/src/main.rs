#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pku_claspider::run().await
}
