use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use time::Duration;

pub const SESSION_COOKIE: &str = "aether_session";

pub fn set_session_cookie(jar: CookieJar, raw_token: &str) -> CookieJar {
    let cookie = Cookie::build((SESSION_COOKIE, raw_token.to_string()))
        .http_only(true)
        .path("/")
        .same_site(SameSite::Lax)
        .max_age(Duration::days(1))
        .build();
    jar.add(cookie)
}

pub fn clear_session_cookie(jar: CookieJar) -> CookieJar {
    let cookie = Cookie::build((SESSION_COOKIE, ""))
        .http_only(true)
        .path("/")
        .same_site(SameSite::Lax)
        .max_age(Duration::seconds(0))
        .build();
    jar.add(cookie)
}

pub fn read_session_token(jar: &CookieJar) -> Option<String> {
    jar.get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .filter(|v| !v.is_empty())
}
