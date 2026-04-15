#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pku_elective::run().await
}
