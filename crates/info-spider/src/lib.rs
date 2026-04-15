mod articles;
pub mod cli;
mod client;
mod login;
mod scrape;
mod search;
mod session;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Commands};

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info_spider=info,warn"));
    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .init();
}

pub async fn run() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();
    dispatch(cli.command).await
}

pub async fn run_from<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::try_parse_from(args)?;
    dispatch(cli.command).await
}

pub async fn dispatch(command: Commands) -> Result<()> {
    match command {
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
