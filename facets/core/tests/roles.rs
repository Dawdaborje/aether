//! Roles against a real SurrealDB (see `host_db_audit.rs` for how to run).

use aether_core::roles::{self, RoleError};
use serde_json::json;
use surrealdb::{
    Surreal,
    engine::remote::ws::{Client, Ws},
    opt::auth::Root,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const NAMESPACE: &str = "aether_roles_test";

async fn world(label: &str) -> Result<Option<Surreal<Client>>, Box<dyn std::error::Error>> {
    let Ok(address) = std::env::var("AETHER_TEST_DB") else {
        eprintln!("AETHER_TEST_DB is not set; skipping");
        return Ok(None);
    };
    let db = Surreal::<Client>::init();
    db.connect::<Ws>(address).await?;
    db.signin(Root {
        username: "root".into(),
        password: "root".into(),
    })
    .await?;
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .subsec_nanos();
    let org = format!("org_{label}_{suffix}");
    aether_orm::migrate_org(&db, NAMESPACE, &org).await?;
    db.use_ns(NAMESPACE).use_db(&org).await?;
    // Two members of the organization.
    for (key, name) in [("users:ann", "ann"), ("users:bob", "bob")] {
        db.query(
            "CREATE org_users SET core_user_id = $id, display_name = $name, is_active = true;",
        )
        .bind(("id", key.to_string()))
        .bind(("name", name.to_string()))
        .await?
        .check()?;
    }
    Ok(Some(db))
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn a_plugins_roles_are_created_given_listed_and_taken_away() -> TestResult {
    let Some(db) = world("roles").await? else {
        return Ok(());
    };
    let declared = vec![
        json!({ "name": "hr_manager", "label": "HR manager", "description": "Hires and ends employment" }),
        json!({ "name": "viewer" }),
    ];
    roles::sync_roles(&db, "hr", Some(&declared)).await?;
    // Declaring again changes nothing and does not duplicate.
    roles::sync_roles(&db, "hr", Some(&declared)).await?;
    // `list` ensures kernel system roles exist alongside plugin roles.
    let listed_names: Vec<_> = roles::list(&db)
        .await?
        .into_iter()
        .map(|r| r.name)
        .collect();
    assert!(listed_names.contains(&"hr.hr_manager".into()));
    assert!(listed_names.contains(&"hr.viewer".into()));
    assert!(listed_names.contains(&roles::ORG_ADMIN.into()));
    assert!(listed_names.contains(&roles::USER_MANAGER.into()));

    roles::grant_by_id(&db, "users:ann", "hr.hr_manager").await?;
    roles::grant_by_id(&db, "users:ann", "hr.hr_manager").await?; // harmless twice
    roles::grant_by_id(&db, "users:bob", "hr.viewer").await?;
    assert_eq!(roles::roles_of(&db, "users:ann").await?, ["hr.hr_manager"]);
    assert_eq!(roles::roles_of(&db, "users:bob").await?, ["hr.viewer"]);
    assert!(roles::roles_of(&db, "users:nobody").await?.is_empty());

    let listed = roles::list(&db).await?;
    let manager = listed
        .iter()
        .find(|r| r.name == "hr.hr_manager")
        .ok_or("role missing")?;
    assert_eq!(manager.label, "HR manager");
    assert!(manager.is_system);
    assert_eq!(
        manager.holders,
        [roles::RoleHolder {
            user_id: "users:ann".into(),
            display_name: "ann".into(),
        }]
    );

    let custom = roles::create_custom(&db, "shift_lead", "Shift lead", Some("Floor leads")).await?;
    assert_eq!(custom.name, "custom.shift_lead");
    assert!(!custom.is_system);
    roles::grant_by_id(&db, "users:bob", "custom.shift_lead").await?;
    roles::update_custom(
        &db,
        "custom.shift_lead",
        Some("Shift Lead"),
        Some(Some("Leads")),
    )
    .await?;
    let again = roles::list(&db).await?;
    let custom_row = again
        .iter()
        .find(|r| r.name == "custom.shift_lead")
        .ok_or("custom missing")?;
    assert_eq!(custom_row.label, "Shift Lead");
    assert_eq!(custom_row.holders.len(), 1);
    roles::delete_custom(&db, "custom.shift_lead").await?;
    assert!(
        roles::list(&db)
            .await?
            .iter()
            .all(|r| r.name != "custom.shift_lead")
    );
    assert!(
        roles::roles_of(&db, "users:bob")
            .await?
            .iter()
            .all(|r| r != "custom.shift_lead")
    );

    // The administrator role is made on first use.
    roles::grant_by_id(&db, "users:ann", roles::ORG_ADMIN).await?;
    assert_eq!(
        roles::roles_of(&db, "users:ann").await?,
        ["hr.hr_manager", "org_admin"]
    );

    roles::revoke_by_id(&db, "users:ann", "hr.hr_manager").await?;
    roles::revoke_by_id(&db, "users:ann", "hr.hr_manager").await?; // harmless twice
    assert_eq!(roles::roles_of(&db, "users:ann").await?, ["org_admin"]);
    assert_eq!(
        roles::roles_of(&db, "users:bob").await?,
        ["hr.viewer"],
        "others keep theirs"
    );

    roles::ensure_system_roles(&db).await?;
    roles::grant_by_id(&db, "users:bob", roles::USER_MANAGER).await?;
    assert_eq!(
        roles::roles_of(&db, "users:bob").await?,
        ["hr.viewer", "user_manager"]
    );
    Ok(())
}

#[tokio::test]
#[ignore = "needs SurrealDB (AETHER_TEST_DB)"]
async fn unknown_roles_and_non_members_are_refused_with_a_useful_message() -> TestResult {
    let Some(db) = world("refused").await? else {
        return Ok(());
    };
    roles::sync_roles(&db, "hr", Some(&[json!({ "name": "hr_manager" })])).await?;

    let unknown = roles::grant_by_id(&db, "users:ann", "hr.hr_mangler").await;
    match unknown {
        Err(RoleError::UnknownRole { known, .. }) => {
            assert!(known.contains("hr.hr_manager"), "{known}")
        }
        other => return Err(format!("{other:?}").into()),
    }
    let stranger = roles::grant_by_id(&db, "users:stranger", "hr.hr_manager").await;
    assert!(
        matches!(stranger, Err(RoleError::NotAMember(_, _))),
        "{stranger:?}"
    );
    Ok(())
}
