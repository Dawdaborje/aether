//! Organization membership commands plugins may call.
//!
//! `org::members` lists login accounts. `org::create_member` creates one with a generated
//! password, or adds an existing account. `org::invite` emails a link so a new person sets
//! their own password. Both write only the organization this call is already serving.
//! Plugins never choose a password and never mint `auth.*` links themselves.

use serde_json::{Value as JsonValue, json};
use surrealdb::types::SurrealValue;

use crate::magic_link::{self, CreateMagicLink, PURPOSE_ACCOUNT_INVITE};
use crate::member_mail::{self, MailResult};
use crate::messaging::Message;
use crate::org_admin::{self, OrganizationError};
use crate::scheduler::queue::{self, NewJob, QueueError};

use super::context::PluginHostContext;
use super::error::HostError;

#[derive(Debug, SurrealValue)]
struct MemberRow {
    core_user_id: String,
    display_name: Option<String>,
}

/// Active members of the caller's organization. Each row is `{ id, display_name }` where `id`
/// is the account id (`users:…`) stored on an employee.
pub async fn members(
    ctx: &PluginHostContext,
    _payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("org::members")?;
    let mut response = ctx
        .db
        .query(
            "SELECT core_user_id, display_name FROM org_users \
             WHERE is_active = true AND deleted_at = NONE LIMIT 500;",
        )
        .await
        .map_err(HostError::Db)?
        .check()
        .map_err(HostError::Db)?;
    let rows: Vec<MemberRow> = response.take(0).map_err(HostError::Db)?;
    let data: Vec<JsonValue> = rows
        .into_iter()
        .filter(|row| !row.core_user_id.trim().is_empty())
        .map(|row| {
            let id = row.core_user_id;
            let display_name = row
                .display_name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| id.clone());
            json!({ "id": id, "display_name": display_name })
        })
        .collect();
    Ok(json!({ "ok": true, "data": data }))
}

/// Create a login in this organization, or add an existing account. The password is generated.
pub async fn create_member(
    ctx: &PluginHostContext,
    payload: &JsonValue,
) -> Result<JsonValue, HostError> {
    ctx.require_cap("org::create_member")?;
    refuse_password(payload)?;
    let email = require_email(payload)?;
    let username = optional_text(payload, "username");
    let display_name = optional_display_name(payload)?;
    let session = open_session(ctx)?;
    let (login, temporary_password) = org_admin::invite_member(
        &session,
        &ctx.namespace,
        &ctx.database,
        &email,
        username.as_deref(),
    )
    .await
    .map_err(map_org)?;
    let (user_id, username) = org_admin::user_by_email(&session, &ctx.namespace, &email)
        .await
        .map_err(map_org)?
        .unwrap_or((String::new(), login));
    if let Some(name) = display_name.as_deref() {
        org_admin::set_org_display_name(&session, &ctx.namespace, &ctx.database, &user_id, name)
            .await
            .map_err(map_org)?;
    }
    bust_membership(ctx, &user_id);
    let created = !temporary_password.is_empty();
    let mut email_sent = false;
    let mut email_error = None;
    if created {
        let org_name = org_label(&session, ctx).await;
        let login_url = format!("{}/login", site_base(ctx));
        let (subject, plain, html) =
            member_mail::invite_bodies(&org_name, &username, &temporary_password, &login_url);
        let mail = queue_email(ctx, &email, &subject, &plain, &html).await;
        member_mail::log_mail("org::create_member", &mail);
        email_sent = mail.email_sent;
        email_error = mail.email_error;
    }
    Ok(json!({
        "ok": true,
        "data": {
            "user_id": user_id,
            "username": username,
            "created": created,
            "temporary_password": temporary_password,
            "email_sent": email_sent,
            "email_error": email_error,
        }
    }))
}

/// Invite a new person to choose a password, or add someone who already has an account.
pub async fn invite(ctx: &PluginHostContext, payload: &JsonValue) -> Result<JsonValue, HostError> {
    ctx.require_cap("org::invite")?;
    refuse_password(payload)?;
    let email = require_email(payload)?;
    let username = optional_text(payload, "username");
    let display_name = optional_display_name(payload)?;
    let session = open_session(ctx)?;
    if let Some((user_id, login)) = org_admin::user_by_email(&session, &ctx.namespace, &email)
        .await
        .map_err(map_org)?
    {
        org_admin::assign_user(&session, &ctx.namespace, &email, &ctx.database)
            .await
            .map_err(map_org)?;
        if let Some(name) = display_name.as_deref() {
            org_admin::set_org_display_name(
                &session,
                &ctx.namespace,
                &ctx.database,
                &user_id,
                name,
            )
            .await
            .map_err(map_org)?;
        }
        bust_membership(ctx, &user_id);
        let org_name = org_label(&session, ctx).await;
        let login_url = format!("{}/login", site_base(ctx));
        let (subject, plain, html) = member_mail::joined_bodies(&org_name, &login_url);
        let mail = queue_email(ctx, &email, &subject, &plain, &html).await;
        member_mail::log_mail("org::invite joined", &mail);
        return Ok(json!({
            "ok": true,
            "data": {
                "status": "joined",
                "user_id": user_id,
                "username": login,
                "email_sent": mail.email_sent,
                "email_error": mail.email_error,
            }
        }));
    }

    session
        .use_ns(&ctx.namespace)
        .use_db("core")
        .await
        .map_err(HostError::Db)?;
    session
        .query(
            "UPDATE magic_links SET revoked_at = time::now() \
             WHERE purpose = $purpose AND email = $email AND org_db = $org \
               AND used_at = NONE AND revoked_at = NONE;",
        )
        .bind(("purpose", PURPOSE_ACCOUNT_INVITE.to_string()))
        .bind(("email", email.clone()))
        .bind(("org", ctx.database.clone()))
        .await
        .map_err(HostError::Db)?
        .check()
        .map_err(HostError::Db)?;

    let mut link_payload = serde_json::Map::new();
    link_payload.insert("email".into(), json!(email));
    link_payload.insert("org_db".into(), json!(ctx.database));
    link_payload.insert("invited_by_plugin".into(), json!(ctx.plugin_name));
    if let Some(username) = username.as_deref() {
        link_payload.insert("username".into(), json!(username));
    }
    if let Some(name) = display_name.as_deref() {
        link_payload.insert("display_name".into(), json!(name));
    }
    let created = magic_link::create(
        &session,
        &ctx.namespace,
        CreateMagicLink {
            purpose: PURPOSE_ACCOUNT_INVITE.to_string(),
            user_id: None,
            email: Some(email.clone()),
            payload: Some(JsonValue::Object(link_payload)),
            plugin: None,
            org_db: Some(ctx.database.clone()),
            path: Some("/login/magic".into()),
            ttl_secs: None,
            single_use: true,
            supersede: false,
            created_by: ctx.audit.actor.id().map(str::to_string),
            public_base: site_base(ctx),
        },
    )
    .await
    .map_err(|error| HostError::Message(error.to_string()))?;

    let org_name = org_label(&session, ctx).await;
    let (subject, plain, html) =
        member_mail::account_invite_bodies(&org_name, &created.url, "This link expires in 7 days.");
    let mail = queue_email(ctx, &email, &subject, &plain, &html).await;
    member_mail::log_mail("org::invite", &mail);
    Ok(json!({
        "ok": true,
        "data": {
            "status": "invited",
            "expires_at": created.expires_at,
            "email_sent": mail.email_sent,
            "email_error": mail.email_error,
            "url": created.url,
        }
    }))
}

fn refuse_password(payload: &JsonValue) -> Result<(), HostError> {
    if payload.get("password").is_some() || payload.get("new_password").is_some() {
        return Err(HostError::InvalidPayload(
            "this command does not accept a password".into(),
        ));
    }
    Ok(())
}

fn require_email(payload: &JsonValue) -> Result<String, HostError> {
    let email = optional_text(payload, "email")
        .ok_or_else(|| HostError::InvalidPayload("email is required".into()))?;
    if !email.contains('@') || email.chars().any(char::is_whitespace) {
        return Err(HostError::InvalidPayload(
            "enter a valid email address".into(),
        ));
    }
    Ok(email)
}

fn optional_text(payload: &JsonValue, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn optional_display_name(payload: &JsonValue) -> Result<Option<String>, HostError> {
    let Some(name) = optional_text(payload, "display_name") else {
        return Ok(None);
    };
    if name.chars().count() > 120 {
        return Err(HostError::InvalidPayload(
            "display_name must be at most 120 characters".into(),
        ));
    }
    Ok(Some(name))
}

fn open_session(
    ctx: &PluginHostContext,
) -> Result<surrealdb::Surreal<surrealdb::engine::remote::ws::Client>, HostError> {
    let open = ctx
        .services()?
        .open_session
        .as_ref()
        .ok_or_else(|| HostError::Message("creating accounts is not available here".into()))?;
    Ok(open())
}

fn site_base(ctx: &PluginHostContext) -> String {
    ctx.services
        .as_ref()
        .map(|services| {
            services
                .public_base
                .trim()
                .trim_end_matches('/')
                .to_string()
        })
        .filter(|base| !base.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:7890".into())
}

async fn org_label(
    session: &surrealdb::Surreal<surrealdb::engine::remote::ws::Client>,
    ctx: &PluginHostContext,
) -> String {
    match org_admin::get_profile(session, &ctx.namespace, &ctx.database).await {
        Ok(profile) => profile.name,
        Err(_) => ctx.database.clone(),
    }
}

fn bust_membership(ctx: &PluginHostContext, user_id: &str) {
    let Some(services) = ctx.services.as_ref() else {
        return;
    };
    let _ = services.cache.warm_invalidate_memberships(user_id);
    let _ = services.cache.warm_invalidate_roles(&ctx.database, user_id);
}

async fn queue_email(
    ctx: &PluginHostContext,
    to: &str,
    subject: &str,
    plain: &str,
    html: &str,
) -> MailResult {
    let Some(scheduler) = ctx
        .services
        .as_ref()
        .and_then(|services| services.scheduler.as_ref())
    else {
        return MailResult {
            email_sent: false,
            email_error: Some("sending messages is not available here".into()),
        };
    };
    if let Err(error) = scheduler.check_messaging(&ctx.database, "email").await {
        return MailResult {
            email_sent: false,
            email_error: Some(error),
        };
    }
    let message = Message::Email {
        to: to.to_string(),
        reply_to: None,
        subject: subject.to_string(),
        text: Some(plain.to_string()),
        html: Some(html.to_string()),
    };
    let mut job = NewJob::new("communication", message.queue());
    job.plugin = Some(ctx.plugin_name.clone());
    job.payload = Some(message.to_payload());
    job.enqueued_by = ctx.audit.actor.id().map(str::to_string);
    job.request_id = Some(ctx.audit.request_id.clone());
    match queue::enqueue(&ctx.db, job).await {
        Ok(_) => {
            scheduler.wake(&ctx.database);
            MailResult {
                email_sent: true,
                email_error: None,
            }
        }
        Err(QueueError::Refused(message)) => MailResult {
            email_sent: false,
            email_error: Some(message),
        },
        Err(QueueError::Db(error)) => MailResult {
            email_sent: false,
            email_error: Some(error.to_string()),
        },
    }
}

fn map_org(error: OrganizationError) -> HostError {
    match error {
        OrganizationError::Surreal(error) => HostError::Db(error),
        OrganizationError::MissingUserCredentials => HostError::InvalidPayload(
            "email, and a username of at least 3 characters when given, are required".into(),
        ),
        other => HostError::Message(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::dummy_ctx;
    use super::*;

    #[tokio::test]
    async fn create_member_refuses_a_password() {
        let ctx = dummy_ctx(&["org::create_member"]);
        let error = create_member(
            &ctx,
            &json!({ "email": "ada@acme.io", "password": "correct horse" }),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, HostError::InvalidPayload(_)), "{error}");
    }

    #[tokio::test]
    async fn invite_refuses_a_password() {
        let ctx = dummy_ctx(&["org::invite"]);
        let error = invite(
            &ctx,
            &json!({ "email": "ada@acme.io", "new_password": "correct horse" }),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, HostError::InvalidPayload(_)), "{error}");
    }
}
