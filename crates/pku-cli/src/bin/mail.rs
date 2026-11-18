#[tokio::main]
async fn main() -> anyhow::Result<()> {
    pku_mail::run().await
}
