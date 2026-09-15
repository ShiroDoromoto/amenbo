//! **A secret one of Amenbo's own features holds**, at one layer (`AMB-D-884`).
//!
//! The body had no place for a credential until this. `config.json` says of itself that it holds none,
//! and the three features that needed one — mail, Slack, the Viewer — were plugins then, keeping theirs in
//! a table of the mechanism's. That table went with it; this one took over what it was for:
//! the connection a notification target sends through (`AMB-D-885`), and the token and key the Viewer's
//! server is reached and sealed with (`AMB-D-886`).
//!
//! **The table is the exclusion.** `secret` is named in [`crate::export::WITHHELD_ON_THE_WAY_OUT`], so no
//! road out of the store walks it — most sharply the sync snapshot, which would otherwise carry the
//! Viewer's `encryption_key` to the very server that key seals. A row-by-row "is this field secret" test
//! would put the judgement on every path that reads a setting, including the ones written later by someone
//! who did not know to ask. `backup`/`restore` carry these rows — a copy of the whole file — because that
//! road leads back to the same person's machine, and dropping them there would mean typing every
//! credential in again.
//!
//! The address is `(project_id, area, owner_id, field_key)`: the layer, the feature, the row inside that
//! feature which holds the secret (or `None` where the feature itself does), and the field.

use crate::error::Result;
use crate::model::{Secret, SecretArea};
use crate::ops::{emit_create, emit_update};
use crate::store_engine::{read, record, WriteTx};
use crate::time::Timestamp;

/// Set (`Some`) or clear (`None`) one secret field's value at one layer, inside the caller's transaction.
/// Idempotent upsert on the `(project_id, area, owner_id, field_key)` address. Returns whether anything
/// changed.
pub fn set(
    tx: &WriteTx<'_>,
    project_id: Option<i64>,
    area: SecretArea,
    owner_id: Option<i64>,
    field_key: &str,
    value: Option<&str>,
) -> Result<bool> {
    let existing_id = read::secret_row_id(tx.conn(), project_id, area, owner_id, field_key)?;
    match (existing_id, value) {
        (Some(id), Some(v)) => {
            let before = read::secret_row_by_id(tx.conn(), id)?
                .expect("the row id was just read from the same transaction");
            if before.value == v {
                return Ok(false);
            }
            let after = Secret { value: v.to_string(), updated_at: Timestamp::now(), ..before.clone() };
            emit_update(tx, record::secret(&before), record::secret(&after))?;
            Ok(true)
        }
        (Some(id), None) => {
            tx.delete_record("secret", id)?;
            Ok(true)
        }
        (None, Some(v)) => {
            let now = Timestamp::now();
            let row = Secret {
                id: read::next_id(tx.conn(), "secret")?,
                project_id,
                area,
                owner_id,
                field_key: field_key.to_string(),
                value: v.to_string(),
                created_at: now,
                updated_at: now,
            };
            emit_create(tx, record::secret(&row))?;
            Ok(true)
        }
        (None, None) => Ok(false),
    }
}

/// Delete **every** secret one owner holds, at every layer, inside the caller's transaction — what the
/// delete of the row a secret hangs off must run before taking that row, and what clears a feature that
/// holds its own (`owner_id` `None`, the Viewer's keys). Returns how many rows went.
///
/// `owner_id` is polymorphic, so no `CASCADE` can reach these rows and no constraint would stop a delete
/// that forgot them (`AMB-D-403` puts that sweep in the op, where a reviewer can read it). It crosses
/// layers for the reason its plugin twin crosses projects: what goes is one owner's residue.
pub fn forget_owner(tx: &WriteTx<'_>, area: SecretArea, owner_id: Option<i64>) -> Result<usize> {
    let ids = read::secret_row_ids(tx.conn(), area, owner_id)?;
    for id in &ids {
        tx.delete_record("secret", *id)?;
    }
    Ok(ids.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::test_support::{mk_project, with_tx};
    use crate::store_engine::read;

    #[test]
    fn set_then_read_back_the_secret() {
        with_tx(|tx| {
            set(tx, None, SecretArea::Viewer, None, "encryption_key", Some("k3y")).unwrap();
            assert_eq!(
                read::secret_value(tx.conn(), None, SecretArea::Viewer, None, "encryption_key")
                    .unwrap()
                    .as_deref(),
                Some("k3y"),
            );
        });
    }

    /// The address is four parts, and every one of them separates: two features, two owners under one
    /// feature, and the same field key under each reads back its own value.
    #[test]
    fn the_area_and_the_owner_each_separate_one_secret_from_another() {
        with_tx(|tx| {
            set(tx, None, SecretArea::Viewer, None, "token", Some("viewer")).unwrap();
            set(tx, None, SecretArea::Notify, Some(1), "token", Some("first")).unwrap();
            set(tx, None, SecretArea::Notify, Some(2), "token", Some("second")).unwrap();

            let read_one = |area, owner| {
                read::secret_value(tx.conn(), None, area, owner, "token").unwrap()
            };
            assert_eq!(read_one(SecretArea::Viewer, None).as_deref(), Some("viewer"));
            assert_eq!(read_one(SecretArea::Notify, Some(1)).as_deref(), Some("first"));
            assert_eq!(read_one(SecretArea::Notify, Some(2)).as_deref(), Some("second"));
            assert_eq!(
                read_one(SecretArea::Notify, None),
                None,
                "the feature's own row is not any of its owners' — an absent owner seeks NULL, not any id",
            );
        });
    }

    /// The layer separates too, and the device row is not a project's: deleting the project takes its own
    /// and leaves the device's, the cascade reaching only the reference it holds.
    #[test]
    fn a_device_secret_stands_apart_from_a_projects_and_survives_it() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, None, SecretArea::Notify, Some(1), "webhook_url", Some("device")).unwrap();
            set(tx, Some(p), SecretArea::Notify, Some(1), "webhook_url", Some("project")).unwrap();

            crate::ops::project::delete(tx, p).unwrap();

            assert_eq!(
                read::secret_value(tx.conn(), Some(p), SecretArea::Notify, Some(1), "webhook_url")
                    .unwrap(),
                None,
            );
            assert_eq!(
                read::secret_value(tx.conn(), None, SecretArea::Notify, Some(1), "webhook_url")
                    .unwrap()
                    .as_deref(),
                Some("device"),
            );
        });
    }

    #[test]
    fn clearing_deletes_the_row_and_leaves_the_field_unset() {
        with_tx(|tx| {
            set(tx, None, SecretArea::Notify, Some(1), "smtp_password", Some("pw")).unwrap();
            assert!(set(tx, None, SecretArea::Notify, Some(1), "smtp_password", None).unwrap());
            assert_eq!(
                read::secret_value(tx.conn(), None, SecretArea::Notify, Some(1), "smtp_password").unwrap(),
                None,
            );
            assert!(
                !set(tx, None, SecretArea::Notify, Some(1), "smtp_password", None).unwrap(),
                "clearing an absent secret is a no-op",
            );
        });
    }

    #[test]
    fn upsert_keeps_one_row_per_address() {
        with_tx(|tx| {
            set(tx, None, SecretArea::Notify, Some(1), "webhook_url", Some("a")).unwrap();
            set(tx, None, SecretArea::Notify, Some(1), "webhook_url", Some("b")).unwrap();
            let n: i64 = tx
                .conn()
                .query_row(
                    "SELECT count(*) FROM secret WHERE area='notify' AND owner_id=1 AND field_key='webhook_url'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "the address is unique — an update reuses the row, never appends");
        });
    }

    /// What a target's delete runs: every layer's secrets for that one owner, and nobody else's.
    #[test]
    fn forgetting_an_owner_erases_every_layer_and_only_that_owner() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, None, SecretArea::Notify, Some(1), "webhook_url", Some("device")).unwrap();
            set(tx, Some(p), SecretArea::Notify, Some(1), "webhook_url", Some("project")).unwrap();
            set(tx, None, SecretArea::Notify, Some(2), "webhook_url", Some("kept")).unwrap();
            set(tx, None, SecretArea::Viewer, None, "auth_token", Some("kept too")).unwrap();

            assert_eq!(forget_owner(tx, SecretArea::Notify, Some(1)).unwrap(), 2);
            assert_eq!(
                read::secret_value(tx.conn(), None, SecretArea::Notify, Some(1), "webhook_url").unwrap(),
                None,
            );
            assert_eq!(
                read::secret_value(tx.conn(), Some(p), SecretArea::Notify, Some(1), "webhook_url").unwrap(),
                None,
            );
            assert_eq!(
                read::secret_value(tx.conn(), None, SecretArea::Notify, Some(2), "webhook_url")
                    .unwrap()
                    .as_deref(),
                Some("kept"),
            );
            assert_eq!(
                read::secret_value(tx.conn(), None, SecretArea::Viewer, None, "auth_token")
                    .unwrap()
                    .as_deref(),
                Some("kept too"),
            );
        });
    }

    /// The same sweep from the other side: a feature that holds its own secrets is cleared by naming no
    /// owner, and that takes none of the rows hanging off its owners.
    #[test]
    fn forgetting_a_feature_that_owns_its_secrets_leaves_its_owners_rows() {
        with_tx(|tx| {
            set(tx, None, SecretArea::Viewer, None, "auth_token", Some("t")).unwrap();
            set(tx, None, SecretArea::Viewer, None, "encryption_key", Some("k")).unwrap();
            set(tx, None, SecretArea::Notify, Some(1), "webhook_url", Some("kept")).unwrap();

            assert_eq!(forget_owner(tx, SecretArea::Viewer, None).unwrap(), 2);
            assert_eq!(
                read::secret_value(tx.conn(), None, SecretArea::Viewer, None, "auth_token").unwrap(),
                None,
            );
            assert_eq!(
                read::secret_value(tx.conn(), None, SecretArea::Notify, Some(1), "webhook_url")
                    .unwrap()
                    .as_deref(),
                Some("kept"),
            );
        });
    }

    #[test]
    fn deleting_the_project_cascades_its_secrets() {
        with_tx(|tx| {
            let p = mk_project(tx, "proj");
            set(tx, Some(p), SecretArea::Notify, Some(1), "webhook_url", Some("s3cret")).unwrap();
            crate::ops::project::delete(tx, p).unwrap();
            let n: i64 = tx
                .conn()
                .query_row("SELECT count(*) FROM secret WHERE project_id=?1", [p], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "the secret cascaded with the project (ON DELETE CASCADE)");
        });
    }
}
