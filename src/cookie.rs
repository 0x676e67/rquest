//! HTTP Cookies

use antidote::RwLock;
use std::convert::TryInto;
use std::fmt;
use std::time::SystemTime;

use crate::header::{HeaderValue, SET_COOKIE};
use bytes::Bytes;

/// Actions for a persistent cookie store providing session support.
pub trait CookieStore: Send + Sync {
    /// Store a set of Set-Cookie header values received from `url`
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &url::Url);
    /// Remove a Cookie value in the store for `url` and `name`
    fn remove_cookie(&self, _url: &url::Url, _name: &str) {}
    /// Get any Cookie values in the store for `url`
    fn cookies(&self, url: &url::Url) -> Option<HeaderValue>;
    /// Remove all cookies from the store.
    fn clear(&self) {}
}

/// A single HTTP cookie.
pub struct Cookie<'a>(cookie_crate::Cookie<'a>);

/// A good default `CookieStore` implementation.
///
/// This is the implementation used when simply calling `cookie_store(true)`.
/// This type is exposed to allow creating one and filling it with some
/// existing cookies more easily, before creating a `Client`.
///
/// For more advanced scenarios, such as needing to serialize the store or
/// manipulate it between requests, you may refer to the
/// [rquest_cookie_store crate](https://crates.io/crates/rquest_cookie_store).
#[derive(Debug)]
pub struct Jar(RwLock<cookie_store::CookieStore>);

// ===== impl Cookie =====

impl<'a> Cookie<'a> {
    fn parse(value: &'a HeaderValue) -> Result<Cookie<'a>, CookieParseError> {
        std::str::from_utf8(value.as_bytes())
            .map_err(cookie_crate::ParseError::from)
            .and_then(cookie_crate::Cookie::parse)
            .map_err(CookieParseError)
            .map(Cookie)
    }

    /// The name of the cookie.
    pub fn name(&self) -> &str {
        self.0.name()
    }

    /// The value of the cookie.
    pub fn value(&self) -> &str {
        self.0.value()
    }

    /// Returns true if the 'HttpOnly' directive is enabled.
    pub fn http_only(&self) -> bool {
        self.0.http_only().unwrap_or(false)
    }

    /// Returns true if the 'Secure' directive is enabled.
    pub fn secure(&self) -> bool {
        self.0.secure().unwrap_or(false)
    }

    /// Returns true if  'SameSite' directive is 'Lax'.
    pub fn same_site_lax(&self) -> bool {
        self.0.same_site() == Some(cookie_crate::SameSite::Lax)
    }

    /// Returns true if  'SameSite' directive is 'Strict'.
    pub fn same_site_strict(&self) -> bool {
        self.0.same_site() == Some(cookie_crate::SameSite::Strict)
    }

    /// Returns the path directive of the cookie, if set.
    pub fn path(&self) -> Option<&str> {
        self.0.path()
    }

    /// Returns the domain directive of the cookie, if set.
    pub fn domain(&self) -> Option<&str> {
        self.0.domain()
    }

    /// Get the Max-Age information.
    pub fn max_age(&self) -> Option<std::time::Duration> {
        self.0.max_age().map(|d| {
            d.try_into()
                .expect("time::Duration into std::time::Duration")
        })
    }

    /// The cookie expiration time.
    pub fn expires(&self) -> Option<SystemTime> {
        match self.0.expires() {
            Some(cookie_crate::Expiration::DateTime(offset)) => Some(SystemTime::from(offset)),
            None | Some(cookie_crate::Expiration::Session) => None,
        }
    }
}

impl fmt::Debug for Cookie<'_> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

pub(crate) fn extract_response_cookie_headers(
    headers: &hyper2::HeaderMap,
) -> impl Iterator<Item = &'_ HeaderValue> {
    headers.get_all(SET_COOKIE).iter()
}

pub(crate) fn extract_response_cookies(
    headers: &hyper2::HeaderMap,
) -> impl Iterator<Item = Result<Cookie<'_>, CookieParseError>> {
    headers.get_all(SET_COOKIE).iter().map(Cookie::parse)
}

/// Error representing a parse failure of a 'Set-Cookie' header.
pub(crate) struct CookieParseError(cookie_crate::ParseError);

impl fmt::Debug for CookieParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for CookieParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for CookieParseError {}

// ===== impl Jar =====

impl Jar {
    /// Add a cookie to this jar.
    ///
    /// # Example
    ///
    /// ```
    /// use rquest::{cookie::Jar, Url};
    ///
    /// let cookie = "foo=bar; Domain=yolo.local";
    /// let url = "https://yolo.local".parse::<Url>().unwrap();
    ///
    /// let jar = Jar::default();
    /// jar.add_cookie_str(cookie, &url);
    ///
    /// // and now add to a `ClientBuilder`?
    /// ```
    pub fn add_cookie_str(&self, cookie: &str, url: &url::Url) {
        let cookies = cookie_crate::Cookie::parse(cookie)
            .ok()
            .map(|c| c.into_owned())
            .into_iter();
        self.0.write().store_response_cookies(cookies, url);
    }
}

impl CookieStore for Jar {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &url::Url) {
        let iter =
            cookie_headers.filter_map(|val| Cookie::parse(val).map(|c| c.0.into_owned()).ok());

        self.0.write().store_response_cookies(iter, url);
    }

    fn remove_cookie(&self, url: &url::Url, name: &str) {
        if let Some(domain) = url.host_str() {
            self.0.write().remove(domain, url.path(), name);
        }
    }

    fn cookies(&self, url: &url::Url) -> Option<HeaderValue> {
        let s = self
            .0
            .read()
            .get_request_values(url)
            .map(|(name, value)| format!("{}={}", name, value))
            .collect::<Vec<_>>()
            .join("; ");

        if s.is_empty() {
            return None;
        }

        HeaderValue::from_maybe_shared(Bytes::from(s)).ok()
    }

    fn clear(&self) {
        self.0.write().clear();
    }
}

impl Default for Jar {
    fn default() -> Self {
        Self(RwLock::new(cookie_store::CookieStore::default()))
    }
}
