use sqlx::FromRow;

#[derive(Debug, FromRow)]
pub struct ApiKeyRow {
    pub id: i64,
    pub api_key: String,
    pub label: Option<String>,
    pub allowed_chatbots: String,
    pub is_active: bool,
    pub created_at: i64,
}

#[derive(Debug, FromRow)]
pub struct UserRow {
    pub public_key: String,
    pub username: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, FromRow)]
pub struct SessionRow {
    pub id: String,
    pub public_key: Option<String>,
    pub started_at: i64,
    pub last_active_at: i64,
    pub title: Option<String>,
    pub has_pending: bool,
}

#[derive(Debug, FromRow)]
pub struct ContractRow {
    pub address: String,
    pub chain: String,
    pub chain_id: i32,
    pub name: String,
    pub symbol: Option<String>,
    pub protocol: Option<String>,
    pub contract_type: Option<String>,
    pub version: Option<String>,
    pub is_proxy: bool,
    pub implementation_address: Option<String>,
    pub updated_at: i64,
}
