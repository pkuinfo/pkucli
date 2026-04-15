#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pku_campuscard::run().await
}
