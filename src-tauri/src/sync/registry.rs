//! The set of tables that travel between devices.
//!
//! Anything derived from the vault (the whole index database), device-local
//! runtime state (logs, checkpoints, page cache) and secrets are deliberately
//! left out. See `docs/SYNC_DESIGN.md` §1.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TableKind {
    /// One row per entity, primary key columns are listed in `pk`.
    Plain,
    /// Same as `Plain`, but `vault_path` is a device-local absolute path. It is
    /// normalised to [`VAULT_PLACEHOLDER`] before it leaves the machine and is
    /// rewritten to the local path when a remote row is applied.
    VaultScoped,
    /// The `settings` key/value table. Device paths and credentials stay local.
    Settings,
}

pub struct TableSpec {
    pub table: &'static str,
    pub pk: &'static [&'static str],
    pub kind: TableKind,
}

pub const VAULT_PLACEHOLDER: &str = "__VAULT__";

pub const SYNCED_TABLES: &[TableSpec] = &[
    TableSpec { table: "annotations", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "settings", pk: &["key"], kind: TableKind::Settings },
    TableSpec { table: "facts", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "note_fact_mentions", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "fact_sources", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "fact_evidence", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "fact_checks", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "fact_relations", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "fact_merges", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "fact_splits", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "fact_conflict_resolutions", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "fact_reviews", pk: &["fact_id"], kind: TableKind::Plain },
    TableSpec { table: "fact_meta", pk: &["key"], kind: TableKind::Plain },
    TableSpec { table: "history_events", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "sources", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "source_passages", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "citations", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "history_meta", pk: &["key"], kind: TableKind::Plain },
    TableSpec { table: "history_search_candidates", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "production_jobs", pk: &["id"], kind: TableKind::VaultScoped },
    TableSpec { table: "production_prompts", pk: &["id"], kind: TableKind::Plain },
    TableSpec { table: "production_prompt_versions", pk: &["prompt_id", "version"], kind: TableKind::Plain },
    TableSpec { table: "production_model_prices", pk: &["provider", "model"], kind: TableKind::Plain },
    TableSpec { table: "production_subscriptions", pk: &["id"], kind: TableKind::VaultScoped },
    TableSpec {
        table: "production_seen_videos",
        pk: &["vault_path", "mid", "bvid"],
        kind: TableKind::VaultScoped,
    },
];

/// Settings that describe this machine, not the user's content, stay local.
/// Credentials never leave the device either; a synced API key would turn the
/// sync server into a secret store we did not design for.
pub fn syncable_setting(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    if key == "vault_path" || key == "index_root" {
        return false;
    }
    const SECRET_MARKERS: &[&str] =
        &["api_key", "apikey", "cookie", "account", "token", "secret", "password", "credential"];
    !SECRET_MARKERS.iter().any(|marker| key.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_paths_and_secrets_are_not_synced() {
        assert!(!syncable_setting("vault_path"));
        assert!(!syncable_setting("index_root"));
        assert!(!syncable_setting("production.ai.api_key.deepseek"));
        assert!(!syncable_setting("production.bili.cookies"));
        assert!(syncable_setting("production.ai.provider"));
        assert!(syncable_setting("production.retries"));
        assert!(syncable_setting("production.default_fields"));
    }

    #[test]
    fn registry_has_unique_tables_and_pk_columns() {
        let mut names = std::collections::HashSet::new();
        for spec in SYNCED_TABLES {
            assert!(names.insert(spec.table), "duplicate table {}", spec.table);
            assert!(!spec.pk.is_empty(), "{} has no pk", spec.table);
        }
    }
}
