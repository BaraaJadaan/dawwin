//! The way out to the internet, for downloads and the update check: through the proxy Windows is
//! set to use (Settings, Network and internet, Proxy, "Use a proxy server"), or the one
//! `HTTPS_PROXY` names. Automatic proxy scripts (PAC) are not read, and a proxy that asks for a
//! sign-in is told apart, not signed in to. Addresses on this computer never go through a proxy.

use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::Result;

use crate::setup::no_window;

/// The proxy for `url`, as `http://host:port`, if one is set and the host is not left out of it.
pub fn proxy_for(url: &str) -> Option<String> {
    let host = host_of(url)?;
    if is_local(&host) {
        return None;
    }
    let https = url.starts_with("https:");
    let from_env = |names: &[&str]| names.iter().find_map(|n| std::env::var(n).ok()).filter(|v| !v.trim().is_empty());
    let env = if https { from_env(&["HTTPS_PROXY", "https_proxy", "ALL_PROXY", "all_proxy"]) } else { from_env(&["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"]) };
    if let Some(p) = env {
        let skip = from_env(&["NO_PROXY", "no_proxy"]).unwrap_or_default();
        return (!bypassed(&host, skip.split(',').map(str::trim))).then(|| with_scheme(&p));
    }
    let w = windows_setting()?;
    (!bypassed(&host, w.bypass.split(';').map(str::trim))).then_some(w.server)
}

/// The host of a URL, in lower case, without its port.
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next()?;
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(v6) = authority.strip_prefix('[') { v6.split(']').next()? } else { authority.split(':').next()? };
    (!host.is_empty()).then(|| host.to_lowercase())
}

fn is_local(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost") || host == "::1" || host.starts_with("127.")
}

fn with_scheme(p: &str) -> String {
    let p = p.trim().trim_end_matches('/');
    if p.contains("://") {
        p.to_string()
    } else {
        format!("http://{p}")
    }
}

/// Whether `host` is one the proxy is not used for: Windows' list (`<local>` for names without a
/// dot, `*` for any part) or `NO_PROXY` (domains, which also cover their subdomains).
fn bypassed<'a>(host: &str, rules: impl Iterator<Item = &'a str>) -> bool {
    rules.filter(|r| !r.is_empty()).any(|rule| {
        let rule = rule.to_lowercase();
        if rule == "<local>" {
            return !host.contains('.');
        }
        if rule == "*" {
            return true;
        }
        if rule.contains('*') {
            return wildcard(&rule, host);
        }
        let rule = rule.trim_start_matches('.');
        host == rule || host.ends_with(&format!(".{rule}"))
    })
}

fn wildcard(pattern: &str, text: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut at = 0;
    for (k, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match text[at..].find(part) {
            Some(i) if k > 0 || i == 0 => at += i + part.len(),
            _ => return false,
        }
    }
    parts.last().is_some_and(|p| p.is_empty()) || at == text.len()
}

struct WindowsProxy {
    server: String,
    bypass: String,
}

/// The manual proxy of the Windows settings, read from the registry, when it is on.
fn windows_setting() -> Option<WindowsProxy> {
    let mut cmd = Command::new("reg.exe");
    cmd.args(["query", r"HKCU\Software\Microsoft\Windows\CurrentVersion\Internet Settings"]).stdin(Stdio::null()).stderr(Stdio::null());
    no_window(&mut cmd);
    let out = cmd.output().ok()?;
    parse_windows_setting(&String::from_utf8_lossy(&out.stdout))
}

fn parse_windows_setting(listing: &str) -> Option<WindowsProxy> {
    let value = |name: &str| {
        listing.lines().find_map(|l| {
            let mut parts = l.split_whitespace();
            (parts.next()? == name).then(|| {
                parts.next(); // the type
                parts.collect::<Vec<_>>().join(" ")
            })
        })
    };
    let enabled = value("ProxyEnable").is_some_and(|v| v.trim_start_matches("0x").parse::<u32>().unwrap_or(0) != 0);
    if !enabled {
        return None;
    }
    let server = value("ProxyServer")?;
    // "host:port", or one for each kind: "http=host:port;https=host:port;socks=…"
    let server = if server.contains('=') {
        let kinds: Vec<(String, String)> = server.split(';').filter_map(|p| p.split_once('=')).map(|(k, v)| (k.trim().to_lowercase(), v.trim().to_string())).collect();
        kinds.iter().find(|(k, _)| k == "https").or_else(|| kinds.iter().find(|(k, _)| k == "http")).map(|(_, v)| v.clone())?
    } else {
        server
    };
    if server.trim().is_empty() || server.starts_with("socks") {
        return None;
    }
    Some(WindowsProxy { server: with_scheme(&server), bypass: value("ProxyOverride").unwrap_or_default() })
}

/// A client for downloads: through `proxy` when there is one.
pub fn agent(proxy: Option<&str>) -> Result<ureq::Agent> {
    let mut b = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        // a connection that sends nothing for this long is dead, however slow it was before
        .timeout_read(Duration::from_secs(60))
        .tls_connector(std::sync::Arc::new(native_tls::TlsConnector::new()?));
    if let Some(p) = proxy {
        b = b.proxy(ureq::Proxy::new(p)?);
    }
    Ok(b.build())
}

/// Why a request failed, in words for the person waiting for it.
pub fn why(e: &ureq::Error, proxy: Option<&str>) -> String {
    let proxy_name = || proxy.map(|p| p.trim_start_matches("http://").to_string()).unwrap_or_default();
    match e {
        ureq::Error::Status(407, _) => crate::tr!("The proxy asks for a sign-in, which Dawwin cannot give it. Ask whoever looks after the network, or download the file by hand."),
        ureq::Error::Status(code, _) if *code == 429 || *code >= 500 => crate::tr!("The server is busy (error {}).", code),
        ureq::Error::Status(code, _) => crate::tr!("The server refused the download (error {}).", code),
        ureq::Error::Transport(t) => match t.kind() {
            ureq::ErrorKind::ProxyUnauthorized => crate::tr!("The proxy asks for a sign-in, which Dawwin cannot give it. Ask whoever looks after the network, or download the file by hand."),
            ureq::ErrorKind::ProxyConnect => crate::tr!("The proxy {} does not answer.", proxy_name()),
            ureq::ErrorKind::Dns if proxy.is_some() => crate::tr!("The proxy {} does not answer.", proxy_name()),
            ureq::ErrorKind::Dns => crate::tr!("No connection to the internet: the server's name could not be looked up."),
            ureq::ErrorKind::ConnectionFailed => crate::tr!("The server could not be reached: the network may be blocking it."),
            _ => crate::tr!("The connection stopped answering."),
        },
    }
}

/// `curl` arguments for the same proxy.
pub fn curl_args(url: &str) -> Vec<String> {
    proxy_for(url).map(|p| vec!["-x".to_string(), p]).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_is_read_from_any_url() {
        assert_eq!(host_of("https://huggingface.co/x/y").as_deref(), Some("huggingface.co"));
        assert_eq!(host_of("http://user:pw@Example.com:8080/a").as_deref(), Some("example.com"));
        assert_eq!(host_of("http://[::1]:9/").as_deref(), Some("::1"));
        assert!(is_local("127.0.0.1") && is_local("localhost") && !is_local("hf.co"));
    }

    #[test]
    fn the_windows_setting_is_read_as_the_registry_lists_it() {
        let listing = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings\r\n    ProxyEnable    REG_DWORD    0x1\r\n    ProxyServer    REG_SZ    proxy.uni.edu:3128\r\n    ProxyOverride    REG_SZ    <local>;*.uni.edu;10.*\r\n";
        let w = parse_windows_setting(listing).unwrap();
        assert_eq!(w.server, "http://proxy.uni.edu:3128");
        let rules = || w.bypass.split(';').map(str::trim);
        assert!(bypassed("intranet", rules()), "<local>: a name without a dot");
        assert!(bypassed("library.uni.edu", rules()));
        assert!(bypassed("10.1.2.3", rules()));
        assert!(!bypassed("huggingface.co", rules()));
        let per_kind = listing.replace("proxy.uni.edu:3128", "http=p1:80;https=p2:443;socks=p3:1080");
        assert_eq!(parse_windows_setting(&per_kind).unwrap().server, "http://p2:443");
        assert!(parse_windows_setting(&listing.replace("0x1", "0x0")).is_none(), "switched off");
    }

    #[test]
    fn no_proxy_covers_a_domain_and_its_subdomains() {
        let rules = || ".corp.example, github.com".split(',').map(str::trim);
        assert!(bypassed("git.corp.example", rules()));
        assert!(bypassed("api.github.com", rules()));
        assert!(!bypassed("huggingface.co", rules()));
    }
}
