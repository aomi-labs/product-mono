mod api_keys;
mod contracts;
mod sessions;
mod users;

use anyhow::Result;
use sqlx::AnyPool;

use crate::cli::{ApiKeysCommand, ContractsCommand, SessionsCommand, UsersCommand};

pub async fn handle_api_keys(cmd: ApiKeysCommand, pool: &AnyPool) -> Result<()> {
    match cmd {
        ApiKeysCommand::Create(args) => api_keys::create_api_key(args, pool).await,
        ApiKeysCommand::List(args) => api_keys::list_api_keys(args, pool).await,
        ApiKeysCommand::Update(args) => api_keys::update_api_key(args, pool).await,
    }
}

pub async fn handle_users(cmd: UsersCommand, pool: &AnyPool) -> Result<()> {
    match cmd {
        UsersCommand::List(args) => users::list_users(args, pool).await,
        UsersCommand::Update(args) => users::update_user(args, pool).await,
        UsersCommand::Delete(args) => users::delete_user(args, pool).await,
    }
}

pub async fn handle_sessions(cmd: SessionsCommand, pool: &AnyPool) -> Result<()> {
    match cmd {
        SessionsCommand::List(args) => sessions::list_sessions(args, pool).await,
        SessionsCommand::Update(args) => sessions::update_session(args, pool).await,
        SessionsCommand::Delete(args) => sessions::delete_session(args, pool).await,
    }
}

pub async fn handle_contracts(cmd: ContractsCommand, pool: &AnyPool) -> Result<()> {
    match cmd {
        ContractsCommand::List(args) => contracts::list_contracts(args, pool).await,
        ContractsCommand::Update(args) => contracts::update_contract(args, pool).await,
        ContractsCommand::Delete(args) => contracts::delete_contract(args, pool).await,
    }
}
