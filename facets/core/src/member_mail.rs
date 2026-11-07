//! Mail to members. The bodies and queueing are in `aether_kernel::member_mail`.

pub use aether_kernel::member_mail::*;

use crate::messaging::{self, Message};
use crate::scheduler::queue::{self, NewJob};
use crate::state::AppState;

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
    public_base(&state.config)
}
