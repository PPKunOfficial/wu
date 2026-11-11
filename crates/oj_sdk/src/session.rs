use std::collections::BTreeMap;

use crate::error::{Error, Result};

/// The cookies for one Luogu account.
///
/// Luogu keeps the session in the HttpOnly `__client_id` and `_uid` cookies, so
/// importing a session means pasting the cookie string from a browser rather
/// than reading `document.cookie`. Cookies rotate (`C3VK` on every captcha
/// fetch), which is why the client updates the session from `Set-Cookie`
/// responses.
#[derive(Debug, Clone, Default)]
pub struct Session {
    cookies: BTreeMap<String, String>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses a `Cookie` header value, such as
    /// `__client_id=...; _uid=311493; C3VK=123456`.
    ///
    /// A leading `Cookie:` (case-insensitive) is tolerated because that is what
    /// browser developer tools put on the clipboard.
    pub fn from_cookie_string(raw: &str) -> Result<Self> {
        let raw = raw.trim();
        let raw = raw
            .strip_prefix("Cookie:")
            .or_else(|| raw.strip_prefix("cookie:"))
            .unwrap_or(raw)
            .trim();

        let mut session = Self::default();
        for part in raw.split(';') {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let Some((name, value)) = part.split_once('=') else {
                return Err(Error::InvalidCookie {
                    message: format!("missing '=' in {part:?}"),
                });
            };
            let name = name.trim();
            if name.is_empty() {
                return Err(Error::InvalidCookie {
                    message: format!("missing cookie name in {part:?}"),
                });
            }
            session
                .cookies
                .insert(name.to_string(), value.trim().to_string());
        }
        Ok(session)
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.cookies.insert(name.into(), value.into());
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.cookies.get(name).map(String::as_str)
    }

    pub fn remove(&mut self, name: &str) -> Option<String> {
        self.cookies.remove(name)
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    pub fn cookies(&self) -> impl Iterator<Item = (&str, &str)> {
        self.cookies
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
    }

    pub fn to_cookie_string(&self) -> String {
        let mut parts = Vec::with_capacity(self.cookies.len());
        for (name, value) in &self.cookies {
            parts.push(format!("{name}={value}"));
        }
        parts.join("; ")
    }

    /// The numeric user id stored in the `_uid` cookie.
    pub fn uid(&self) -> Option<u64> {
        self.cookies.get("_uid")?.parse().ok()
    }

    /// Applies a single `Set-Cookie` header value received from Luogu.
    pub fn update_from_set_cookie(&mut self, header: &str) {
        let Some(pair) = header.split(';').next() else {
            return;
        };
        let Some((name, value)) = pair.split_once('=') else {
            return;
        };
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        self.cookies
            .insert(name.to_string(), value.trim().to_string());
    }

    pub(crate) fn update_from_headers<'a>(&mut self, headers: impl Iterator<Item = &'a str>) {
        for header in headers {
            self.update_from_set_cookie(header);
        }
    }
}
