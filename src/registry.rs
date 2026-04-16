use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use longbridge::httpclient::HttpClient;
use longbridge::{Config, ContentContext, QuoteContext, TradeContext};
use rusqlite::Connection;
use serde::Serialize;
use tokio::sync::RwLock;

use crate::auth::server::OAuthState;
use crate::error::Error;

pub struct UserSession {
    /// Longbridge SDK Config
    pub config: Arc<Config>,
    /// QuoteContext (lazy)
    pub quote_context: Option<QuoteContext>,
    /// TradeContext (lazy)
    pub trade_context: Option<TradeContext>,
    /// ContentContext (lazy)
    pub content_context: Option<ContentContext>,
    /// HttpClient (lazy)
    pub http_client: Option<HttpClient>,
    /// Last access time
    pub last_accessed: Instant,
}

#[derive(Serialize)]
pub struct UserInfo {
    pub user_id: String,
    pub client_id: String,
    pub created_at: String,
    pub active: bool,
    pub last_accessed: Option<String>,
}

impl std::fmt::Debug for UserRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserRegistry")
            .field("idle_timeout", &self.idle_timeout)
            .field("db_path", &self.db_path)
            .finish_non_exhaustive()
    }
}

pub struct UserRegistry {
    sessions: RwLock<HashMap<String, UserSession>>,
    idle_timeout: Duration,
    db_path: PathBuf,
    oauth_state: OAuthState,
}

impl UserRegistry {
    pub async fn new(idle_timeout: Duration, mcp_dir: &Path) -> Result<Self, Error> {
        let db_path = mcp_dir.join("mcp.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                user_id TEXT PRIMARY KEY,
                client_id TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        )?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS mcp_clients (
                client_id TEXT PRIMARY KEY,
                client_name TEXT,
                redirect_uris TEXT NOT NULL,
                created_at TEXT NOT NULL
            )",
            [],
        )?;
        Ok(Self {
            sessions: RwLock::new(HashMap::new()),
            idle_timeout,
            db_path,
            oauth_state: OAuthState::new(),
        })
    }

    pub fn oauth_state(&self) -> &OAuthState {
        &self.oauth_state
    }

    pub async fn user_exists(&self, user_id: &str) -> bool {
        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => return false,
        };
        conn.query_row("SELECT 1 FROM users WHERE user_id = ?1", [user_id], |_| {
            Ok(())
        })
        .is_ok()
    }

    pub async fn register_mcp_client(
        &self,
        client_id: &str,
        client_name: Option<&str>,
        redirect_uris: &[String],
    ) -> Result<(), Error> {
        let conn = Connection::open(&self.db_path)?;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| Error::Other(e.to_string()))?;
        let uris_json = serde_json::to_string(redirect_uris)?;
        conn.execute(
            "INSERT OR REPLACE INTO mcp_clients (client_id, client_name, redirect_uris, created_at) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![client_id, client_name.unwrap_or(""), uris_json, now],
        )?;
        Ok(())
    }

    pub async fn validate_mcp_client(
        &self,
        client_id: &str,
        redirect_uri: &str,
    ) -> Result<bool, Error> {
        let conn = Connection::open(&self.db_path)?;
        let uris_json: String = conn
            .query_row(
                "SELECT redirect_uris FROM mcp_clients WHERE client_id = ?1",
                [client_id],
                |row| row.get(0),
            )
            .map_err(|_| Error::SessionNotFound(client_id.to_string()))?;
        let uris: Vec<String> = serde_json::from_str(&uris_json).unwrap_or_default();
        Ok(uris.is_empty() || uris.contains(&redirect_uri.to_string()))
    }

    pub async fn register_user(&self, user_id: &str, client_id: &str) -> Result<(), Error> {
        let conn = Connection::open(&self.db_path)?;
        let now = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| Error::Other(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO users (user_id, client_id, created_at) VALUES (?1, ?2, ?3)",
            rusqlite::params![user_id, client_id, now],
        )?;
        Ok(())
    }

    pub async fn revoke_user(&self, user_id: &str) -> Result<(), Error> {
        let conn = Connection::open(&self.db_path)?;

        // Get client_id before deleting
        let client_id: Option<String> = conn
            .query_row(
                "SELECT client_id FROM users WHERE user_id = ?1",
                [user_id],
                |row| row.get(0),
            )
            .ok();

        let deleted = conn.execute("DELETE FROM users WHERE user_id = ?1", [user_id])?;
        if deleted == 0 {
            return Err(Error::SessionNotFound(user_id.to_string()));
        }

        // Remove in-memory session
        self.sessions.write().await.remove(user_id);

        // Delete token file
        if let Some(cid) = client_id {
            let token_path = dirs::home_dir()
                .unwrap_or_default()
                .join(".longbridge")
                .join("openapi")
                .join("tokens")
                .join(&cid);
            let _ = std::fs::remove_file(&token_path);
        }

        Ok(())
    }

    async fn recover_session(&self, user_id: &str) -> Result<(), Error> {
        let conn = Connection::open(&self.db_path)?;
        let client_id: String = conn
            .query_row(
                "SELECT client_id FROM users WHERE user_id = ?1",
                [user_id],
                |row| row.get(0),
            )
            .map_err(|_| Error::SessionNotFound(user_id.to_string()))?;

        let (config, http_client) = crate::auth::longbridge::create_session(&client_id).await?;

        self.sessions.write().await.insert(
            user_id.to_string(),
            UserSession {
                config,
                quote_context: None,
                trade_context: None,
                content_context: None,
                http_client: Some(http_client),
                last_accessed: Instant::now(),
            },
        );

        tracing::info!(user_id, client_id, "recovered session from token file");
        Ok(())
    }

    pub async fn get_quote_context(&self, user_id: &str) -> Result<QuoteContext, Error> {
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(user_id) {
                session.last_accessed = Instant::now();
                if let Some(ref ctx) = session.quote_context {
                    return Ok(ctx.clone());
                }
                let (ctx, _) = QuoteContext::new(session.config.clone());
                session.quote_context = Some(ctx.clone());
                return Ok(ctx);
            }
        }

        self.recover_session(user_id).await?;

        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(user_id)
            .ok_or_else(|| Error::SessionNotFound(user_id.to_string()))?;
        session.last_accessed = Instant::now();
        let (ctx, _) = QuoteContext::new(session.config.clone());
        session.quote_context = Some(ctx.clone());
        Ok(ctx)
    }

    pub async fn get_trade_context(&self, user_id: &str) -> Result<TradeContext, Error> {
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(user_id) {
                session.last_accessed = Instant::now();
                if let Some(ref ctx) = session.trade_context {
                    return Ok(ctx.clone());
                }
                let (ctx, _) = TradeContext::new(session.config.clone());
                session.trade_context = Some(ctx.clone());
                return Ok(ctx);
            }
        }

        self.recover_session(user_id).await?;

        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(user_id)
            .ok_or_else(|| Error::SessionNotFound(user_id.to_string()))?;
        session.last_accessed = Instant::now();
        let (ctx, _) = TradeContext::new(session.config.clone());
        session.trade_context = Some(ctx.clone());
        Ok(ctx)
    }

    pub async fn get_http_client(&self, user_id: &str) -> Result<HttpClient, Error> {
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(user_id) {
                session.last_accessed = Instant::now();
                return session
                    .http_client
                    .clone()
                    .ok_or_else(|| Error::Other("http client not initialized".to_string()));
            }
        }

        self.recover_session(user_id).await?;

        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(user_id)
            .ok_or_else(|| Error::SessionNotFound(user_id.to_string()))?;
        session.last_accessed = Instant::now();
        session
            .http_client
            .clone()
            .ok_or_else(|| Error::Other("http client not initialized".to_string()))
    }

    pub async fn get_content_context(&self, user_id: &str) -> Result<ContentContext, Error> {
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(user_id) {
                session.last_accessed = Instant::now();
                if let Some(ref ctx) = session.content_context {
                    return Ok(ctx.clone());
                }
                let ctx = ContentContext::new(session.config.clone());
                session.content_context = Some(ctx.clone());
                return Ok(ctx);
            }
        }

        self.recover_session(user_id).await?;

        let mut sessions = self.sessions.write().await;
        let session = sessions
            .get_mut(user_id)
            .ok_or_else(|| Error::SessionNotFound(user_id.to_string()))?;
        session.last_accessed = Instant::now();
        let ctx = ContentContext::new(session.config.clone());
        session.content_context = Some(ctx.clone());
        Ok(ctx)
    }

    pub async fn list_users(&self) -> Vec<UserInfo> {
        let conn = match Connection::open(&self.db_path) {
            Ok(c) => c,
            Err(_) => return vec![],
        };

        let sessions = self.sessions.read().await;

        let mut stmt = match conn.prepare("SELECT user_id, client_id, created_at FROM users") {
            Ok(s) => s,
            Err(_) => return vec![],
        };

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .ok();

        let mut users = Vec::new();
        if let Some(rows) = rows {
            for row in rows.flatten() {
                let (uid, cid, created) = row;
                let session = sessions.get(&uid);
                users.push(UserInfo {
                    user_id: uid,
                    client_id: cid,
                    created_at: created,
                    active: session.is_some(),
                    last_accessed: session.map(|s| {
                        let elapsed = s.last_accessed.elapsed();
                        let secs = elapsed.as_secs();
                        let now =
                            time::OffsetDateTime::now_utc() - time::Duration::seconds(secs as i64);
                        now.format(&time::format_description::well_known::Rfc3339)
                            .unwrap_or_else(|_| format!("{secs}s ago"))
                    }),
                });
            }
        }
        users
    }

    pub async fn create_session(
        &self,
        user_id: &str,
        client_id: &str,
        config: Arc<Config>,
        http_client: HttpClient,
    ) -> Result<(), Error> {
        self.register_user(user_id, client_id).await?;
        self.sessions.write().await.insert(
            user_id.to_string(),
            UserSession {
                config,
                quote_context: None,
                trade_context: None,
                content_context: None,
                http_client: Some(http_client),
                last_accessed: Instant::now(),
            },
        );
        Ok(())
    }

    pub fn spawn_cleanup_task(self: &Arc<Self>) {
        let registry = Arc::clone(self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            loop {
                interval.tick().await;
                let mut sessions = registry.sessions.write().await;
                let before = sessions.len();
                sessions.retain(|user_id, session| {
                    let idle = session.last_accessed.elapsed() < registry.idle_timeout;
                    if !idle {
                        tracing::info!(user_id, "releasing idle session");
                    }
                    idle
                });
                let removed = before - sessions.len();
                if removed > 0 {
                    tracing::info!(removed, "cleaned up idle sessions");
                }
                let session_count = sessions.len() as i64;
                let quote_count = sessions
                    .values()
                    .filter(|s| s.quote_context.is_some())
                    .count() as i64;
                let trade_count = sessions
                    .values()
                    .filter(|s| s.trade_context.is_some())
                    .count() as i64;
                crate::metrics::set_active_sessions(session_count);
                crate::metrics::set_active_quote_contexts(quote_count);
                crate::metrics::set_active_trade_contexts(trade_count);

                // Count registered users from DB
                if let Ok(conn) = Connection::open(&registry.db_path)
                    && let Ok(count) =
                        conn.query_row("SELECT COUNT(*) FROM users", [], |row| row.get::<_, i64>(0))
                {
                    crate::metrics::set_registered_users_total(count);
                }
            }
        });
    }
}
