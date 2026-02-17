use anyhow::{Context, Result};
use keyring::Entry;

const KEYRING_SERVICE: &str = "solverforge-calendar";
const KEYRING_CLIENT_ID_KEY: &str = "google_client_id";
const KEYRING_CLIENT_SECRET_KEY: &str = "google_client_secret";
const KEYRING_REFRESH_TOKEN_KEY: &str = "google_refresh_token";

/* Opaque client handle — used by the sync module to make API calls. */
/* Wraps credentials needed to build an authenticated Hub. */
pub struct GoogleClient {
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
}

impl GoogleClient {
    // Load credentials from OS keyring. Returns None if not configured.
    pub fn from_keyring() -> Option<Self> {
        let client_id = read_keyring(KEYRING_CLIENT_ID_KEY)?;
        let client_secret = read_keyring(KEYRING_CLIENT_SECRET_KEY)?;
        let refresh_token = read_keyring(KEYRING_REFRESH_TOKEN_KEY)?;
        Some(Self {
            client_id,
            client_secret,
            refresh_token,
        })
    }

    // True if Google credentials are stored in the keyring.
    pub fn is_configured() -> bool {
        read_keyring(KEYRING_REFRESH_TOKEN_KEY).is_some()
    }

    // Save client credentials to keyring.
    pub fn save_credentials(client_id: &str, client_secret: &str) -> Result<()> {
        write_keyring(KEYRING_CLIENT_ID_KEY, client_id)?;
        write_keyring(KEYRING_CLIENT_SECRET_KEY, client_secret)?;
        Ok(())
    }

}

fn read_keyring(key: &str) -> Option<String> {
    Entry::new(KEYRING_SERVICE, key).ok()?.get_password().ok()
}

fn write_keyring(key: &str, value: &str) -> Result<()> {
    Entry::new(KEYRING_SERVICE, key)
        .context("keyring entry creation failed")?
        .set_password(value)
        .context("keyring write failed")
}

/* Run the OAuth2 authorization flow. Opens the browser and waits for the callback. */
/* Returns the refresh token on success. */
pub async fn run_oauth_flow(client_id: &str, client_secret: &str) -> Result<String> {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    // Build the authorization URL
    let redirect_uri = "http://127.0.0.1:8989/oauth/callback";
    let scope = "https://www.googleapis.com/auth/calendar";
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth\
         ?client_id={}\
         &redirect_uri={}\
         &response_type=code\
         &scope={}\
         &access_type=offline\
         &prompt=consent",
        urlenccode(client_id),
        urlenccode(redirect_uri),
        urlenccode(scope),
    );

    // Open browser
    let _ = open::that(&auth_url);

    // Start local callback server
    let listener = TcpListener::bind("127.0.0.1:8989")
        .context("cannot bind OAuth callback server on :8989")?;

    let (mut stream, _) = listener.accept().context("OAuth callback not received")?;

    // Read the HTTP request to extract the `code` parameter
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;

    let code = extract_oauth_code(&request_line)
        .ok_or_else(|| anyhow::anyhow!("No auth code in OAuth callback"))?;

    // Send a success response to the browser
    let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n\
        <html><body style='background:#0B0C16;color:#82FB9C;font-family:monospace'>\
        <h2>SolverForge Calendar</h2><p>Authorization complete. You can close this tab.</p>\
        </body></html>";
    let _ = stream.write_all(response.as_bytes());

    // Exchange code for tokens
    let token_resp = exchange_code_for_token(client_id, client_secret, &code, redirect_uri).await?;
    let refresh_token = token_resp
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("No refresh_token in token response"))?;

    Ok(refresh_token.to_string())
}

fn extract_oauth_code(request_line: &str) -> Option<String> {
    // GET /oauth/callback?code=4/xxxxx&... HTTP/1.1
    let path = request_line.split_whitespace().nth(1)?;
    let query = path.split('?').nth(1)?;
    for pair in query.split('&') {
        if let Some(code) = pair.strip_prefix("code=") {
            return Some(urldecode(code));
        }
    }
    None
}

async fn exchange_code_for_token(
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
) -> Result<serde_json::Value> {
    let client = reqwest_client()?;
    let params = [
        ("code", code),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&params)
        .send()
        .await
        .context("token exchange HTTP request failed")?
        .json::<serde_json::Value>()
        .await
        .context("token exchange JSON parse failed")?;
    Ok(resp)
}

fn reqwest_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .build()
        .context("cannot build HTTP client")
}

fn urlenccode(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' {
                vec![c]
            } else {
                format!("%{:02X}", c as u32).chars().collect()
            }
        })
        .collect()
}

fn urldecode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().unwrap_or('0');
            let h2 = chars.next().unwrap_or('0');
            if let Ok(byte) = u8::from_str_radix(&format!("{}{}", h1, h2), 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}
