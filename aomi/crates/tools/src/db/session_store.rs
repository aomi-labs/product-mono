use super::traits::SessionStoreApi;
use super::{Message, PendingTransaction, Session, User};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::{Pool, QueryBuilder, Row, any::Any};

/// Parse namespaces from database - handles both PostgreSQL array format and JSON
fn parse_namespaces(raw: Option<String>) -> Vec<String> {
    match raw {
        Some(s) if s.is_empty() => vec![],
        Some(s) => {
            // Try PostgreSQL array format: {ns1,ns2,ns3}
            if s.starts_with('{') && s.ends_with('}') {
                let inner = &s[1..s.len() - 1];
                if inner.is_empty() {
                    return vec![];
                }
                return inner.split(',').map(|s| s.trim().to_string()).collect();
            }
            // Try JSON array format: ["ns1","ns2"]
            if let Ok(parsed) = serde_json::from_str::<Vec<String>>(&s) {
                return parsed;
            }
            // Single value or comma-separated
            s.split(',').map(|s| s.trim().to_string()).collect()
        }
        None => vec![],
    }
}

#[derive(Clone, Debug)]
pub struct SessionStore {
    pool: Pool<Any>,
}

impl SessionStore {
    pub fn new(pool: Pool<Any>) -> Self {
        Self { pool }
    }

    pub async fn delete_session_only(&self, session_id: &str) -> Result<u64> {
        let result = sqlx::query::<Any>("DELETE FROM sessions WHERE id = $1")
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}

#[async_trait]
impl SessionStoreApi for SessionStore {
    // User operations
    async fn get_or_create_user(&self, public_key: &str) -> Result<User> {
        // Try to get existing user
        if let Some(user) = self.get_user(public_key).await? {
            return Ok(user);
        }

        // Create new user with explicit timestamp (namespaces uses DB default)
        let now = chrono::Utc::now().timestamp();

        // Insert without RETURNING (more portable)
        let insert_query = "INSERT INTO users (public_key, username, created_at)
                           VALUES ($1, NULL, $2)";

        sqlx::query::<Any>(insert_query)
            .bind(public_key)
            .bind(now)
            .execute(&self.pool)
            .await?;

        // Fetch the created user
        self.get_user(public_key)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to create user"))
    }

    async fn get_user(&self, public_key: &str) -> Result<Option<User>> {
        // Use manual row mapping for cross-database compatibility
        let query = "SELECT public_key, username, created_at, CAST(namespaces AS TEXT) AS namespaces \
                     FROM users WHERE public_key = $1";

        let row = sqlx::query(query)
            .bind(public_key)
            .fetch_optional(&self.pool)
            .await?;

        let user = row
            .map(|r| -> Result<User> {
                let namespaces_raw: Option<String> = r.try_get("namespaces").ok();
                let namespaces = parse_namespaces(namespaces_raw);

                Ok(User {
                    public_key: r.try_get("public_key")?,
                    username: r.try_get("username")?,
                    created_at: r.try_get("created_at")?,
                    namespaces,
                })
            })
            .transpose()?;

        Ok(user)
    }

    async fn update_user_username(&self, public_key: &str, username: Option<String>) -> Result<()> {
        let query = "UPDATE users SET username = $1 WHERE public_key = $2";

        sqlx::query::<Any>(query)
            .bind(username)
            .bind(public_key)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_user_namespaces(
        &self,
        public_key: &str,
        namespaces: Vec<String>,
    ) -> Result<()> {
        // Store as PostgreSQL array literal format for PostgreSQL, works as text for SQLite
        let namespaces_str = format!("{{{}}}", namespaces.join(","));

        // Try PostgreSQL syntax first, fall back to plain text for SQLite
        let pg_query = "UPDATE users SET namespaces = $1::text[] WHERE public_key = $2";
        let result = sqlx::query::<Any>(pg_query)
            .bind(&namespaces_str)
            .bind(public_key)
            .execute(&self.pool)
            .await;

        if result.is_err() {
            // Fallback for SQLite - store as plain text
            let sqlite_query = "UPDATE users SET namespaces = $1 WHERE public_key = $2";
            sqlx::query::<Any>(sqlite_query)
                .bind(&namespaces_str)
                .bind(public_key)
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    async fn list_users(&self, limit: Option<i64>, offset: Option<i64>) -> Result<Vec<User>> {
        // Cast namespaces to TEXT to avoid Any driver failing on PostgreSQL TEXT[] type
        // Using CAST() for cross-database compatibility (works on both PostgreSQL and SQLite)
        let mut query = QueryBuilder::<Any>::new(
            "SELECT public_key, username, created_at, CAST(namespaces AS TEXT) AS namespaces \
             FROM users ORDER BY created_at DESC",
        );

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit);
        }

        if let Some(offset) = offset {
            query.push(" OFFSET ").push_bind(offset);
        }

        let rows = query.build().fetch_all(&self.pool).await?;

        let users = rows
            .into_iter()
            .map(|r| -> Result<User> {
                let namespaces_raw: Option<String> = r.try_get("namespaces").ok();
                let namespaces = parse_namespaces(namespaces_raw);

                Ok(User {
                    public_key: r.try_get("public_key")?,
                    username: r.try_get("username")?,
                    created_at: r.try_get("created_at")?,
                    namespaces,
                })
            })
            .collect::<Result<Vec<User>>>()?;

        Ok(users)
    }

    async fn delete_user(&self, public_key: &str) -> Result<u64> {
        let result = sqlx::query::<Any>("DELETE FROM users WHERE public_key = $1")
            .bind(public_key)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    // Session operations
    async fn create_session(&self, session: &Session) -> Result<()> {
        // Store JSON as JSONB (cast TEXT to JSONB for PostgreSQL)
        let query = "INSERT INTO sessions (id, public_key, started_at, last_active_at, title, pending_transaction)
                     VALUES ($1, $2, $3, $4, $5, $6::jsonb)";

        let pending_tx_json = session
            .pending_transaction
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        sqlx::query::<Any>(query)
            .bind(&session.id)
            .bind(&session.public_key)
            .bind(session.started_at)
            .bind(session.last_active_at)
            .bind(&session.title)
            .bind(pending_tx_json)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_session(&self, session_id: &str) -> Result<Option<Session>> {
        let query = "SELECT id, public_key, started_at, last_active_at, title, \
                     CAST(pending_transaction AS TEXT) AS pending_transaction \
                     FROM sessions WHERE id = $1";

        let row = sqlx::query(query)
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await?;

        let session = row
            .map(|r| -> Result<Session> {
                let pending_tx_str: Option<String> = r.try_get("pending_transaction")?;

                Ok(Session {
                    id: r.try_get("id")?,
                    public_key: r.try_get("public_key")?,
                    started_at: r.try_get("started_at")?,
                    last_active_at: r.try_get("last_active_at")?,
                    title: r.try_get("title")?,
                    pending_transaction: match pending_tx_str {
                        Some(s) => serde_json::from_str(&s).ok(),
                        None => None,
                    },
                })
            })
            .transpose()?;

        Ok(session)
    }

    async fn update_session_activity(&self, session_id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let query = "UPDATE sessions SET last_active_at = $1 WHERE id = $2";

        sqlx::query::<Any>(query)
            .bind(now)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_session_public_key(
        &self,
        session_id: &str,
        public_key: Option<String>,
    ) -> Result<()> {
        let query = "UPDATE sessions SET public_key = $1 WHERE id = $2";

        sqlx::query::<Any>(query)
            .bind(public_key)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_session_title(&self, session_id: &str, title: String) -> Result<()> {
        let query = "UPDATE sessions SET title = $1 WHERE id = $2";

        sqlx::query::<Any>(query)
            .bind(title)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn set_session_title(&self, session_id: &str, title: Option<String>) -> Result<()> {
        let query = "UPDATE sessions SET title = $1 WHERE id = $2";

        sqlx::query::<Any>(query)
            .bind(title)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_messages_persisted(&self, session_id: &str, persisted: bool) -> Result<()> {
        let query = "UPDATE sessions SET messages_persisted = $1 WHERE id = $2";

        sqlx::query::<Any>(query)
            .bind(persisted)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_messages_persisted(&self, session_id: &str) -> Result<Option<bool>> {
        let query = "SELECT messages_persisted FROM sessions WHERE id = $1";

        let row = sqlx::query(query)
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(row.map(|r| r.try_get("messages_persisted")).transpose()?)
    }

    async fn get_user_sessions(&self, public_key: &str, limit: i32) -> Result<Vec<Session>> {
        let query = "SELECT id, public_key, started_at, last_active_at, title, pending_transaction::TEXT
                     FROM sessions
                     WHERE public_key = $1
                     ORDER BY last_active_at DESC
                     LIMIT $2";

        let rows = sqlx::query(query)
            .bind(public_key)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        let sessions = rows
            .into_iter()
            .map(|r| -> Result<Session> {
                let pending_tx_str: Option<String> = r.try_get("pending_transaction")?;

                Ok(Session {
                    id: r.try_get("id")?,
                    public_key: r.try_get("public_key")?,
                    started_at: r.try_get("started_at")?,
                    last_active_at: r.try_get("last_active_at")?,
                    title: r.try_get("title")?,
                    pending_transaction: match pending_tx_str {
                        Some(s) => serde_json::from_str(&s).ok(),
                        None => None,
                    },
                })
            })
            .collect::<Result<Vec<Session>>>()?;

        Ok(sessions)
    }

    async fn list_sessions(
        &self,
        public_key: Option<String>,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> Result<Vec<Session>> {
        let mut query = QueryBuilder::<Any>::new(
            "SELECT id, public_key, started_at, last_active_at, title, \
             CAST(pending_transaction AS TEXT) AS pending_transaction FROM sessions",
        );

        if let Some(public_key) = public_key {
            query.push(" WHERE public_key = ").push_bind(public_key);
        }

        query.push(" ORDER BY last_active_at DESC");

        if let Some(limit) = limit {
            query.push(" LIMIT ").push_bind(limit);
        }

        if let Some(offset) = offset {
            query.push(" OFFSET ").push_bind(offset);
        }

        let rows = query.build().fetch_all(&self.pool).await?;

        let sessions = rows
            .into_iter()
            .map(|r| -> Result<Session> {
                let pending_tx_str: Option<String> = r.try_get("pending_transaction")?;

                Ok(Session {
                    id: r.try_get("id")?,
                    public_key: r.try_get("public_key")?,
                    started_at: r.try_get("started_at")?,
                    last_active_at: r.try_get("last_active_at")?,
                    title: r.try_get("title")?,
                    pending_transaction: match pending_tx_str {
                        Some(s) => serde_json::from_str(&s).ok(),
                        None => None,
                    },
                })
            })
            .collect::<Result<Vec<Session>>>()?;

        Ok(sessions)
    }

    async fn delete_old_sessions(&self, inactive_since: i64) -> Result<u64> {
        let query = "DELETE FROM sessions WHERE last_active_at < $1";

        let result = sqlx::query::<Any>(query)
            .bind(inactive_since)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }

    async fn delete_session(&self, session_id: &str) -> Result<()> {
        // Delete all messages for this session first
        let delete_messages_query = "DELETE FROM messages WHERE session_id = $1";
        sqlx::query::<Any>(delete_messages_query)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        // Then delete the session
        let delete_session_query = "DELETE FROM sessions WHERE id = $1";
        sqlx::query::<Any>(delete_session_query)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // Pending transaction operations
    async fn update_pending_transaction(
        &self,
        session_id: &str,
        tx: Option<PendingTransaction>,
    ) -> Result<()> {
        let tx_json = tx.map(|v| serde_json::to_string(&v)).transpose()?;
        let now = chrono::Utc::now().timestamp();

        let query = "UPDATE sessions
                     SET pending_transaction = $1::jsonb,
                         last_active_at = $2
                     WHERE id = $3";

        sqlx::query::<Any>(query)
            .bind(tx_json)
            .bind(now)
            .bind(session_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // Message operations
    async fn save_message(&self, message: &Message) -> Result<i64> {
        let content_json = serde_json::to_string(&message.content)?;

        // Insert without RETURNING for cross-database compatibility
        let insert_query =
            "INSERT INTO messages (session_id, message_type, sender, content, timestamp)
                           VALUES ($1, $2, $3, $4, $5)";

        sqlx::query::<Any>(insert_query)
            .bind(&message.session_id)
            .bind(&message.message_type)
            .bind(&message.sender)
            .bind(&content_json)
            .bind(message.timestamp)
            .execute(&self.pool)
            .await?;

        // Get the last inserted ID
        let id_query = "SELECT MAX(id) as id FROM messages WHERE session_id = $1";
        let row = sqlx::query(id_query)
            .bind(&message.session_id)
            .fetch_one(&self.pool)
            .await?;

        let id: i64 = row.try_get("id")?;
        Ok(id)
    }

    async fn get_messages(
        &self,
        session_id: &str,
        message_type: Option<&str>,
        limit: Option<i32>,
    ) -> Result<Vec<Message>> {
        let query = match (message_type, limit) {
            (Some(_), Some(_)) => {
                "SELECT id, session_id, message_type, sender, content, timestamp
                 FROM messages
                 WHERE session_id = $1 AND message_type = $2
                 ORDER BY timestamp DESC
                 LIMIT $3"
            }
            (Some(_), None) => {
                "SELECT id, session_id, message_type, sender, content, timestamp
                 FROM messages
                 WHERE session_id = $1 AND message_type = $2
                 ORDER BY timestamp DESC"
            }
            (None, Some(_)) => {
                "SELECT id, session_id, message_type, sender, content, timestamp
                 FROM messages
                 WHERE session_id = $1
                 ORDER BY timestamp DESC
                 LIMIT $2"
            }
            (None, None) => {
                "SELECT id, session_id, message_type, sender, content, timestamp
                 FROM messages
                 WHERE session_id = $1
                 ORDER BY timestamp DESC"
            }
        };

        let mut query_builder = sqlx::query(query).bind(session_id);

        if let Some(msg_type) = message_type {
            query_builder = query_builder.bind(msg_type);
        }

        if let Some(lim) = limit {
            query_builder = query_builder.bind(lim);
        }

        let rows = query_builder.fetch_all(&self.pool).await?;

        let messages = rows
            .into_iter()
            .map(|r| -> Result<Message> {
                let content_str: String = r.try_get("content")?;
                let content = serde_json::from_str(&content_str)?;

                Ok(Message {
                    id: r.try_get("id")?,
                    session_id: r.try_get("session_id")?,
                    message_type: r.try_get("message_type")?,
                    sender: r.try_get("sender")?,
                    content,
                    timestamp: r.try_get("timestamp")?,
                })
            })
            .collect::<Result<Vec<Message>>>()?;

        Ok(messages)
    }

    async fn get_user_message_history(&self, public_key: &str, limit: i32) -> Result<Vec<Message>> {
        let query = "SELECT m.id, m.session_id, m.message_type, m.sender, m.content, m.timestamp
                     FROM messages m
                     JOIN sessions s ON m.session_id = s.id
                     WHERE s.public_key = $1 AND m.message_type = 'chat'
                     ORDER BY m.timestamp DESC
                     LIMIT $2";

        let rows = sqlx::query(query)
            .bind(public_key)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        let messages = rows
            .into_iter()
            .map(|r| -> Result<Message> {
                let content_str: String = r.try_get("content")?;
                let content = serde_json::from_str(&content_str)?;

                Ok(Message {
                    id: r.try_get("id")?,
                    session_id: r.try_get("session_id")?,
                    message_type: r.try_get("message_type")?,
                    sender: r.try_get("sender")?,
                    content,
                    timestamp: r.try_get("timestamp")?,
                })
            })
            .collect::<Result<Vec<Message>>>()?;

        Ok(messages)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::any::AnyPoolOptions;

    async fn setup_test_store() -> Result<SessionStore> {
        // Install SQLite driver for sqlx::Any
        sqlx::any::install_default_drivers();

        // Use sqlite: prefix to tell sqlx::Any which driver to use
        let pool = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;

        // Create the session persistence tables
        sqlx::query(
            r#"
            CREATE TABLE users (
                public_key TEXT PRIMARY KEY,
                username TEXT UNIQUE,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                namespaces TEXT DEFAULT '{default}'
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                public_key TEXT REFERENCES users(public_key) ON DELETE SET NULL,
                started_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                last_active_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                title TEXT,
                pending_transaction TEXT,
                messages_persisted INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                message_type TEXT NOT NULL DEFAULT 'chat',
                sender TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(SessionStore::new(pool))
    }

    #[tokio::test]
    async fn test_get_or_create_user() -> Result<()> {
        let store = setup_test_store().await?;

        // Create new user
        let user = store.get_or_create_user("test_public_key").await?;
        assert_eq!(user.public_key, "test_public_key");
        assert_eq!(user.username, None);
        assert!(user.created_at > 0);

        // Get existing user
        let user2 = store.get_or_create_user("test_public_key").await?;
        assert_eq!(user.public_key, user2.public_key);
        assert_eq!(user.created_at, user2.created_at);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_user_username() -> Result<()> {
        let store = setup_test_store().await?;

        let user = store.get_or_create_user("test_key").await?;
        assert_eq!(user.username, None);

        // Update username
        store
            .update_user_username("test_key", Some("alice".to_string()))
            .await?;

        let updated_user = store.get_user("test_key").await?;
        assert!(updated_user.is_some());
        assert_eq!(updated_user.unwrap().username, Some("alice".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_create_and_get_session() -> Result<()> {
        let store = setup_test_store().await?;

        let session = Session {
            id: "session_123".to_string(),
            public_key: None,
            started_at: 1699564800,
            last_active_at: 1699564800,
            title: Some("Test Session".to_string()),
            pending_transaction: None,
        };

        // Create session
        store.create_session(&session).await?;

        // Retrieve session
        let retrieved = store.get_session("session_123").await?;
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.id, "session_123");
        assert_eq!(retrieved.title, Some("Test Session".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_update_session_activity() -> Result<()> {
        let store = setup_test_store().await?;

        let session = Session {
            id: "session_456".to_string(),
            public_key: None,
            started_at: 1699564800,
            last_active_at: 1699564800,
            title: None,
            pending_transaction: None,
        };

        store.create_session(&session).await?;

        // Update activity
        store.update_session_activity("session_456").await?;

        let updated = store.get_session("session_456").await?;
        assert!(updated.is_some());
        // last_active_at should be updated (will be different from initial value)
        assert!(updated.unwrap().last_active_at >= 1699564800);

        Ok(())
    }

    #[tokio::test]
    async fn test_update_session_public_key() -> Result<()> {
        let store = setup_test_store().await?;

        // Create user first
        store.get_or_create_user("user_key").await?;

        let session = Session {
            id: "session_789".to_string(),
            public_key: None,
            started_at: 1699564800,
            last_active_at: 1699564800,
            title: None,
            pending_transaction: None,
        };

        store.create_session(&session).await?;

        // Associate session with user
        store
            .update_session_public_key("session_789", Some("user_key".to_string()))
            .await?;

        let updated = store.get_session("session_789").await?;
        assert!(updated.is_some());
        assert_eq!(updated.unwrap().public_key, Some("user_key".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_user_sessions() -> Result<()> {
        let store = setup_test_store().await?;

        // Create user
        store.get_or_create_user("user_abc").await?;

        // Create multiple sessions for the user
        for i in 1..=3 {
            let session = Session {
                id: format!("session_{}", i),
                public_key: Some("user_abc".to_string()),
                started_at: 1699564800 + i,
                last_active_at: 1699564800 + i,
                title: Some(format!("Session {}", i)),
                pending_transaction: None,
            };
            store.create_session(&session).await?;
        }

        // Get sessions for user
        let sessions = store.get_user_sessions("user_abc", 10).await?;
        assert_eq!(sessions.len(), 3);

        // Should be ordered by last_active_at DESC
        assert_eq!(sessions[0].title, Some("Session 3".to_string()));
        assert_eq!(sessions[1].title, Some("Session 2".to_string()));
        assert_eq!(sessions[2].title, Some("Session 1".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_delete_old_sessions() -> Result<()> {
        let store = setup_test_store().await?;

        // Create old and new sessions
        let old_session = Session {
            id: "old_session".to_string(),
            public_key: None,
            started_at: 1000000,
            last_active_at: 1000000,
            title: Some("Old".to_string()),
            pending_transaction: None,
        };

        let new_session = Session {
            id: "new_session".to_string(),
            public_key: None,
            started_at: 2000000,
            last_active_at: 2000000,
            title: Some("New".to_string()),
            pending_transaction: None,
        };

        store.create_session(&old_session).await?;
        store.create_session(&new_session).await?;

        // Delete sessions older than 1500000
        let deleted = store.delete_old_sessions(1500000).await?;
        assert_eq!(deleted, 1);

        // Verify old session is gone
        assert!(store.get_session("old_session").await?.is_none());
        // Verify new session still exists
        assert!(store.get_session("new_session").await?.is_some());

        Ok(())
    }

    #[tokio::test]
    async fn test_update_pending_transaction() -> Result<()> {
        let store = setup_test_store().await?;

        let session = Session {
            id: "session_tx".to_string(),
            public_key: None,
            started_at: 1699564800,
            last_active_at: 1699564800,
            title: None,
            pending_transaction: None,
        };

        store.create_session(&session).await?;

        // Add pending transaction
        let pending_tx = PendingTransaction {
            created_at: 1699564800,
            expires_at: 1699568400,
            chain_id: 1,
            transaction: json!({
                "from": "0x123",
                "to": "0x456",
                "value": "0x16345785d8a0000"
            }),
            user_intent: "Send 0.1 ETH to alice".to_string(),
            signature: None,
        };

        store
            .update_pending_transaction("session_tx", Some(pending_tx.clone()))
            .await?;

        // Verify it was saved
        let updated = store.get_session("session_tx").await?;
        assert!(updated.is_some());
        let updated = updated.unwrap();
        assert!(updated.pending_transaction.is_some());

        let retrieved_tx = updated.get_pending_transaction()?;
        assert!(retrieved_tx.is_some());
        let retrieved_tx = retrieved_tx.unwrap();
        assert_eq!(retrieved_tx.chain_id, 1);
        assert_eq!(retrieved_tx.user_intent, "Send 0.1 ETH to alice");

        // Clear pending transaction
        store.update_pending_transaction("session_tx", None).await?;

        let cleared = store.get_session("session_tx").await?;
        assert!(cleared.is_some());
        assert!(cleared.unwrap().pending_transaction.is_none());

        Ok(())
    }

    #[tokio::test]
    async fn test_save_and_get_messages() -> Result<()> {
        let store = setup_test_store().await?;

        // Create session
        let session = Session {
            id: "session_msg".to_string(),
            public_key: None,
            started_at: 1699564800,
            last_active_at: 1699564800,
            title: None,
            pending_transaction: None,
        };
        store.create_session(&session).await?;

        // Save chat message
        let message = Message {
            id: 0, // Will be assigned by DB
            session_id: "session_msg".to_string(),
            message_type: "chat".to_string(),
            sender: "user".to_string(),
            content: json!({"text": "Hello, world!"}),
            timestamp: 1699564800,
        };

        let msg_id = store.save_message(&message).await?;
        assert!(msg_id > 0);

        // Retrieve messages
        let messages = store.get_messages("session_msg", None, None).await?;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].sender, "user");
        assert_eq!(messages[0].content["text"], "Hello, world!");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_messages_by_type() -> Result<()> {
        let store = setup_test_store().await?;

        // Create session
        let session = Session {
            id: "session_types".to_string(),
            public_key: None,
            started_at: 1699564800,
            last_active_at: 1699564800,
            title: None,
            pending_transaction: None,
        };
        store.create_session(&session).await?;

        // Save different message types
        let chat_msg = Message {
            id: 0,
            session_id: "session_types".to_string(),
            message_type: "chat".to_string(),
            sender: "user".to_string(),
            content: json!({"text": "Chat message"}),
            timestamp: 1699564800,
        };

        let agent_msg = Message {
            id: 0,
            session_id: "session_types".to_string(),
            message_type: "agent_history".to_string(),
            sender: "assistant".to_string(),
            content: json!({"type": "api_message", "content": []}),
            timestamp: 1699564801,
        };

        store.save_message(&chat_msg).await?;
        store.save_message(&agent_msg).await?;

        // Get only chat messages
        let chat_messages = store
            .get_messages("session_types", Some("chat"), None)
            .await?;
        assert_eq!(chat_messages.len(), 1);
        assert_eq!(chat_messages[0].message_type, "chat");

        // Get only agent history
        let agent_messages = store
            .get_messages("session_types", Some("agent_history"), None)
            .await?;
        assert_eq!(agent_messages.len(), 1);
        assert_eq!(agent_messages[0].message_type, "agent_history");

        // Get all messages
        let all_messages = store.get_messages("session_types", None, None).await?;
        assert_eq!(all_messages.len(), 2);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_user_message_history() -> Result<()> {
        let store = setup_test_store().await?;

        // Create user
        store.get_or_create_user("user_history").await?;

        // Create two sessions for the user
        let session1 = Session {
            id: "session_h1".to_string(),
            public_key: Some("user_history".to_string()),
            started_at: 1699564800,
            last_active_at: 1699564800,
            title: None,
            pending_transaction: None,
        };

        let session2 = Session {
            id: "session_h2".to_string(),
            public_key: Some("user_history".to_string()),
            started_at: 1699564900,
            last_active_at: 1699564900,
            title: None,
            pending_transaction: None,
        };

        store.create_session(&session1).await?;
        store.create_session(&session2).await?;

        // Add messages to both sessions
        for i in 1..=3 {
            let msg = Message {
                id: 0,
                session_id: "session_h1".to_string(),
                message_type: "chat".to_string(),
                sender: "user".to_string(),
                content: json!({"text": format!("Message {}", i)}),
                timestamp: 1699564800 + i,
            };
            store.save_message(&msg).await?;
        }

        for i in 1..=2 {
            let msg = Message {
                id: 0,
                session_id: "session_h2".to_string(),
                message_type: "chat".to_string(),
                sender: "user".to_string(),
                content: json!({"text": format!("Session 2 Message {}", i)}),
                timestamp: 1699564900 + i,
            };
            store.save_message(&msg).await?;
        }

        // Get user history
        let history = store.get_user_message_history("user_history", 10).await?;
        assert_eq!(history.len(), 5);

        // Should be ordered by timestamp DESC (most recent first)
        assert_eq!(history[0].content["text"], "Session 2 Message 2");
        assert_eq!(history[4].content["text"], "Message 1");

        Ok(())
    }

    #[tokio::test]
    async fn test_message_limit() -> Result<()> {
        let store = setup_test_store().await?;

        // Create session
        let session = Session {
            id: "session_limit".to_string(),
            public_key: None,
            started_at: 1699564800,
            last_active_at: 1699564800,
            title: None,
            pending_transaction: None,
        };
        store.create_session(&session).await?;

        // Save 10 messages
        for i in 1..=10 {
            let msg = Message {
                id: 0,
                session_id: "session_limit".to_string(),
                message_type: "chat".to_string(),
                sender: "user".to_string(),
                content: json!({"text": format!("Message {}", i)}),
                timestamp: 1699564800 + i,
            };
            store.save_message(&msg).await?;
        }

        // Get with limit
        let limited = store.get_messages("session_limit", None, Some(5)).await?;
        assert_eq!(limited.len(), 5);

        Ok(())
    }
}
