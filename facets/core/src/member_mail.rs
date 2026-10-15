//! Queue invite / password-reset emails for organization user management.

use serde_json::json;
use surrealdb::{Surreal, engine::remote::ws::Client};

use crate::messaging::{self, Message};
use crate::scheduler::queue::{self, NewJob};
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct MailResult {
    pub email_sent: bool,
    pub email_error: Option<String>,
}

fn multipart_bodies(subject_line: &str, plain: &str, html_inner: &str) -> (String, String) {
    let html = format!(
        r#"<!DOCTYPE html>
<html><body style="font-family:system-ui,sans-serif;line-height:1.5;color:#111;background:#f6f6f4;padding:24px;">
<div style="max-width:480px;margin:0 auto;background:#fff;padding:28px 24px;border:1px solid #e5e5e0;">
<h1 style="font-size:1.25rem;margin:0 0 12px;">{subject_line}</h1>
{html_inner}
</div>
</body></html>"#
    );
    (plain.to_string(), html)
}

/// Queue one email job on the organization database. Soft-fails when email is not configured.
pub async fn queue_email(
    state: &AppState,
    org_db: &str,
    to: &str,
    subject: &str,
    plain: &str,
    html: &str,
    enqueued_by: Option<&str>,
) -> MailResult {
    if let Err(error) = messaging::check_configured(state, org_db, "email").await {
        return MailResult {
            email_sent: false,
            email_error: Some(error.message),
        };
    }
    let org = match state.org(org_db).await {
        Ok(db) => db,
        Err(error) => {
            return MailResult {
                email_sent: false,
                email_error: Some(error.to_string()),
            };
        }
    };
    let message = Message::Email {
        to: to.to_string(),
        reply_to: None,
        subject: subject.to_string(),
        text: Some(plain.to_string()),
        html: Some(html.to_string()),
    };
    let mut job = NewJob::new("communication", "email");
    job.plugin = Some("aether.user_management".into());
    job.payload = Some(message.to_payload());
    job.enqueued_by = enqueued_by.map(str::to_string);
    match queue::enqueue(&org, job).await {
        Ok(_) => {
            state.scheduler.wake(org_db);
            MailResult {
                email_sent: true,
                email_error: None,
            }
        }
        Err(error) => MailResult {
            email_sent: false,
            email_error: Some(error.to_string()),
        },
    }
}

pub fn invite_bodies(
    org_name: &str,
    username: &str,
    password: &str,
    login_url: &str,
) -> (String, String, String) {
    let subject = format!("You have been invited to {org_name}");
    let plain = format!(
        "You have been invited to {org_name}.\n\n\
         Username: {username}\n\
         Temporary password: {password}\n\n\
         Sign in: {login_url}\n\
         Change your password after your first login.\n"
    );
    let inner = format!(
        r#"<p>You have been invited to <strong>{org}</strong>.</p>
<p style="margin:16px 0;padding:12px;background:#f4f4f0;font-family:ui-monospace,monospace;font-size:0.9rem;">
Username: {user}<br>Temporary password: {pass}
</p>
<p><a href="{url}" style="display:inline-block;padding:10px 16px;background:#111;color:#fff;text-decoration:none;">Sign in</a></p>
<p style="font-size:0.875rem;color:#555;">Change your password after your first login.</p>"#,
        org = html_escape(org_name),
        user = html_escape(username),
        pass = html_escape(password),
        url = html_escape(login_url),
    );
    let (plain, html) = multipart_bodies(&subject, &plain, &inner);
    (subject, plain, html)
}

pub fn reset_bodies(
    org_name: &str,
    reset_url: &str,
    expires_note: &str,
) -> (String, String, String) {
    let subject = format!("Reset your password for {org_name}");
    let plain = format!(
        "Reset your password for {org_name}.\n\n\
         Open this link ({expires_note}):\n{reset_url}\n\n\
         If you did not expect this, you can ignore the message.\n"
    );
    let inner = format!(
        r#"<p>Reset your password for <strong>{org}</strong>.</p>
<p style="font-size:0.875rem;color:#555;">{note}</p>
<p><a href="{url}" style="display:inline-block;padding:10px 16px;background:#111;color:#fff;text-decoration:none;">Choose a new password</a></p>
<p style="font-size:0.875rem;color:#555;">If you did not expect this, you can ignore the message.</p>"#,
        org = html_escape(org_name),
        note = html_escape(expires_note),
        url = html_escape(reset_url),
    );
    let (plain, html) = multipart_bodies(&subject, &plain, &inner);
    (subject, plain, html)
}

pub fn account_invite_bodies(
    org_name: &str,
    invite_url: &str,
    expires_note: &str,
) -> (String, String, String) {
    let subject = format!("Create your account for {org_name}");
    let plain = format!(
        "Create your account for {org_name}.\n\n\
         Open this link ({expires_note}):\n{invite_url}\n\n\
         If you did not expect this, you can ignore the message.\n"
    );
    let inner = format!(
        r#"<p>Create your account for <strong>{org}</strong>.</p>
<p style="font-size:0.875rem;color:#555;">{note}</p>
<p><a href="{url}" style="display:inline-block;padding:10px 16px;background:#111;color:#fff;text-decoration:none;">Choose a password</a></p>
<p style="font-size:0.875rem;color:#555;">If you did not expect this, you can ignore the message.</p>"#,
        org = html_escape(org_name),
        note = html_escape(expires_note),
        url = html_escape(invite_url),
    );
    let (plain, html) = multipart_bodies(&subject, &plain, &inner);
    (subject, plain, html)
}

pub fn joined_bodies(org_name: &str, login_url: &str) -> (String, String, String) {
    let subject = format!("You have been added to {org_name}");
    let plain = format!(
        "You have been added to {org_name}.\n\n\
         Sign in with your existing account:\n{login_url}\n"
    );
    let inner = format!(
        r#"<p>You have been added to <strong>{org}</strong>.</p>
<p><a href="{url}" style="display:inline-block;padding:10px 16px;background:#111;color:#fff;text-decoration:none;">Sign in</a></p>
<p style="font-size:0.875rem;color:#555;">Use the password you already have.</p>"#,
        org = html_escape(org_name),
        url = html_escape(login_url),
    );
    let (plain, html) = multipart_bodies(&subject, &plain, &inner);
    (subject, plain, html)
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The settings key holding the address people use to reach the application.
pub const DOMAIN_KEY: &str = "domain";

/// The origin (`https://app.example.com`) for what an administrator typed as the domain: a bare
/// host name (`app.example.com`, `localhost:5173`) or a full address, with any path ignored.
/// A bare host gets `https://`, except local ones, which get `http://`. `None` when it is blank
/// or not an address a link could use.
pub fn origin_from_domain(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let (scheme, rest) = match raw.split_once("://") {
        Some((scheme, rest)) => {
            let scheme = scheme.to_ascii_lowercase();
            if scheme != "http" && scheme != "https" {
                return None;
            }
            (Some(scheme), rest)
        }
        None => (None, raw),
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if authority.is_empty()
        || !authority
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
    {
        return None;
    }
    let host = authority
        .trim_start_matches('[')
        .split([']', ':'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if host.is_empty() {
        return None;
    }
    let local = host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.parse::<std::net::IpAddr>().is_ok();
    let scheme = scheme.unwrap_or_else(|| if local { "http" } else { "https" }.to_string());
    Some(format!("{scheme}://{}", authority.to_ascii_lowercase()))
}

/// The origin for links in an organization's emails: its `domain` setting, else the global one,
/// else a guess from the server's own configuration ([`public_base`]).
pub async fn public_base_for(state: &AppState, org_db: &str) -> String {
    let org = crate::tenancy::OrgRef {
        slug: org_db.to_string(),
        db_name: org_db.to_string(),
    };
    match crate::application::settings::get_setting_text(state, DOMAIN_KEY, Some(&org)).await {
        Ok(Some(text)) => {
            if let Some(origin) = origin_from_domain(&text) {
                return origin;
            }
        }
        Ok(None) => {}
        Err(error) => log::warn!("domain setting for `{org_db}`: {error}"),
    }
    public_base(state)
}

/// Best-effort public origin for links in emails when no domain is configured.
pub fn public_base(state: &AppState) -> String {
    let server = state.config.server.clone().unwrap_or_default();
    if let Some(origin) = server.csrf_trusted_origins.first() {
        return origin.trim_end_matches('/').to_string();
    }
    let host = if server.host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        server.host.as_str()
    };
    format!("http://{host}:{}", server.port)
}

/// Resolve a member's email from core by user id.
pub async fn email_for_user(
    db: &Surreal<Client>,
    namespace: &str,
    user_id: &str,
) -> Option<String> {
    let _ = db.use_ns(namespace).await.ok()?;
    let _ = db.use_db("core").await.ok()?;
    let mut response = db
        .query("SELECT VALUE email FROM type::record('users', $key) LIMIT 1;")
        .bind((
            "key",
            user_id
                .strip_prefix("users:")
                .unwrap_or(user_id)
                .to_string(),
        ))
        .await
        .ok()?
        .check()
        .ok()?;
    let emails: Vec<Option<String>> = response.take(0).ok()?;
    emails.into_iter().flatten().next()
}

/// Small helper so callers can log without pulling serde_json.
pub fn log_mail(kind: &str, result: &MailResult) {
    if result.email_sent {
        log::info!("{kind} email queued");
    } else {
        log::warn!(
            "{kind} email not sent: {}",
            result.email_error.as_deref().unwrap_or("unknown")
        );
    }
    let _ = json!({});
}

#[cfg(test)]
mod domain_tests {
    use super::origin_from_domain;

    #[test]
    fn a_bare_host_gets_https() {
        assert_eq!(
            origin_from_domain("app.example.com").as_deref(),
            Some("https://app.example.com")
        );
        assert_eq!(
            origin_from_domain("  App.Example.com/  ").as_deref(),
            Some("https://app.example.com")
        );
    }

    #[test]
    fn local_hosts_get_http() {
        assert_eq!(
            origin_from_domain("localhost:5173").as_deref(),
            Some("http://localhost:5173")
        );
        assert_eq!(
            origin_from_domain("192.168.1.20:7890").as_deref(),
            Some("http://192.168.1.20:7890")
        );
        assert_eq!(
            origin_from_domain("erp.local").as_deref(),
            Some("http://erp.local")
        );
    }

    #[test]
    fn a_full_address_keeps_its_scheme_and_loses_its_path() {
        assert_eq!(
            origin_from_domain("https://app.example.com/login?x=1").as_deref(),
            Some("https://app.example.com")
        );
        assert_eq!(
            origin_from_domain("http://app.example.com:8080/").as_deref(),
            Some("http://app.example.com:8080")
        );
    }

    #[test]
    fn refuses_what_a_link_cannot_use() {
        for bad in [
            "",
            "   ",
            "ftp://example.com",
            "javascript:alert(1)",
            "exa mple.com",
            "https://",
            "//evil",
            "https://user@",
        ] {
            assert_eq!(origin_from_domain(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn credentials_in_the_address_are_dropped() {
        assert_eq!(
            origin_from_domain("https://admin:pw@app.example.com").as_deref(),
            Some("https://app.example.com")
        );
    }
}
