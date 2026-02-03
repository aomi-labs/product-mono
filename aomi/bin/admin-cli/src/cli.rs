use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(name = "admin-cli")]
#[command(about = "Admin CLI for database operations")]
pub struct Cli {
    /// Database connection string (falls back to DATABASE_URL or a local default)
    #[arg(short = 'd', long, env = "DATABASE_URL")]
    pub database_url: Option<String>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    ApiKeys(ApiKeysArgs),
    Users(UsersArgs),
    Sessions(SessionsArgs),
    Contracts(ContractsArgs),
}

#[derive(Args)]
pub struct ApiKeysArgs {
    #[command(subcommand)]
    pub command: ApiKeysCommand,
}

#[derive(Args)]
pub struct UsersArgs {
    #[command(subcommand)]
    pub command: UsersCommand,
}

#[derive(Args)]
pub struct SessionsArgs {
    #[command(subcommand)]
    pub command: SessionsCommand,
}

#[derive(Args)]
pub struct ContractsArgs {
    #[command(subcommand)]
    pub command: ContractsCommand,
}

#[derive(Subcommand)]
pub enum ApiKeysCommand {
    Create(ApiKeyCreateArgs),
    List(ApiKeyListArgs),
    Update(ApiKeyUpdateArgs),
}

#[derive(Subcommand)]
pub enum UsersCommand {
    List(UserListArgs),
    Update(UserUpdateArgs),
    Delete(UserDeleteArgs),
}

#[derive(Subcommand)]
pub enum SessionsCommand {
    List(SessionListArgs),
    Update(SessionUpdateArgs),
    Delete(SessionDeleteArgs),
}

#[derive(Subcommand)]
pub enum ContractsCommand {
    List(ContractListArgs),
    Update(ContractUpdateArgs),
    Delete(ContractDeleteArgs),
}

#[derive(Args, Clone)]
pub struct ApiKeyCreateArgs {
    /// Namespaces for this key (can be specified multiple times, e.g. -n ns1 -n ns2)
    #[arg(short = 'n', long, alias = "chatbots", required = true)]
    pub namespaces: Option<Vec<String>>,

    /// Optional label for the key
    #[arg(short = 'l', long)]
    pub label: Option<String>,

    /// Provide your own API key (otherwise a random key is generated)
    #[arg(short = 'k', long)]
    pub key: Option<String>,
}

#[derive(Args, Clone)]
pub struct ApiKeyListArgs {
    /// Only include active keys
    #[arg(short = 'a', long)]
    pub active_only: bool,

    /// Max rows to return
    #[arg(short = 'l', long)]
    pub limit: Option<i64>,

    /// Offset for pagination
    #[arg(short = 'o', long)]
    pub offset: Option<i64>,
}

#[derive(Args, Clone)]
pub struct ApiKeyUpdateArgs {
    /// API key value to update
    #[arg(short = 'k', long)]
    pub api_key: String,

    /// Update label
    #[arg(short = 'l', long)]
    pub label: Option<String>,

    /// Clear label (set to NULL)
    #[arg(short = 'L', long)]
    pub clear_label: bool,

    /// Replace allowed namespaces (can be specified multiple times, e.g. -n ns1 -n ns2)
    #[arg(short = 'n', long, alias = "chatbots")]
    pub namespaces: Option<Vec<String>>,

    /// Mark key as active
    #[arg(short = 'a', long)]
    pub active: bool,

    /// Mark key as inactive
    #[arg(short = 'i', long)]
    pub inactive: bool,
}

#[derive(Args, Clone)]
pub struct UserListArgs {
    /// Max rows to return
    #[arg(short = 'l', long)]
    pub limit: Option<i64>,

    /// Offset for pagination
    #[arg(short = 'o', long)]
    pub offset: Option<i64>,
}

#[derive(Args, Clone)]
pub struct UserUpdateArgs {
    /// User public key
    #[arg(short = 'p', long)]
    pub public_key: String,

    /// New username
    #[arg(short = 'u', long)]
    pub username: Option<String>,

    /// Clear username (set to NULL)
    #[arg(short = 'U', long)]
    pub clear_username: bool,

    /// Update namespaces (can be specified multiple times, e.g. -n ns1 -n ns2)
    #[arg(short = 'n', long)]
    pub namespaces: Option<Vec<String>>,
}

#[derive(Args, Clone)]
pub struct UserDeleteArgs {
    /// User public key
    #[arg(short = 'p', long)]
    pub public_key: String,
}

#[derive(Args, Clone)]
pub struct SessionListArgs {
    /// Filter by user public key
    #[arg(short = 'p', long)]
    pub public_key: Option<String>,

    /// Max rows to return
    #[arg(short = 'l', long)]
    pub limit: Option<i64>,

    /// Offset for pagination
    #[arg(short = 'o', long)]
    pub offset: Option<i64>,
}

#[derive(Args, Clone)]
pub struct SessionUpdateArgs {
    /// Session id
    #[arg(short = 'i', long)]
    pub id: String,

    /// New title
    #[arg(short = 't', long)]
    pub title: Option<String>,

    /// Clear title (set to NULL)
    #[arg(short = 'T', long)]
    pub clear_title: bool,

    /// Update public key (nullable)
    #[arg(short = 'p', long)]
    pub public_key: Option<String>,

    /// Clear public key (set to NULL)
    #[arg(short = 'P', long)]
    pub clear_public_key: bool,
}

#[derive(Args, Clone)]
pub struct SessionDeleteArgs {
    /// Session id
    #[arg(short = 'i', long)]
    pub id: String,
}

#[derive(Args, Clone)]
pub struct ContractListArgs {
    /// Filter by chain id
    #[arg(short = 'c', long)]
    pub chain_id: Option<i32>,

    /// Filter by address
    #[arg(short = 'a', long)]
    pub address: Option<String>,

    /// Filter by symbol
    #[arg(short = 's', long)]
    pub symbol: Option<String>,

    /// Filter by name
    #[arg(short = 'n', long)]
    pub name: Option<String>,

    /// Filter by protocol
    #[arg(short = 'p', long)]
    pub protocol: Option<String>,

    /// Filter by contract type
    #[arg(short = 't', long)]
    pub contract_type: Option<String>,

    /// Filter by version
    #[arg(short = 'v', long)]
    pub version: Option<String>,

    /// Max rows to return
    #[arg(short = 'l', long)]
    pub limit: Option<i64>,

    /// Offset for pagination
    #[arg(short = 'o', long)]
    pub offset: Option<i64>,
}

#[derive(Args, Clone)]
pub struct ContractUpdateArgs {
    /// Contract chain id
    #[arg(short = 'c', long)]
    pub chain_id: i32,

    /// Contract address
    #[arg(short = 'a', long)]
    pub address: String,

    /// Update name
    #[arg(short = 'n', long)]
    pub name: Option<String>,

    /// Update symbol
    #[arg(short = 's', long)]
    pub symbol: Option<String>,

    /// Clear symbol (set to NULL)
    #[arg(short = 'S', long)]
    pub clear_symbol: bool,

    /// Update protocol
    #[arg(short = 'p', long)]
    pub protocol: Option<String>,

    /// Clear protocol (set to NULL)
    #[arg(short = 'R', long)]
    pub clear_protocol: bool,

    /// Update contract type
    #[arg(short = 't', long)]
    pub contract_type: Option<String>,

    /// Clear contract type (set to NULL)
    #[arg(short = 'T', long)]
    pub clear_contract_type: bool,

    /// Update version
    #[arg(short = 'v', long)]
    pub version: Option<String>,

    /// Clear version (set to NULL)
    #[arg(short = 'V', long)]
    pub clear_version: bool,

    /// Mark as proxy
    #[arg(short = 'x', long)]
    pub is_proxy: bool,

    /// Mark as non-proxy
    #[arg(short = 'X', long)]
    pub not_proxy: bool,

    /// Update implementation address
    #[arg(short = 'i', long)]
    pub implementation_address: Option<String>,

    /// Clear implementation address (set to NULL)
    #[arg(short = 'I', long)]
    pub clear_implementation_address: bool,

    /// Update description
    #[arg(short = 'd', long)]
    pub description: Option<String>,

    /// Clear description (set to NULL)
    #[arg(short = 'D', long)]
    pub clear_description: bool,
}

#[derive(Args, Clone)]
pub struct ContractDeleteArgs {
    /// Contract chain id
    #[arg(short = 'c', long)]
    pub chain_id: i32,

    /// Contract address
    #[arg(short = 'a', long)]
    pub address: String,
}
