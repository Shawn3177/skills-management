//! The GitHub account the user connects. Its token is validated against
//! `api.github.com/user`, then kept in the OS credential store (never in a
//! file and never sent to the UI). A token in the environment
//! (`SKILLS_MANAGE_GITHUB_TOKEN`, `GITHUB_TOKEN`, `GH_TOKEN`) still wins.

use crate::github::{self, GithubError};
use serde::Serialize;
use std::process::Command;

const KEYRING_SERVICE: &str = "skills-manage";
const KEYRING_USER: &str = "github";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubAccount {
    pub connected: bool,
    pub login: Option<String>,
    pub name: Option<String>,
    /// "stored" (connected in the app) or "environment" (an env var token).
    pub source: Option<String>,
}

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|error| format!("Could not open the system credential store: {error}"))
}

/// The token connected in the app, if any.
pub(crate) fn stored_token() -> Option<String> {
    entry()
        .ok()?
        .get_password()
        .ok()
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

pub(crate) fn environment_token() -> Option<String> {
    ["SKILLS_MANAGE_GITHUB_TOKEN", "GITHUB_TOKEN", "GH_TOKEN"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

/// Who the token belongs to (`login`, `name`).
fn whoami(token: &str) -> Result<(String, Option<String>), String> {
    let agent = github::build_agent()?;
    let user = github::api_get_json_with_token(&agent, "https://api.github.com/user", Some(token))
        .map_err(|error| match error {
            GithubError::RateLimited => error.message(),
            GithubError::Other(detail) if detail.starts_with("Network error") => detail,
            _ => "GitHub did not accept this token.".to_string(),
        })?;
    let login = user
        .get("login")
        .and_then(|value| value.as_str())
        .ok_or_else(|| "GitHub did not return the account for this token.".to_string())?
        .to_string();
    let name = user.get("name").and_then(|value| value.as_str()).map(str::to_string);
    Ok((login, name))
}

pub fn github_account() -> Result<GithubAccount, String> {
    let (token, source) = match (environment_token(), stored_token()) {
        (Some(token), _) => (token, "environment"),
        (None, Some(token)) => (token, "stored"),
        (None, None) => {
            return Ok(GithubAccount { connected: false, login: None, name: None, source: None });
        }
    };
    // A token that no longer works (revoked, expired) shows as connected
    // without a login, so the UI can offer to reconnect.
    let (login, name) = match whoami(&token) {
        Ok((login, name)) => (Some(login), name),
        Err(_) => (None, None),
    };
    Ok(GithubAccount { connected: true, login, name, source: Some(source.to_string()) })
}

/// Connect with a personal access token the user pasted.
pub fn connect_github_token(token: String) -> Result<GithubAccount, String> {
    let token = token.trim().to_string();
    if token.is_empty() || token.chars().any(char::is_whitespace) {
        return Err("Paste a GitHub token (it has no spaces).".to_string());
    }
    let (login, name) = whoami(&token)?;
    entry()?
        .set_password(&token)
        .map_err(|error| format!("Could not save the token in the system credential store: {error}"))?;
    Ok(GithubAccount { connected: true, login: Some(login), name, source: Some("stored".to_string()) })
}

/// Connect with the account GitHub CLI (`gh`) is signed in to.
pub fn connect_github_cli() -> Result<GithubAccount, String> {
    let mut command = Command::new("gh");
    command.args(["auth", "token", "--hostname", "github.com"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command
        .output()
        .map_err(|_| "GitHub CLI (gh) was not found. Install it, or paste a token instead.".to_string())?;
    if !output.status.success() {
        return Err("GitHub CLI is not signed in. Run `gh auth login`, or paste a token instead.".to_string());
    }
    connect_github_token(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Forget the connected token (the account itself is not touched).
pub fn disconnect_github() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(error) => return Err(format!("Could not remove the token: {error}")),
    }
    crate::catalog::forget_cache();
    Ok(())
}
