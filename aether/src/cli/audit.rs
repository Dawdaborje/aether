//! `--audit-verify` and `--audit-export-anchor`: check the audit chains of an organization and keep
//! proof of them elsewhere. See `docs/architecture/audit_chain.md`.

use std::path::Path;

use aether_core::access::chain::{Anchor, Chain, export_anchor, verify as verify_chain};
use surrealdb::{Surreal, engine::remote::ws::Client};

/// Verify every chain of the organization the handle is on. Prints one block per chain and returns
/// whether all of them are sound.
pub async fn verify_all(
    db: &Surreal<Client>,
    anchor: Option<&Anchor>,
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut sound = true;
    for chain in Chain::ALL {
        let report = verify_chain(db, chain, anchor).await?;
        let state = if report.ok { "ok" } else { "BROKEN" };
        println!(
            "{:<13} {state}: {} row(s) checked, {} unsealed, {} checkpoint(s) checked",
            report.chain, report.rows_checked, report.unsealed, report.checkpoints_checked
        );
        for failure in &report.failures {
            let at = failure.seq.map(|seq| format!(" at row {seq}")).unwrap_or_default();
            println!("  - {}{at}: {}", failure.rule, failure.detail);
        }
        if report.more_failures > 0 {
            println!("  - and {} more", report.more_failures);
        }
        sound &= report.ok;
    }
    Ok(sound)
}

/// `--audit-verify`, with an anchor file if one was named.
pub async fn verify(db: &Surreal<Client>, anchor: Option<&Path>) -> Result<bool, Box<dyn std::error::Error>> {
    let anchor = match anchor {
        Some(path) => Some(
            serde_json::from_str::<Anchor>(&std::fs::read_to_string(path)?)
                .map_err(|e| format!("{} is not an anchor file: {e}", path.display()))?,
        ),
        None => None,
    };
    verify_all(db, anchor.as_ref()).await
}

/// `--audit-export-anchor DIR`: one `<chain>.anchor.json` per chain that has a checkpoint.
pub async fn export_anchors(db: &Surreal<Client>, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(dir)?;
    let mut wrote = 0;
    for chain in Chain::ALL {
        let Some(anchor) = export_anchor(db, chain).await? else {
            println!("{:<13} no checkpoint yet", chain.name());
            continue;
        };
        let path = dir.join(format!("{}.anchor.json", chain.name()));
        std::fs::write(&path, serde_json::to_string_pretty(&anchor)?)?;
        println!("{:<13} row {} -> {}", chain.name(), anchor.to_seq, path.display());
        wrote += 1;
    }
    if wrote == 0 {
        println!("Nothing to anchor yet: the sealer makes the first checkpoints soon after rows are written.");
    }
    Ok(())
}
