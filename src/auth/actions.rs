//! Authentication actions (login, logout, whoami).

use std::time::Duration;

use tokio::time::sleep;

use super::api_keys::create_api_key;
use super::errors::AuthError;
use super::keyring::{delete_api_key, get_api_key, save_api_key};
use super::responses::{
    AccountResponse, DeviceAuthResponse, OpenIdConfig, TokenErrorResponse, TokenResponse,
};
use crate::context::CommandContext;
use crate::http::{Client, bearer_header};
use crate::input::KeyListener;
use crate::ui::status;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Print a QR code to the terminal with indentation.
fn print_qr(qr: &str) {
    eprintln!();
    for line in qr.lines() {
        eprintln!("    {}", line);
    }
    eprintln!();
}

/// Print logged-in user status line.
///
/// Example: `✓ Logged in as user@example.com`
fn print_logged_in_status(email: &str) {
    status::success(&format!("Logged in as {}", status::highlight(email)));
}

/// Parse a date string (YYYY-MM-DD format) into a NaiveDate.
fn parse_date(date_str: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()
}

/// Format a duration from today to a target date as a human-readable string.
///
/// Uses chrono's date arithmetic to correctly handle leap years.
///
/// Returns (formatted_string, is_expired).
/// Examples:
/// - 400 days from now -> ("1 year, 35 days", false)
/// - 1 day from now -> ("1 day", false)
/// - past date -> ("expired", true)
fn format_duration_until(target: chrono::NaiveDate) -> (String, bool) {
    let today = chrono::Utc::now().date_naive();

    if target < today {
        return ("expired".to_string(), true);
    }

    // Calculate years by finding how many full years fit
    let mut years = 0i32;
    let mut date_after_years = today;
    while let Some(next) = date_after_years.checked_add_months(chrono::Months::new(12)) {
        if next <= target {
            years += 1;
            date_after_years = next;
        } else {
            break;
        }
    }

    // Remaining days after subtracting full years
    let remaining_days = (target - date_after_years).num_days();

    let year_unit = if years == 1 { "year" } else { "years" };
    let day_unit = if remaining_days == 1 { "day" } else { "days" };

    let s = if years > 0 {
        if remaining_days == 0 {
            format!("{} {}", years, year_unit)
        } else {
            format!("{} {}, {} {}", years, year_unit, remaining_days, day_unit)
        }
    } else {
        format!("{} {}", remaining_days, day_unit)
    };

    (s, false)
}

/// Print token expiration info.
///
/// Example: `  expires      2027-04-20 (1 year)`
fn print_token_expiration(expires_at: &str) {
    if let Some(target_date) = parse_date(expires_at) {
        let (duration_str, _) = format_duration_until(target_date);
        // "Logged in as " is 13 chars; "  expires" is 9 chars; need 4 more spaces
        eprintln!(
            "  {}{}{}",
            status::dim("expires      "),
            status::highlight(expires_at),
            status::dim(&format!(" ({})", duration_str))
        );
    }
}

/// Save API key and display login success information.
///
/// This is the common "finalize login" logic shared by both device flow and direct API key login.
async fn save_and_display_login(ctx: &CommandContext, api_key: &str) -> Result<(), AuthError> {
    use super::api_keys::get_expiration;

    // Save to keyring
    save_api_key(&ctx.config, api_key)?;
    status::success("API key stored in keyring");

    // Fetch and display user info
    if let Ok(login_info) = fetch_login_info(ctx, api_key).await {
        print_logged_in_status(&login_info.email);
        if let Some(expires_at) = get_expiration(api_key) {
            print_token_expiration(&expires_at);
        }
    }

    Ok(())
}

/// Combined login information for display.
struct LoginInfo {
    email: String,
}

/// Fetch login info for display after login.
async fn fetch_login_info(ctx: &CommandContext, api_key: &str) -> Result<LoginInfo, AuthError> {
    // Create a client with the just-saved API key
    let builder = reqwest::Client::builder().default_headers(bearer_header(api_key));
    let client = Client::new(builder, ctx.config.base_url())?;

    // Fetch account info
    let account_response = client.get("/api/account").send().await?;
    let account: AccountResponse = account_response.json().await?;

    let email = account
        .user
        .as_ref()
        .and_then(|u| u.email.clone())
        .or_else(|| account.user.as_ref().and_then(|u| u.username.clone()))
        .unwrap_or_else(|| "unknown".to_string());

    Ok(LoginInfo { email })
}

/// Check if stdin is a pipe (non-TTY).
fn stdin_is_pipe() -> bool {
    use std::io::IsTerminal;
    !std::io::stdin().is_terminal()
}

/// Read API key from stdin (for piped input).
fn read_api_key_from_stdin() -> Result<String, AuthError> {
    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut line = String::new();
    stdin
        .lock()
        .read_line(&mut line)
        .map_err(|e| AuthError::InvalidApiKey(format!("Failed to read from stdin: {}", e)))?;
    Ok(line.trim().to_string())
}

/// Prompt user for API key with secure (hidden) input.
fn prompt_api_key_hidden() -> Result<String, AuthError> {
    use std::io::Write;
    eprint!("{} ", status::dim("API key:"));
    std::io::stderr().flush().unwrap();

    let api_key = rpassword::read_password()
        .map_err(|e| AuthError::InvalidApiKey(format!("Failed to read API key: {}", e)))?;

    // Show masked feedback on the same line as prompt
    // Move cursor up one line, clear it, then reprint with mask
    let mask = "•".repeat(api_key.len().min(32));
    eprint!("\x1b[1A\x1b[2K"); // ANSI: move up, clear line
    eprintln!("{} {}", status::dim("API key:"), status::dim(&mask));

    Ok(api_key.trim().to_string())
}

/// Login with a provided API key (bypassing device flow).
async fn login_with_api_key(
    ctx: &CommandContext,
    api_key: String,
    force: bool,
) -> Result<(), AuthError> {
    use super::api_keys::is_valid_api_key;

    // Validate the API key format
    if !is_valid_api_key(&api_key) {
        return Err(AuthError::InvalidApiKey(
            "not a valid JWT token".to_string(),
        ));
    }

    // Check if already logged in
    if !force && get_api_key(&ctx.config)?.is_some() {
        status::warn(&format!(
            "Already logged in to {}",
            status::highlight(&ctx.config.domain)
        ));

        // If stdin is a pipe, we can't prompt interactively - require --force
        if stdin_is_pipe() {
            status::info(&format!(
                "Use {} to overwrite existing credentials",
                status::highlight("--force")
            ));
            return Ok(());
        }

        if !crate::input::prompt_yes_no("Overwrite existing credentials?") {
            return Ok(());
        }
    }

    save_and_display_login(ctx, &api_key).await
}

/// Try to read API key from stdin if data is available.
/// Returns Some(key) if stdin has data, None if empty/EOF.
fn try_read_api_key_from_stdin() -> Option<String> {
    if !stdin_is_pipe() {
        return None;
    }

    use std::io::BufRead;
    let stdin = std::io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).ok()? > 0 {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
}

/// Perform the device authorization flow.
async fn login_device_flow(ctx: &CommandContext, force: bool) -> Result<(), AuthError> {
    // We use an unauthenticated client since login by definition happens first.
    // The auth flow needs to follow direct URLs from openid-configuration etc.

    // Check if already logged in
    if !force && get_api_key(&ctx.config)?.is_some() {
        status::warn(&format!(
            "Already logged in to {}",
            status::highlight(&ctx.config.domain)
        ));
        if !crate::input::prompt_yes_no("Login again?") {
            // Return early if user declines to log in again
            return Ok(());
        }
    }

    let client = ctx.unauthenticated_client(REQUEST_TIMEOUT).ok_or_else(|| {
        AuthError::Middleware("failed to create unauthenticated client".to_string())
    })?;

    // Fetch OpenID configuration
    let openid_config: OpenIdConfig = client
        .get(ctx.config.well_known_url())
        .send()
        .await?
        .json()
        .await?;

    let device_auth_endpoint = openid_config
        .device_authorization_endpoint
        .ok_or_else(|| AuthError::MissingEndpoint("device_authorization_endpoint".to_string()))?;

    // Request device authorization
    let device_response: DeviceAuthResponse = client
        .post(&device_auth_endpoint)
        .form(&[
            ("client_id", ctx.config.client_id.as_str()),
            ("scope", "openid profile email"),
        ])
        .send()
        .await?
        .json()
        .await?;

    // Display instructions to user
    let display_uri = device_response
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&device_response.verification_uri);

    // Try to open browser first — this determines whether we show QR immediately
    let browser_opened = if ctx.config.open_browser {
        let uri = device_response
            .verification_uri_complete
            .as_ref()
            .unwrap_or(&device_response.verification_uri);
        webbrowser::open(uri).is_ok()
    } else {
        false
    };

    // Pre-generate the QR code string
    let qr_output = crate::qr::qr_to_terminal(display_uri, 1, true).ok();

    // Listen for 'q' keypress in a background thread (for on-demand QR).
    // KeyListener handles terminal state restoration and Ctrl+C.
    let listen_for_q = browser_opened && qr_output.is_some();
    let key_listener = if listen_for_q {
        KeyListener::spawn(&['q'])
    } else {
        None
    };

    let mut qr_shown = false;
    if browser_opened {
        // Browser opened — clean layout, offer QR on demand
        status::info(&format!(
            "Opening {} in your browser...",
            status::highlight(&ctx.config.domain)
        ));
        status::blank_line();
        status::info("To authenticate, visit:");
        status::blank_line();
        eprintln!("  {}", status::highlight(display_uri));
        if device_response.verification_uri_complete.is_none() {
            status::blank_line();
            status::info(&format!(
                "And enter the code: {}",
                status::highlight(&device_response.user_code)
            ));
        }
        status::blank_line();
        if qr_output.is_some() {
            status::waiting("Waiting for authentication... (press q for QR code)");
        } else {
            status::waiting("Waiting for authentication...");
        }
    } else {
        status::info("To authenticate, scan the QR code or visit:");
        status::blank_line();
        eprintln!("  {}", status::highlight(display_uri));
        if device_response.verification_uri_complete.is_none() {
            status::blank_line();
            status::info(&format!(
                "And enter the code: {}",
                status::highlight(&device_response.user_code)
            ));
        }
        status::blank_line();
        status::waiting("Waiting for authentication...");

        // No browser — show QR code immediately
        if let Some(ref qr) = qr_output {
            print_qr(qr);
            qr_shown = true;
        }
    }

    // Poll for token
    let interval = Duration::from_secs(device_response.interval.unwrap_or(5));
    let timeout = Duration::from_secs(device_response.expires_in);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            tracing::error!("Authentication timed out");
            return Err(AuthError::Timeout);
        }

        // Check for 'q' keypress while waiting
        let sleep_until = std::time::Instant::now() + interval;
        while std::time::Instant::now() < sleep_until {
            if !qr_shown {
                if let Some(ref listener) = key_listener {
                    if listener.try_recv().is_some() {
                        if let Some(ref qr) = qr_output {
                            print_qr(qr);
                            qr_shown = true;
                        }
                    }
                }
            }
            sleep(Duration::from_millis(100)).await;
        }

        let response = client
            .post(&openid_config.token_endpoint)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("device_code", &device_response.device_code),
                ("client_id", &ctx.config.client_id),
            ])
            .send()
            .await?;

        if response.status().is_success() {
            let token: TokenResponse = response.json().await?;
            status::blank_line();
            status::success("Authentication complete");

            // Create API key
            let api_key = create_api_key(&client, &ctx.config, &token.access_token).await?;

            return save_and_display_login(ctx, &api_key).await;
        }

        let error: TokenErrorResponse = response.json().await?;
        match error.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => {
                sleep(Duration::from_secs(5)).await;
                continue;
            }
            "expired_token" => {
                tracing::error!("Token expired during authentication");
                return Err(AuthError::Timeout);
            }
            "access_denied" => {
                tracing::error!("Access denied by user");
                return Err(AuthError::Authorization(
                    "Access denied by user".to_string(),
                ));
            }
            _ => {
                let msg = error
                    .error_description
                    .unwrap_or_else(|| error.error.clone());
                tracing::error!("Authorization error: {}", msg);
                return Err(AuthError::Authorization(msg));
            }
        }
    }
}

/// Perform login - either via API key or device authorization flow.
///
/// Arguments:
/// - `api_key`: Positional API key value. Use "-" to read from stdin.
/// - `prompt_api_key`: If true (--api-key flag), prompt for API key with hidden input.
/// - `force`: Overwrite existing credentials without confirmation.
///
/// Precedence:
/// 1. `ana login <key>` - use provided key directly
/// 2. `ana login -` - read from stdin explicitly
/// 3. `ana login --api-key` - prompt for API key (hidden input)
/// 4. `echo key | ana login` - read from stdin if piped
/// 5. `ana login` - device flow
pub async fn login(
    ctx: &CommandContext,
    api_key: Option<String>,
    prompt_api_key: bool,
    force: bool,
) -> Result<(), AuthError> {
    match api_key {
        Some(key) if key == "-" => {
            // Explicit stdin read: `ana login -`
            let api_key = read_api_key_from_stdin()?;
            login_with_api_key(ctx, api_key, force).await
        }
        Some(key) => {
            // Positional argument: `ana login <key>`
            login_with_api_key(ctx, key, force).await
        }
        None if prompt_api_key => {
            // --api-key flag: prompt for API key (or read stdin if piped)
            let api_key = if stdin_is_pipe() {
                read_api_key_from_stdin()?
            } else {
                prompt_api_key_hidden()?
            };
            login_with_api_key(ctx, api_key, force).await
        }
        None => {
            // No args: check if stdin has data piped in, otherwise device flow
            if let Some(api_key) = try_read_api_key_from_stdin() {
                login_with_api_key(ctx, api_key, force).await
            } else {
                login_device_flow(ctx, force).await
            }
        }
    }
}

/// Log out by removing the API key for the current domain.
pub fn logout(ctx: &CommandContext) -> Result<(), AuthError> {
    // Check if already logged out
    if get_api_key(&ctx.config)?.is_none() {
        status::warn(&format!(
            "Not logged in to {}",
            status::highlight(&ctx.config.domain)
        ));
        return Ok(());
    }

    delete_api_key(&ctx.config)?;
    status::success(&format!(
        "Logged out of {}",
        status::highlight(&ctx.config.domain)
    ));
    status::success("API key removed from system keyring");
    status::warn(&format!(
        "To fully revoke your token visit {}",
        status::highlight(&format!("{}/app/profile/api-keys", ctx.config.base_url()))
    ));
    Ok(())
}

/// Display the API key for the current domain.
pub fn show_api_key(ctx: &CommandContext) -> Result<(), AuthError> {
    match get_api_key(&ctx.config)? {
        Some(key) => println!("{}", key),
        None => {
            status::error("not logged in");
            status::info(&format!(
                "Run {} to authenticate.",
                status::highlight("ana login")
            ));
        }
    }

    Ok(())
}

/// Print a labeled key-value line for whoami output.
///
/// Example: `  name        Alice Smith`
fn print_kv(key: &str, value: &str) {
    // Pad key to 12 chars (longest key "username" is 8 + 4 spaces = 12)
    // Pad plain text first, then apply styling (ANSI codes break format padding)
    eprintln!(
        "  {}{}",
        status::dim(&format!("{:<12}", key)),
        status::highlight(value)
    );
}

/// Mask an API key, showing only the prefix.
///
/// Example: `pfx_****************************`
fn mask_api_key(key: &str) -> String {
    if key.len() > 4 {
        format!("{}_{}", &key[..3], "*".repeat(28))
    } else {
        "*".repeat(32)
    }
}

/// Display information about the logged-in user.
pub async fn whoami(ctx: &CommandContext, json: bool) -> Result<(), AuthError> {
    // Check if logged in by checking for API key
    if get_api_key(&ctx.config)?.is_none() {
        status::error("not logged in");
        status::info(&format!(
            "Run {} to authenticate.",
            status::highlight("ana login")
        ));
        return Ok(());
    }

    let response = ctx.client.get("/api/auth/sessions/whoami").send().await?;

    if !response.status().is_success() {
        let resp_status = response.status();
        let body = response.text().await.unwrap_or_default();
        tracing::error!("Failed to get account info: {} - {}", resp_status, body);
        return Err(AuthError::Authorization(format!(
            "Failed to get account info: {} - {}",
            resp_status, body
        )));
    }

    let data: serde_json::Value = response.json().await?;

    // JSON output mode
    if json {
        let pretty = serde_json::to_string_pretty(&data).unwrap_or_default();
        println!("{}", pretty);
        return Ok(());
    }

    // Styled output mode
    // Data is nested under passport.profile for user info
    let profile = data.get("passport").and_then(|p| p.get("profile"));

    // Account section
    eprintln!("{}", status::section("account"));

    // Build name from first_name + last_name
    let first = profile
        .and_then(|p| p.get("first_name"))
        .and_then(|v| v.as_str());
    let last = profile
        .and_then(|p| p.get("last_name"))
        .and_then(|v| v.as_str());
    match (first, last) {
        (Some(f), Some(l)) => print_kv("name", &format!("{} {}", f, l)),
        (Some(f), None) => print_kv("name", f),
        (None, Some(l)) => print_kv("name", l),
        _ => {}
    }

    if let Some(username) = profile
        .and_then(|p| p.get("username"))
        .and_then(|v| v.as_str())
    {
        print_kv("username", username);
    }
    if let Some(email) = profile
        .and_then(|p| p.get("email"))
        .and_then(|v| v.as_str())
    {
        print_kv("email", email);
    }

    // Organizations section - shows orgs with their subscriptions
    let organizations = data
        .get("passport")
        .and_then(|p| p.get("organizations"))
        .and_then(|v| v.as_array());

    if let Some(orgs) = organizations {
        // Filter to orgs that have subscription attributes
        let orgs_with_subs: Vec<_> = orgs
            .iter()
            .filter_map(|org| {
                let title = org.get("title").and_then(|v| v.as_str())?;
                let attrs = org.get("attributes").and_then(|v| v.as_array())?;
                let sub = attrs
                    .iter()
                    .find(|a| a.get("group").and_then(|v| v.as_str()) == Some("subscriptions"))?;
                let sub_id = sub.get("id").and_then(|v| v.as_str())?;
                let expires = sub
                    .get("data")
                    .and_then(|d| d.get("expires_at"))
                    .and_then(|v| v.as_str())?;
                Some((title, sub_id, expires))
            })
            .collect();

        if !orgs_with_subs.is_empty() {
            status::blank_line();
            eprintln!("{}", status::section("subscriptions"));

            // Build labels and find max width for alignment
            let rows: Vec<_> = orgs_with_subs
                .iter()
                .map(|(org_title, sub_id, expires)| {
                    let sub_type = sub_id.split('_').next().unwrap_or(sub_id);
                    let label = format!("{} ({})", org_title, sub_type);
                    let date_part = if expires.len() >= 10 {
                        &expires[..10]
                    } else {
                        *expires
                    };
                    (label, date_part.to_string())
                })
                .collect();

            let max_label_width = rows.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
            let pad_width = max_label_width + 4; // 4 spaces after longest label

            for (label, date_part) in rows {
                if let Some(target_date) = parse_date(&date_part) {
                    let (duration_str, is_expired) = format_duration_until(target_date);
                    let suffix = if is_expired {
                        use crate::ui::styles::UiColor;
                        format!(" ({})", UiColor::Red.apply_to(&duration_str))
                    } else {
                        status::dim(&format!(" ({})", duration_str))
                    };
                    eprintln!(
                        "  {}{}{}",
                        status::dim(&format!("{:<width$}", label, width = pad_width)),
                        status::highlight(&date_part),
                        suffix
                    );
                } else {
                    eprintln!(
                        "  {}{}",
                        status::dim(&format!("{:<width$}", label, width = pad_width)),
                        status::highlight(&date_part)
                    );
                }
            }
        }
    }

    // Token info section
    status::blank_line();
    eprintln!("{}", status::section("token"));
    if let Some(api_key) = get_api_key(&ctx.config)? {
        eprintln!(
            "  {}{}",
            status::dim(&format!("{:<12}", "value")),
            status::dim(&mask_api_key(&api_key))
        );
    }
    eprintln!(
        "  {}{}",
        status::dim(&format!("{:<12}", "storage")),
        status::dim(&ctx.config.keyring_path.display().to_string())
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_openid_config_deserialize() {
        let config: OpenIdConfig = serde_json::from_value(json!({
            "device_authorization_endpoint": "https://example.com/device",
            "token_endpoint": "https://example.com/token"
        }))
        .unwrap();
        assert_eq!(
            config.device_authorization_endpoint,
            Some("https://example.com/device".to_string())
        );
        assert_eq!(config.token_endpoint, "https://example.com/token");
    }

    #[test]
    fn test_openid_config_without_device_endpoint() {
        let config: OpenIdConfig = serde_json::from_value(json!({
            "token_endpoint": "https://example.com/token"
        }))
        .unwrap();
        assert!(config.device_authorization_endpoint.is_none());
    }

    #[test]
    fn test_device_auth_response_deserialize() {
        let response: DeviceAuthResponse = serde_json::from_value(json!({
            "device_code": "abc123",
            "user_code": "ABCD-1234",
            "verification_uri": "https://example.com/verify",
            "verification_uri_complete": "https://example.com/verify?code=ABCD-1234",
            "expires_in": 600,
            "interval": 5
        }))
        .unwrap();
        assert_eq!(response.device_code, "abc123");
        assert_eq!(response.user_code, "ABCD-1234");
        assert_eq!(response.verification_uri, "https://example.com/verify");
        assert_eq!(response.expires_in, 600);
        assert_eq!(response.interval, Some(5));
    }

    #[test]
    fn test_device_auth_response_minimal() {
        let response: DeviceAuthResponse = serde_json::from_value(json!({
            "device_code": "abc",
            "user_code": "XYZ",
            "verification_uri": "https://example.com",
            "expires_in": 300
        }))
        .unwrap();
        assert!(response.verification_uri_complete.is_none());
        assert!(response.interval.is_none());
    }

    #[test]
    fn test_token_error_response_deserialize() {
        let response: TokenErrorResponse = serde_json::from_value(json!({
            "error": "authorization_pending"
        }))
        .unwrap();
        assert_eq!(response.error, "authorization_pending");
        assert!(response.error_description.is_none());
    }

    #[test]
    fn test_token_error_response_with_description() {
        let response: TokenErrorResponse = serde_json::from_value(json!({
            "error": "access_denied",
            "error_description": "User denied access"
        }))
        .unwrap();
        assert_eq!(response.error, "access_denied");
        assert_eq!(
            response.error_description,
            Some("User denied access".to_string())
        );
    }

    #[test]
    fn test_format_duration_until_expired() {
        let yesterday = chrono::Utc::now().date_naive() - chrono::Duration::days(1);
        let (s, expired) = format_duration_until(yesterday);
        assert_eq!(s, "expired");
        assert!(expired);
    }

    #[test]
    fn test_format_duration_until_today() {
        let today = chrono::Utc::now().date_naive();
        let (s, expired) = format_duration_until(today);
        assert_eq!(s, "0 days");
        assert!(!expired);
    }

    #[test]
    fn test_format_duration_until_one_day() {
        let tomorrow = chrono::Utc::now().date_naive() + chrono::Duration::days(1);
        let (s, expired) = format_duration_until(tomorrow);
        assert_eq!(s, "1 day");
        assert!(!expired);
    }

    #[test]
    fn test_format_duration_until_multiple_days() {
        let future = chrono::Utc::now().date_naive() + chrono::Duration::days(42);
        let (s, expired) = format_duration_until(future);
        assert_eq!(s, "42 days");
        assert!(!expired);
    }

    #[test]
    fn test_format_duration_until_one_year() {
        let today = chrono::Utc::now().date_naive();
        // Add exactly one year using months to handle leap years correctly
        let one_year_later = today
            .checked_add_months(chrono::Months::new(12))
            .expect("valid date");
        let (s, expired) = format_duration_until(one_year_later);
        assert_eq!(s, "1 year");
        assert!(!expired);
    }

    #[test]
    fn test_format_duration_until_year_and_days() {
        let today = chrono::Utc::now().date_naive();
        let one_year_later = today
            .checked_add_months(chrono::Months::new(12))
            .expect("valid date");
        let target = one_year_later + chrono::Duration::days(30);
        let (s, expired) = format_duration_until(target);
        assert_eq!(s, "1 year, 30 days");
        assert!(!expired);
    }

    #[test]
    fn test_format_duration_until_multiple_years() {
        let today = chrono::Utc::now().date_naive();
        let two_years_later = today
            .checked_add_months(chrono::Months::new(24))
            .expect("valid date");
        let (s, expired) = format_duration_until(two_years_later);
        assert_eq!(s, "2 years");
        assert!(!expired);
    }
}
