use aomi_tools::db::{
    ApiKey, ApiKeyStore, ApiKeyStoreApi, ApiKeyUpdate, Contract, ContractSearchParams,
    ContractStore, ContractStoreApi, ContractUpdate, Session, SessionStore, SessionStoreApi, User,
};
use aomi_tools::{AomiTool, AomiToolArgs, ToolCallCtx, with_topic};
use rand::{RngCore, rngs::OsRng};
use rig::tool::ToolError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{AnyPool, any::AnyPoolOptions};
use tokio::sync::{OnceCell, oneshot};

const ADMIN_NAMESPACE: &str = "admin";

static POOL: OnceCell<AnyPool> = OnceCell::const_new();

fn resolve_database_url() -> String {
    if let Ok(url) = std::env::var("DATABASE_URL") {
        return url;
    }

    let user = std::env::var("PGUSER")
        .or_else(|_| std::env::var("USER"))
        .unwrap_or_else(|_| "postgres".to_string());
    let host = std::env::var("PGHOST").unwrap_or_else(|_| "localhost".to_string());
    let port = std::env::var("PGPORT").unwrap_or_else(|_| "5432".to_string());
    let database = std::env::var("PGDATABASE").unwrap_or_else(|_| "chatbot".to_string());
    format!("postgres://{user}@{host}:{port}/{database}")
}

async fn admin_pool() -> Result<AnyPool, ToolError> {
    let pool = POOL
        .get_or_try_init(|| async {
            sqlx::any::install_default_drivers();
            let database_url = resolve_database_url();
            AnyPoolOptions::new()
                .max_connections(5)
                .connect(&database_url)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))
        })
        .await?;
    Ok(pool.clone())
}

fn api_key_to_json(row: &ApiKey) -> Value {
    json!({
        "id": row.id,
        "api_key": row.api_key,
        "label": row.label,
        "allowed_namespaces": row.allowed_namespaces,
        "is_active": row.is_active,
        "created_at": row.created_at,
    })
}

fn user_to_json(row: &User) -> Value {
    json!({
        "public_key": row.public_key,
        "username": row.username,
        "created_at": row.created_at,
    })
}

fn session_to_json(row: &Session) -> Value {
    json!({
        "id": row.id,
        "public_key": row.public_key,
        "started_at": row.started_at,
        "last_active_at": row.last_active_at,
        "title": row.title,
        "has_pending": row.pending_transaction.is_some(),
    })
}

fn contract_to_json(row: &Contract) -> Value {
    json!({
        "address": row.address,
        "chain": row.chain,
        "chain_id": row.chain_id,
        "name": row.name,
        "symbol": row.symbol,
        "protocol": row.protocol,
        "contract_type": row.contract_type,
        "version": row.version,
        "is_proxy": row.is_proxy,
        "implementation_address": row.implementation_address,
        "description": row.description,
        "updated_at": row.updated_at,
    })
}

fn generate_api_key() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push_str(&format!("{:02x}", byte));
    }
    out
}

fn send_tool_result(
    sender: oneshot::Sender<eyre::Result<Value>>,
    result: Result<Value, ToolError>,
) {
    let _ = sender.send(result.map_err(|e| eyre::eyre!(e.to_string())));
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateApiKeyArgs {
    pub namespaces: Vec<String>,
    pub label: Option<String>,
    pub api_key: Option<String>,
}

impl AomiToolArgs for CreateApiKeyArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "namespaces": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Namespaces allowed for this key (e.g., ['default','l2beat','admin'])"
                },
                "label": { "type": "string" },
                "api_key": { "type": "string", "description": "Optional override for the API key value" }
            },
            "required": ["namespaces"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminCreateApiKey;

impl AomiTool for AdminCreateApiKey {
    const NAME: &'static str = "admin_create_api_key";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = CreateApiKeyArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Create an API key with allowed namespaces."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            if args.namespaces.is_empty() {
                return Err(ToolError::ToolCallError(
                    "namespaces cannot be empty".into(),
                ));
            }
            let namespaces = args
                .namespaces
                .into_iter()
                .map(|entry| entry.trim().to_string())
                .filter(|entry| !entry.is_empty())
                .collect::<Vec<_>>();
            if namespaces.is_empty() {
                return Err(ToolError::ToolCallError(
                    "namespaces cannot be empty".into(),
                ));
            }

            let api_key = args.api_key.unwrap_or_else(generate_api_key);
            let pool = admin_pool().await?;
            let store = ApiKeyStore::new(pool);
            let row = store
                .create_api_key(api_key, args.label, namespaces)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            Ok(api_key_to_json(&row))
        }
        .await;

        send_tool_result(sender, result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListApiKeysArgs {
    #[serde(default)]
    pub active_only: bool,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl AomiToolArgs for ListApiKeysArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "active_only": { "type": "boolean", "default": false },
                "limit": { "type": "integer" },
                "offset": { "type": "integer" }
            }
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminListApiKeys;

impl AomiTool for AdminListApiKeys {
    const NAME: &'static str = "admin_list_api_keys";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = ListApiKeysArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "List API keys with optional filters."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            let pool = admin_pool().await?;
            let store = ApiKeyStore::new(pool);
            let rows = store
                .list_api_keys(args.active_only, args.limit, args.offset)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            Ok(json!(rows.iter().map(api_key_to_json).collect::<Vec<_>>()))
        }
        .await;

        send_tool_result(sender, result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateApiKeyArgs {
    pub api_key: String,
    pub label: Option<String>,
    #[serde(default)]
    pub clear_label: bool,
    pub namespaces: Option<Vec<String>>,
    pub active: Option<bool>,
}

impl AomiToolArgs for UpdateApiKeyArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "api_key": { "type": "string" },
                "label": { "type": "string" },
                "clear_label": { "type": "boolean", "default": false },
                "namespaces": { "type": "array", "items": { "type": "string" } },
                "active": { "type": "boolean", "description": "Set to true/false to toggle key activation" }
            },
            "required": ["api_key"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminUpdateApiKey;

impl AomiTool for AdminUpdateApiKey {
    const NAME: &'static str = "admin_update_api_key";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = UpdateApiKeyArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Update API key label, namespaces, or active status."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            if args.clear_label && args.label.is_some() {
                return Err(ToolError::ToolCallError(
                    "cannot set label and clear_label together".into(),
                ));
            }

            let namespaces = args.namespaces.map(|entries| {
                entries
                    .into_iter()
                    .map(|entry| entry.trim().to_string())
                    .filter(|entry| !entry.is_empty())
                    .collect::<Vec<_>>()
            });

            if namespaces.as_ref().is_some_and(|values| values.is_empty()) {
                return Err(ToolError::ToolCallError(
                    "namespaces cannot be empty".into(),
                ));
            }

            let pool = admin_pool().await?;
            let store = ApiKeyStore::new(pool);
            let update = ApiKeyUpdate {
                api_key: args.api_key,
                label: args.label,
                clear_label: args.clear_label,
                allowed_namespaces: namespaces,
                is_active: args.active,
            };
            let row = store
                .update_api_key(update)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            Ok(api_key_to_json(&row))
        }
        .await;

        send_tool_result(sender, result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListUsersArgs {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl AomiToolArgs for ListUsersArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer" },
                "offset": { "type": "integer" }
            }
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminListUsers;

impl AomiTool for AdminListUsers {
    const NAME: &'static str = "admin_list_users";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = ListUsersArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "List users from the database."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            let pool = admin_pool().await?;
            let store = SessionStore::new(pool);
            let rows = store
                .list_users(args.limit, args.offset)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            Ok(json!(rows.iter().map(user_to_json).collect::<Vec<_>>()))
        }
        .await;

        send_tool_result(sender, result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserArgs {
    pub public_key: String,
    pub username: Option<String>,
    #[serde(default)]
    pub clear_username: bool,
}

impl AomiToolArgs for UpdateUserArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "public_key": { "type": "string" },
                "username": { "type": "string" },
                "clear_username": { "type": "boolean", "default": false }
            },
            "required": ["public_key"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminUpdateUser;

impl AomiTool for AdminUpdateUser {
    const NAME: &'static str = "admin_update_user";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = UpdateUserArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Update a user's username."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            if args.clear_username && args.username.is_some() {
                return Err(ToolError::ToolCallError(
                    "cannot set username and clear_username together".into(),
                ));
            }

            if !args.clear_username && args.username.is_none() {
                return Err(ToolError::ToolCallError(
                    "no fields provided to update".into(),
                ));
            }

            let pool = admin_pool().await?;
            let store = SessionStore::new(pool);

            let username = if args.clear_username {
                None
            } else {
                args.username
            };
            store
                .update_user_username(&args.public_key, username)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;

            let user = store
                .get_user(&args.public_key)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                .ok_or_else(|| ToolError::ToolCallError("user not found".into()))?;
            Ok(user_to_json(&user))
        }
        .await;

        send_tool_result(sender, result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteUserArgs {
    pub public_key: String,
}

impl AomiToolArgs for DeleteUserArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "public_key": { "type": "string" }
            },
            "required": ["public_key"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminDeleteUser;

impl AomiTool for AdminDeleteUser {
    const NAME: &'static str = "admin_delete_user";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = DeleteUserArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Delete a user by public key."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            let pool = admin_pool().await?;
            let store = SessionStore::new(pool);
            let deleted = store
                .delete_user(&args.public_key)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            Ok(json!({
                "public_key": args.public_key,
                "deleted": deleted,
            }))
        }
        .await;

        send_tool_result(sender, result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSessionsArgs {
    pub public_key: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl AomiToolArgs for ListSessionsArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "public_key": { "type": "string" },
                "limit": { "type": "integer" },
                "offset": { "type": "integer" }
            }
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminListSessions;

impl AomiTool for AdminListSessions {
    const NAME: &'static str = "admin_list_sessions";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = ListSessionsArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "List sessions with optional filters."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            let pool = admin_pool().await?;
            let store = SessionStore::new(pool);
            let rows = store
                .list_sessions(args.public_key, args.limit, args.offset)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            Ok(json!(rows.iter().map(session_to_json).collect::<Vec<_>>()))
        }
        .await;

        send_tool_result(sender, result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSessionArgs {
    pub id: String,
    pub title: Option<String>,
    #[serde(default)]
    pub clear_title: bool,
    pub public_key: Option<String>,
    #[serde(default)]
    pub clear_public_key: bool,
}

impl AomiToolArgs for UpdateSessionArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "title": { "type": "string" },
                "clear_title": { "type": "boolean", "default": false },
                "public_key": { "type": "string" },
                "clear_public_key": { "type": "boolean", "default": false }
            },
            "required": ["id"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminUpdateSession;

impl AomiTool for AdminUpdateSession {
    const NAME: &'static str = "admin_update_session";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = UpdateSessionArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Update session metadata."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            if args.clear_title && args.title.is_some() {
                return Err(ToolError::ToolCallError(
                    "cannot set title and clear_title together".into(),
                ));
            }
            if args.clear_public_key && args.public_key.is_some() {
                return Err(ToolError::ToolCallError(
                    "cannot set public_key and clear_public_key together".into(),
                ));
            }

            if !args.clear_title
                && args.title.is_none()
                && !args.clear_public_key
                && args.public_key.is_none()
            {
                return Err(ToolError::ToolCallError(
                    "no fields provided to update".into(),
                ));
            }

            let pool = admin_pool().await?;
            let store = SessionStore::new(pool);

            if args.clear_title {
                store
                    .set_session_title(&args.id, None)
                    .await
                    .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            } else if let Some(title) = args.title {
                store
                    .set_session_title(&args.id, Some(title))
                    .await
                    .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            }

            if args.clear_public_key {
                store
                    .update_session_public_key(&args.id, None)
                    .await
                    .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            } else if let Some(public_key) = args.public_key {
                store
                    .update_session_public_key(&args.id, Some(public_key))
                    .await
                    .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            }

            let session = store
                .get_session(&args.id)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                .ok_or_else(|| ToolError::ToolCallError("session not found".into()))?;
            Ok(session_to_json(&session))
        }
        .await;

        send_tool_result(sender, result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteSessionArgs {
    pub id: String,
}

impl AomiToolArgs for DeleteSessionArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" }
            },
            "required": ["id"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminDeleteSession;

impl AomiTool for AdminDeleteSession {
    const NAME: &'static str = "admin_delete_session";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = DeleteSessionArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Delete a session by id."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            let pool = admin_pool().await?;
            let store = SessionStore::new(pool);
            let existed = store
                .get_session(&args.id)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                .is_some();
            store
                .delete_session(&args.id)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            Ok(json!({
                "id": args.id,
                "deleted": if existed { 1 } else { 0 },
            }))
        }
        .await;

        send_tool_result(sender, result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListContractsArgs {
    pub chain_id: Option<u32>,
    pub address: Option<String>,
    pub symbol: Option<String>,
    pub name: Option<String>,
    pub protocol: Option<String>,
    pub contract_type: Option<String>,
    pub version: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl AomiToolArgs for ListContractsArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "chain_id": { "type": "integer" },
                "address": { "type": "string" },
                "symbol": { "type": "string" },
                "name": { "type": "string" },
                "protocol": { "type": "string" },
                "contract_type": { "type": "string" },
                "version": { "type": "string" },
                "limit": { "type": "integer" },
                "offset": { "type": "integer" }
            }
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminListContracts;

impl AomiTool for AdminListContracts {
    const NAME: &'static str = "admin_list_contracts";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = ListContractsArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "List contracts with optional filters."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            let params = ContractSearchParams {
                chain_id: args.chain_id,
                address: args.address,
                name: args.name,
                symbol: args.symbol,
                protocol: args.protocol,
                contract_type: args.contract_type,
                version: args.version,
            };

            let pool = admin_pool().await?;
            let store = ContractStore::new(pool);
            let rows = store
                .list_contracts(params, args.limit, args.offset)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            Ok(json!(rows.iter().map(contract_to_json).collect::<Vec<_>>()))
        }
        .await;

        send_tool_result(sender, result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateContractArgs {
    pub chain_id: u32,
    pub address: String,
    pub name: Option<String>,
    pub symbol: Option<String>,
    #[serde(default)]
    pub clear_symbol: bool,
    pub protocol: Option<String>,
    #[serde(default)]
    pub clear_protocol: bool,
    pub contract_type: Option<String>,
    #[serde(default)]
    pub clear_contract_type: bool,
    pub version: Option<String>,
    #[serde(default)]
    pub clear_version: bool,
    #[serde(default)]
    pub is_proxy: bool,
    #[serde(default)]
    pub not_proxy: bool,
    pub implementation_address: Option<String>,
    #[serde(default)]
    pub clear_implementation_address: bool,
    pub description: Option<String>,
    #[serde(default)]
    pub clear_description: bool,
}

impl AomiToolArgs for UpdateContractArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "chain_id": { "type": "integer" },
                "address": { "type": "string" },
                "name": { "type": "string" },
                "symbol": { "type": "string" },
                "clear_symbol": { "type": "boolean", "default": false },
                "protocol": { "type": "string" },
                "clear_protocol": { "type": "boolean", "default": false },
                "contract_type": { "type": "string" },
                "clear_contract_type": { "type": "boolean", "default": false },
                "version": { "type": "string" },
                "clear_version": { "type": "boolean", "default": false },
                "is_proxy": { "type": "boolean", "default": false },
                "not_proxy": { "type": "boolean", "default": false },
                "implementation_address": { "type": "string" },
                "clear_implementation_address": { "type": "boolean", "default": false },
                "description": { "type": "string" },
                "clear_description": { "type": "boolean", "default": false }
            },
            "required": ["chain_id", "address"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminUpdateContract;

impl AomiTool for AdminUpdateContract {
    const NAME: &'static str = "admin_update_contract";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = UpdateContractArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Update contract metadata."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            if args.is_proxy && args.not_proxy {
                return Err(ToolError::ToolCallError(
                    "cannot set is_proxy and not_proxy together".into(),
                ));
            }

            let is_proxy = if args.is_proxy {
                Some(true)
            } else if args.not_proxy {
                Some(false)
            } else {
                None
            };

            let update = ContractUpdate {
                chain_id: args.chain_id,
                address: args.address,
                name: args.name,
                symbol: args.symbol,
                clear_symbol: args.clear_symbol,
                protocol: args.protocol,
                clear_protocol: args.clear_protocol,
                contract_type: args.contract_type,
                clear_contract_type: args.clear_contract_type,
                version: args.version,
                clear_version: args.clear_version,
                is_proxy,
                implementation_address: args.implementation_address,
                clear_implementation_address: args.clear_implementation_address,
                description: args.description,
                clear_description: args.clear_description,
            };

            let pool = admin_pool().await?;
            let store = ContractStore::new(pool);
            let row = store
                .update_contract(update)
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            Ok(contract_to_json(&row))
        }
        .await;

        send_tool_result(sender, result);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteContractArgs {
    pub chain_id: u32,
    pub address: String,
}

impl AomiToolArgs for DeleteContractArgs {
    fn schema() -> Value {
        with_topic(json!({
            "type": "object",
            "properties": {
                "chain_id": { "type": "integer" },
                "address": { "type": "string" }
            },
            "required": ["chain_id", "address"]
        }))
    }
}

#[derive(Debug, Clone)]
pub struct AdminDeleteContract;

impl AomiTool for AdminDeleteContract {
    const NAME: &'static str = "admin_delete_contract";
    const NAMESPACE: &'static str = ADMIN_NAMESPACE;

    type Args = DeleteContractArgs;
    type Output = Value;
    type Error = ToolError;

    fn description(&self) -> &'static str {
        "Delete a contract by chain id and address."
    }

    async fn run_sync(
        &self,
        sender: oneshot::Sender<eyre::Result<Value>>,
        _ctx: ToolCallCtx,
        args: Self::Args,
    ) {
        let result = async {
            let pool = admin_pool().await?;
            let store = ContractStore::new(pool);
            let existed = store
                .get_contract(args.chain_id, args.address.clone())
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?
                .is_some();
            store
                .delete_contract(args.chain_id, args.address.clone())
                .await
                .map_err(|e| ToolError::ToolCallError(e.to_string().into()))?;
            Ok(json!({
                "chain_id": args.chain_id,
                "address": args.address,
                "deleted": if existed { 1 } else { 0 },
            }))
        }
        .await;

        send_tool_result(sender, result);
    }
}
