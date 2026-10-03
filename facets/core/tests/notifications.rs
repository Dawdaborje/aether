//! Stored notifications against a real SurrealDB (see `host_db_audit.rs` for how to run).

use std::collections::HashSet;

use aether_core::access::audit::{Actor, AuditContext};
use aether_core::kernel::{CallInfo, DbScope, HostError, PluginHostContext, kernel_command};
use aether_core::notifications::{
    Audience, HubMessage, Level, NewNotification, NotificationHub, Viewer, send, store,
};
use serde_json::json;
use surrealdb::{Surreal, engine::remote::ws::{Client, Ws}, opt::auth::Root};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NAMESPACE: &str = "aether_notifications_test";

async fn organization(label: &str) -> Result<Option<(Surreal<Client>, String)>, Box<dyn std::error::Error>> {
    let Ok(address) = std::env::var("AETHER_TEST_DB") else {
        eprintln!("AETHER_TEST_DB is not set; skipping");
        return Ok(None);
    };
    let db = Surreal::<Client>::init();
    db.connect::<Ws>(address).await?;
    db.signin(Root { username: "root".into(), password: "root".into() }).await?;
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .subsec_nanos();
    let name = format!("org_{label}_{suffix}");
    aether_orm::migrate_org(&db, NAMESPACE, &name).await?;
    let session = db.clone();
    session.use_ns(NAMESPACE).use_db(&name).await?;
    Ok(Some((session, name)))
}

fn note(title: &str, audience: Audience) -> NewNotification {
    let mut new = NewNotification::kernel(Level::Info, title, Some("body".into()));
    new.audience = audience;
    new
}

fn member(id: &str) -> Viewer {
    Viewer { actor: Some(id.to_string()), member: true }
}

fn visitor(id: &str) -> Viewer {
    Viewer { actor: Some(id.to_string()), member: false }
}

const NOBODY: Viewer = Viewer { actor: None, member: false };

fn titles(list: &[aether_core::notifications::Notification]) -> HashSet<&str> {
    list.iter().map(|item| item.title.as_str()).collect()
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn each_viewer_sees_only_what_is_for_them() -> TestResult {
    let Some((db, org)) = organization("visibility").await? else { return Ok(()) };
    let hub = NotificationHub::default();
    for new in [
        note("for members", Audience::Members),
        note("for everyone", Audience::Everyone),
        note("for v1", Audience::Actors { actors: vec!["visitors:v1".into()] }),
        note("for u1", Audience::Actors { actors: vec!["users:u1".into()] }),
    ] {
        send(&db, &hub, &org, new).await?;
    }
    let mut expired = note("expired", Audience::Everyone);
    expired.expires_in_secs = Some(0);
    send(&db, &hub, &org, expired).await?;

    let seen = |viewer: Viewer| {
        let db = db.clone();
        async move { store::list(&db, &viewer, false, 50).await }
    };
    let user = seen(member("users:u1")).await?;
    assert_eq!(titles(&user), HashSet::from(["for members", "for everyone", "for u1"]));
    let other_user = seen(member("users:u2")).await?;
    assert_eq!(titles(&other_user), HashSet::from(["for members", "for everyone"]));
    let first_visitor = seen(visitor("visitors:v1")).await?;
    assert_eq!(titles(&first_visitor), HashSet::from(["for everyone", "for v1"]));
    let second_visitor = seen(visitor("visitors:v2")).await?;
    assert_eq!(titles(&second_visitor), HashSet::from(["for everyone"]));
    let stranger = seen(NOBODY).await?;
    assert_eq!(titles(&stranger), HashSet::from(["for everyone"]));
    // Newest first.
    assert!(user.windows(2).all(|pair| pair[0].id > pair[1].id));
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn reading_is_tracked_per_actor_and_only_for_what_they_can_see() -> TestResult {
    let Some((db, org)) = organization("reads").await? else { return Ok(()) };
    let hub = NotificationHub::default();
    let one = send(&db, &hub, &org, note("one", Audience::Members)).await?;
    let two = send(&db, &hub, &org, note("two", Audience::Members)).await?;
    let secret = send(&db, &hub, &org, note("secret", Audience::Actors { actors: vec!["users:u9".into()] })).await?;

    let ann = member("users:ann");
    assert_eq!(store::unread_count(&db, &ann).await?, 2);

    let marked = store::mark_read(&db, &ann, "users:ann", Some(vec![one.id.clone(), secret.id.clone()])).await?;
    assert_eq!(marked, 1, "a notification for someone else cannot be marked");
    assert_eq!(store::unread_count(&db, &ann).await?, 1);
    let unread = store::list(&db, &ann, true, 50).await?;
    assert_eq!(titles(&unread), HashSet::from(["two"]));
    let all = store::list(&db, &ann, false, 50).await?;
    assert!(all.iter().any(|item| item.id == one.id && item.read));
    assert!(all.iter().any(|item| item.id == two.id && !item.read));

    // Reading twice changes nothing; someone else is unaffected.
    assert_eq!(store::mark_read(&db, &ann, "users:ann", Some(vec![one.id.clone()])).await?, 0);
    assert_eq!(store::unread_count(&db, &member("users:bob")).await?, 2);

    assert_eq!(store::mark_read(&db, &ann, "users:ann", None).await?, 1);
    assert_eq!(store::unread_count(&db, &ann).await?, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_reconnecting_browser_gets_what_it_missed_in_order() -> TestResult {
    let Some((db, org)) = organization("replay").await? else { return Ok(()) };
    let hub = NotificationHub::default();
    let first = send(&db, &hub, &org, note("first", Audience::Members)).await?;
    send(&db, &hub, &org, note("second", Audience::Members)).await?;
    send(&db, &hub, &org, note("hidden", Audience::Actors { actors: vec!["users:x".into()] })).await?;
    send(&db, &hub, &org, note("third", Audience::Everyone)).await?;

    let missed = store::after(&db, &member("users:ann"), &first.id, 100).await?;
    let order: Vec<&str> = missed.iter().map(|item| item.title.as_str()).collect();
    assert_eq!(order, ["second", "third"]);
    let limited = store::after(&db, &member("users:ann"), &first.id, 1).await?;
    assert_eq!(limited.len(), 1);
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn sending_wakes_connected_browsers_and_rejects_bad_input() -> TestResult {
    let Some((db, org)) = organization("hub").await? else { return Ok(()) };
    let hub = NotificationHub::default();
    let mut receiver = hub.subscribe(&org);
    let mut elsewhere = hub.subscribe("another_org");

    let stored = send(&db, &hub, &org, note("hello", Audience::Members)).await?;
    let HubMessage::Notification(woken) = receiver.recv().await? else {
        panic!("expected a notification");
    };
    assert_eq!(woken.id, stored.id);
    assert!(woken.audience.includes(&member("users:a")));
    assert!(!woken.audience.includes(&visitor("visitors:a")));
    assert!(elsewhere.try_recv().is_err());

    let mut off_site = note("x", Audience::Members);
    off_site.link = Some("https://evil.example".into());
    assert!(send(&db, &hub, &org, off_site).await.is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn purging_removes_expired_and_old_notifications_and_their_reads() -> TestResult {
    let Some((db, org)) = organization("purge").await? else { return Ok(()) };
    let hub = NotificationHub::default();
    let mut expired = note("expired", Audience::Members);
    expired.expires_in_secs = Some(0);
    send(&db, &hub, &org, expired).await?;
    let kept = send(&db, &hub, &org, note("kept", Audience::Members)).await?;
    store::mark_read(&db, &member("users:a"), "users:a", Some(vec![kept.id.clone()])).await?;

    assert_eq!(store::purge(&db, None).await?, 1);
    assert_eq!(store::list(&db, &member("users:a"), false, 10).await?.len(), 1);

    // Retention of zero days removes everything, and the read mark with it.
    assert_eq!(store::purge(&db, Some(0)).await?, 1);
    assert!(store::list(&db, &member("users:a"), false, 10).await?.is_empty());
    let mut response = db.query("SELECT * FROM notification_reads;").await?.check()?;
    let left: Vec<serde_json::Value> = response.take(0)?;
    assert!(left.is_empty(), "left over: {left:?}");
    Ok(())
}

fn host_context(db: &Surreal<Client>, org: &str, hub: &NotificationHub, actor: Actor, caps: &[&str]) -> PluginHostContext {
    PluginHostContext::new(
        "chat",
        caps.iter().map(|cap| cap.to_string()).collect(),
        Default::default(),
        std::sync::Arc::new(db.clone()),
        DbScope::new(NAMESPACE, org),
        hub.clone(),
        CallInfo::new(
            AuditContext { actor, request_id: "req".into(), ip: None, user_agent: None },
            "post_message",
        ),
    )
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn plugins_notify_through_the_kernel_with_the_right_capabilities() -> TestResult {
    let Some((db, org)) = organization("plugin").await? else { return Ok(()) };
    let hub = NotificationHub::default();
    let caller = Actor::Visitor("visitors:guest".into());

    // Without the capability: refused.
    let denied = host_context(&db, &org, &hub, caller.clone(), &[]);
    let result = kernel_command(&denied, "notify::send", json!({ "title": "x" })).await;
    assert!(matches!(result, Err(HostError::Capability(_))));

    let ctx = host_context(&db, &org, &hub, caller.clone(), &["notify::send"]);
    // Back to the caller: the visitor sees it, members and strangers do not.
    kernel_command(&ctx, "notify::send", json!({ "title": "welcome", "audience": "caller", "link": "/chat" })).await?;
    let own = store::list(&db, &visitor("visitors:guest"), false, 10).await?;
    assert_eq!(own.len(), 1);
    assert_eq!(own[0].source, "chat");
    assert!(store::list(&db, &member("users:ann"), false, 10).await?.is_empty());

    // `everyone` needs notify::public as well.
    let refused = kernel_command(&ctx, "notify::send", json!({ "title": "all", "audience": "everyone" })).await;
    assert!(matches!(refused, Err(HostError::Capability(_))));
    let public = host_context(&db, &org, &hub, caller, &["notify::send", "notify::public"]);
    kernel_command(&public, "notify::send", json!({ "title": "all", "audience": "everyone" })).await?;
    assert_eq!(store::list(&db, &NOBODY, false, 10).await?.len(), 1);

    // Specific actors, and bad input.
    kernel_command(&ctx, "notify::send", json!({ "title": "dm", "audience": { "actors": ["users:ann"] } })).await?;
    assert_eq!(store::list(&db, &member("users:ann"), false, 10).await?.len(), 2);
    let bad = kernel_command(&ctx, "notify::send", json!({ "title": "x", "audience": { "actors": ["org:acme"] } })).await;
    assert!(matches!(bad, Err(HostError::InvalidPayload(_))));
    Ok(())
}
