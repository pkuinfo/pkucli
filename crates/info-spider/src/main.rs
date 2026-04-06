mod articles;
mod cli;
mod client;
mod login;
mod scrape;
mod search;
mod session;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();

    let cli = Cli::parse();
    match cli.command {
        Commands::Login => login::run().await?,
        Commands::Logout => login::logout()?,
        Commands::Status => login::status()?,
        Commands::Search {
            query,
            count,
            format,
        } => search::run(query, count, format).await?,
        Commands::Articles {
            name,
            fakeid,
            begin,
            count,
            limit,
            delay_ms,
            format,
        } => {
            articles::run(name, fakeid, begin, count, limit, delay_ms, format).await?;
        }
        Commands::Scrape { url, output } => scrape::run(url, output).await?,
    }
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info_spider=info,warn"));
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .init();
}
