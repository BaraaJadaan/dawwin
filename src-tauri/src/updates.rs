//! "Check for updates": ask GitHub for the latest published release. Only runs when the user asks
//! (or turned on checking at startup).

use std::process::{Command, Stdio};

use anyhow::{bail, Result};
use serde::Serialize;

use crate::setup::no_window;

pub const REPO: &str = "BaraaJadaan/dawwin";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, Serialize)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub newer: bool,
    /// The release page (notes and all files)
    pub page: String,
    /// The installer of the latest release, when it has one
    pub installer: Option<String>,
}

/// `v1.2.3` / `1.2.3` → (1, 2, 3); anything after a `-` is ignored.
fn parse(v: &str) -> (u64, u64, u64) {
    let core = v.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next().unwrap_or("");
    let mut it = core.split('.').map(|p| p.parse::<u64>().unwrap_or(0));
    (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0))
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    parse(latest) > parse(current)
}

pub fn check() -> Result<UpdateInfo> {
    let mut cmd = Command::new("curl.exe");
    cmd.args(["-sS", "-L", "-m", "30", "--retry", "2", "-w", "\n%{http_code}"])
        .args(["-H", "Accept: application/vnd.github+json", "-H", "User-Agent: Dawwin"])
        .arg(format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .stdin(Stdio::null());
    no_window(&mut cmd);
    let out = cmd.output()?;
    if !out.status.success() {
        bail!(crate::tr!("Could not reach GitHub. Check the connection and try again."));
    }
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let (body, code) = text.rsplit_once('\n').unwrap_or((&text, ""));
    match code.trim() {
        "200" => {}
        "404" => bail!(crate::tr!("No version has been published yet.")),
        c => bail!(crate::tr!("GitHub answered {}. Try again later.", c)),
    }
    let v: serde_json::Value = serde_json::from_str(body)?;
    let latest = v.get("tag_name").and_then(|t| t.as_str()).unwrap_or("").to_string();
    let page = v
        .get("html_url")
        .and_then(|t| t.as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("https://github.com/{REPO}/releases/latest"));
    let installer = v
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|a| {
            a.iter()
                .filter_map(|x| x.get("browser_download_url")?.as_str())
                .find(|u| u.ends_with("-setup.exe"))
                .map(String::from)
        });
    Ok(UpdateInfo {
        current: VERSION.to_string(),
        newer: is_newer(&latest, VERSION),
        latest: latest.trim_start_matches(['v', 'V']).to_string(),
        page,
        installer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.2.0", "0.1.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(!is_newer("v0.1.0", "0.1.0"));
        assert!(!is_newer("v0.0.9", "0.1.0"));
        assert!(!is_newer("garbage", "0.1.0"));
    }
}
