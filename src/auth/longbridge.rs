use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use longbridge::Config;
use longbridge::httpclient::{HttpClient, HttpClientConfig};
use longbridge::oauth::OAuthBuilder;
use serde::{Deserialize, Serialize};

use crate::error::Error;

/// Token response from Longbridge OAuth token endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct LongbridgeTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    pub token_type: String,
}

/// Token file format compatible with the Longbridge SDK.
#[derive(Debug, Serialize, Deserialize)]
struct TokenFile {
    client_id: String,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: u64,
}

/// Return the Longbridge API base URL.
///
/// Defaults to `https://openapi.longbridge.com` but can be overridden via the
/// `LONGBRIDGE_HTTP_URL` environment variable.
fn longbridge_api_url() -> String {
    std::env::var("LONGBRIDGE_HTTP_URL")
        .unwrap_or_else(|_| "https://openapi.longbridge.com".to_string())
}

/// Dynamically register an OAuth client with Longbridge and return the
/// `client_id`.
pub async fn register_client(callback_url: &str) -> Result<String, Error> {
    let base_url = longbridge_api_url();
    let url = format!("{base_url}/oauth2/register");

    let body = serde_json::json!({
        "redirect_uris": [callback_url],
        "token_endpoint_auth_method": "none",
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "client_name": "Longbridge MCP Server"
    });

    let resp = reqwest::Client::new().post(&url).json(&body).send().await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(Error::OAuth(format!(
            "client registration failed ({status}): {text}"
        )));
    }

    #[derive(Deserialize)]
    struct RegisterResponse {
        client_id: String,
    }

    let parsed: RegisterResponse = resp.json().await?;
    Ok(parsed.client_id)
}

/// Exchange an authorization code for Longbridge tokens.
pub async fn exchange_token(
    client_id: &str,
    code: &str,
    callback_url: &str,
) -> Result<LongbridgeTokens, Error> {
    let base_url = longbridge_api_url();
    let url = format!("{base_url}/oauth2/token");

    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", callback_url),
        ("client_id", client_id),
    ];

    let resp = reqwest::Client::new()
        .post(&url)
        .form(&params)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(Error::OAuth(format!(
            "token exchange failed ({status}): {text}"
        )));
    }

    let tokens: LongbridgeTokens = resp.json().await?;
    Ok(tokens)
}

/// Save tokens in the SDK-compatible file format at
/// `~/.longbridge/openapi/tokens/<client_id>`.
pub fn save_token_file(client_id: &str, tokens: &LongbridgeTokens) -> Result<(), Error> {
    let home = dirs::home_dir().ok_or_else(|| Error::Other("no home directory".to_string()))?;
    let token_dir = home.join(".longbridge").join("openapi").join("tokens");
    std::fs::create_dir_all(&token_dir)?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let file = TokenFile {
        client_id: client_id.to_string(),
        access_token: tokens.access_token.clone(),
        refresh_token: tokens.refresh_token.clone(),
        expires_at: now + tokens.expires_in,
    };

    let path = token_dir.join(client_id);
    let data = serde_json::to_string_pretty(&file)?;
    std::fs::write(&path, data)?;

    tracing::debug!(client_id, path = %path.display(), "saved token file");
    Ok(())
}

/// Load the token via `OAuthBuilder` and create a `Config` + `HttpClient` pair.
///
/// The token file must already exist on disk before calling this function.
pub async fn create_session(client_id: &str) -> Result<(Arc<Config>, HttpClient), Error> {
    let oauth = OAuthBuilder::new(client_id)
        .build(|_| unreachable!("token should be cached"))
        .await
        .map_err(|e| Error::OAuth(format!("failed to build OAuth client: {e}")))?;

    let http_config = HttpClientConfig::from_oauth(oauth.clone());
    let http_client = HttpClient::new(http_config);
    let config = Arc::new(Config::from_oauth(oauth).dont_print_quote_packages());

    Ok((config, http_client))
}
