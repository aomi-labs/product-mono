mod cli;
mod commands;
mod db;
mod util;

use anyhow::Result;
use clap::Parser;

use crate::cli::{Cli, Command};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let database_url = db::resolve_database_url(cli.database_url);
    let pool = db::connect_database(&database_url).await?;

    match cli.command {
        Command::ApiKeys(cmd) => commands::handle_api_keys(cmd.command, &pool).await?,
        Command::Users(cmd) => commands::handle_users(cmd.command, &pool).await?,
        Command::Sessions(cmd) => commands::handle_sessions(cmd.command, &pool).await?,
        Command::Contracts(cmd) => commands::handle_contracts(cmd.command, &pool).await?,
    }

    Ok(())
}
