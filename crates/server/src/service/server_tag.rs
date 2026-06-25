use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
    TransactionTrait,
};

use crate::entity::server_tag;
use crate::error::AppError;

pub const MAX_TAGS: usize = 8;
pub const MAX_TAG_LEN: usize = 16;

pub fn validate_tags(raw: &[String]) -> Result<Vec<String>, AppError> {
    if raw.len() > MAX_TAGS {
        return Err(AppError::Validation(format!("at most {MAX_TAGS} tags")));
    }
    let mut seen = std::collections::BTreeSet::new();
    for tag in raw {
        let trimmed = tag.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.chars().count() > MAX_TAG_LEN {
            return Err(AppError::Validation(format!(
                "tag '{trimmed}' exceeds {MAX_TAG_LEN} chars"
            )));
        }
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        {
            return Err(AppError::Validation(format!(
                "tag '{trimmed}' contains invalid characters"
            )));
        }
        seen.insert(trimmed);
    }
    Ok(seen.into_iter().collect())
}

pub async fn list_tags(db: &DatabaseConnection, server_id: &str) -> Result<Vec<String>, AppError> {
    let rows = server_tag::Entity::find()
        .filter(server_tag::Column::ServerId.eq(server_id))
        .order_by_asc(server_tag::Column::Tag)
        .all(db)
        .await?;
    Ok(rows.into_iter().map(|r| r.tag).collect())
}

pub async fn set_tags(
    db: &DatabaseConnection,
    server_id: &str,
    tags: Vec<String>,
) -> Result<Vec<String>, AppError> {
    let normalized = validate_tags(&tags)?;
    let txn = db.begin().await?;
    server_tag::Entity::delete_many()
        .filter(server_tag::Column::ServerId.eq(server_id))
        .exec(&txn)
        .await?;
    for tag in &normalized {
        server_tag::ActiveModel {
            server_id: Set(server_id.to_string()),
            tag: Set(tag.clone()),
        }
        .insert(&txn)
        .await?;
    }
    txn.commit().await?;
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::server;
    use crate::test_utils::setup_test_db;
    use chrono::{TimeZone, Utc};
    use sea_orm::ConnectionTrait;

    #[test]
    fn validate_rejects_too_many() {
        let tags: Vec<String> = (0..9).map(|i| format!("t{i}")).collect();
        assert!(validate_tags(&tags).is_err());
    }

    #[test]
    fn validate_rejects_too_long() {
        let tags = vec!["a".repeat(17)];
        assert!(validate_tags(&tags).is_err());
    }

    #[test]
    fn validate_rejects_invalid_chars() {
        assert!(validate_tags(&["bad space".into()]).is_err());
        assert!(validate_tags(&["bad/slash".into()]).is_err());
    }

    #[test]
    fn validate_trims_and_dedupes_and_sorts() {
        let got = validate_tags(&["  b ".into(), "a".into(), "b".into()]).unwrap();
        assert_eq!(got, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn validate_skips_empty_after_trim() {
        let got = validate_tags(&["  ".into(), "a".into()]).unwrap();
        assert_eq!(got, vec!["a".to_string()]);
    }

    #[test]
    fn validate_allows_underscore_dash_dot() {
        assert!(
            validate_tags(&["db_primary".into(), "db-secondary".into(), "v1.0".into()]).is_ok()
        );
    }

    // Empty input yields an empty normalized vec, not an error.
    #[test]
    fn validate_accepts_empty_input() {
        let got = validate_tags(&[]).unwrap();
        assert!(got.is_empty());
    }

    // An input made entirely of blank/whitespace tags is dropped to an empty result.
    #[test]
    fn validate_all_blank_yields_empty() {
        let got = validate_tags(&["".into(), "   ".into(), "\t".into()]).unwrap();
        assert!(got.is_empty());
    }

    // Exactly MAX_TAGS distinct tags is allowed (boundary on count is inclusive).
    #[test]
    fn validate_accepts_exactly_max_tags() {
        let tags: Vec<String> = (0..MAX_TAGS).map(|i| format!("t{i}")).collect();
        let got = validate_tags(&tags).unwrap();
        assert_eq!(got.len(), MAX_TAGS);
    }

    // A tag of exactly MAX_TAG_LEN characters is allowed (length boundary is inclusive).
    #[test]
    fn validate_accepts_exactly_max_len() {
        let tag = "a".repeat(MAX_TAG_LEN);
        let got = validate_tags(&[tag.clone()]).unwrap();
        assert_eq!(got, vec![tag]);
    }

    // The length check counts unicode scalars, not bytes: a multibyte tag within
    // MAX_TAG_LEN chars is still rejected because non-ASCII fails the charset check.
    #[test]
    fn validate_rejects_non_ascii_alphanumeric() {
        let err = validate_tags(&["café".into()]).err().expect("non-ascii must fail");
        assert!(matches!(err, AppError::Validation(_)));
    }

    // The too-many error message includes the configured maximum.
    #[test]
    fn validate_too_many_error_message() {
        let tags: Vec<String> = (0..(MAX_TAGS + 1)).map(|i| format!("t{i}")).collect();
        let err = validate_tags(&tags).err().expect("too many must fail");
        match err {
            AppError::Validation(msg) => assert!(msg.contains(&MAX_TAGS.to_string())),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    // The too-long error message names the offending tag and the limit.
    #[test]
    fn validate_too_long_error_message() {
        let long = "x".repeat(MAX_TAG_LEN + 1);
        let err = validate_tags(&[long.clone()]).err().expect("too long must fail");
        match err {
            AppError::Validation(msg) => {
                assert!(msg.contains(&long));
                assert!(msg.contains(&MAX_TAG_LEN.to_string()));
            }
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    // The invalid-char error message names the offending tag.
    #[test]
    fn validate_invalid_char_error_message() {
        let err = validate_tags(&["bad space".into()]).err().expect("invalid must fail");
        match err {
            AppError::Validation(msg) => assert!(msg.contains("bad space")),
            other => panic!("expected Validation, got {other:?}"),
        }
    }

    // Seed a minimal `servers` row so tag rows have a valid parent id.
    async fn seed_server(db: &DatabaseConnection, id: &str) {
        let now = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        server::ActiveModel {
            id: Set(id.to_string()),
            name: Set(format!("srv-{id}")),
            weight: Set(0),
            hidden: Set(false),
            capabilities: Set(0),
            protocol_version: Set(1),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("insert test server should succeed");
    }

    // list_tags returns an empty vec when the server has no tags.
    #[tokio::test]
    async fn list_tags_empty_when_none() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1").await;
        let got = list_tags(&db, "s1").await.unwrap();
        assert!(got.is_empty());
    }

    // list_tags returns the server's tags sorted ascending by tag name.
    #[tokio::test]
    async fn list_tags_returns_sorted() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1").await;
        for tag in ["zeta", "alpha", "mid"] {
            server_tag::ActiveModel {
                server_id: Set("s1".to_string()),
                tag: Set(tag.to_string()),
            }
            .insert(&db)
            .await
            .unwrap();
        }
        let got = list_tags(&db, "s1").await.unwrap();
        assert_eq!(got, vec!["alpha", "mid", "zeta"]);
    }

    // list_tags filters strictly by server_id and ignores other servers' tags.
    #[tokio::test]
    async fn list_tags_filters_by_server() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1").await;
        seed_server(&db, "s2").await;
        server_tag::ActiveModel {
            server_id: Set("s1".to_string()),
            tag: Set("only-s1".to_string()),
        }
        .insert(&db)
        .await
        .unwrap();
        server_tag::ActiveModel {
            server_id: Set("s2".to_string()),
            tag: Set("only-s2".to_string()),
        }
        .insert(&db)
        .await
        .unwrap();
        assert_eq!(list_tags(&db, "s1").await.unwrap(), vec!["only-s1"]);
        assert_eq!(list_tags(&db, "s2").await.unwrap(), vec!["only-s2"]);
    }

    // set_tags persists the normalized tags and returns them.
    #[tokio::test]
    async fn set_tags_inserts_and_returns_normalized() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1").await;
        let returned = set_tags(&db, "s1", vec!["  prod ".into(), "web".into()])
            .await
            .unwrap();
        assert_eq!(returned, vec!["prod", "web"]);
        // Persisted state matches the normalized, sorted result.
        assert_eq!(list_tags(&db, "s1").await.unwrap(), vec!["prod", "web"]);
    }

    // set_tags deduplicates and sorts before persisting.
    #[tokio::test]
    async fn set_tags_dedupes_and_sorts() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1").await;
        let returned = set_tags(&db, "s1", vec!["b".into(), "a".into(), "b".into(), " a ".into()])
            .await
            .unwrap();
        assert_eq!(returned, vec!["a", "b"]);
        assert_eq!(list_tags(&db, "s1").await.unwrap(), vec!["a", "b"]);
    }

    // set_tags replaces the previous tag set rather than appending.
    #[tokio::test]
    async fn set_tags_replaces_existing() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1").await;
        set_tags(&db, "s1", vec!["old1".into(), "old2".into()])
            .await
            .unwrap();
        set_tags(&db, "s1", vec!["new1".into()]).await.unwrap();
        // Only the new tag remains after replacement.
        assert_eq!(list_tags(&db, "s1").await.unwrap(), vec!["new1"]);
    }

    // set_tags with an empty list clears all existing tags for the server.
    #[tokio::test]
    async fn set_tags_empty_clears_all() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1").await;
        set_tags(&db, "s1", vec!["a".into(), "b".into()])
            .await
            .unwrap();
        let returned = set_tags(&db, "s1", vec![]).await.unwrap();
        assert!(returned.is_empty());
        assert!(list_tags(&db, "s1").await.unwrap().is_empty());
    }

    // set_tags only affects the target server, leaving other servers' tags intact.
    #[tokio::test]
    async fn set_tags_scoped_to_server() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1").await;
        seed_server(&db, "s2").await;
        set_tags(&db, "s1", vec!["keep".into()]).await.unwrap();
        set_tags(&db, "s2", vec!["replace".into()]).await.unwrap();
        // s1's tags are untouched by the s2 write.
        assert_eq!(list_tags(&db, "s1").await.unwrap(), vec!["keep"]);
        assert_eq!(list_tags(&db, "s2").await.unwrap(), vec!["replace"]);
    }

    // set_tags rejects invalid input and leaves the existing DB state unchanged.
    #[tokio::test]
    async fn set_tags_validation_error_does_not_mutate() {
        let (db, _tmp) = setup_test_db().await;
        seed_server(&db, "s1").await;
        set_tags(&db, "s1", vec!["existing".into()]).await.unwrap();
        let err = set_tags(&db, "s1", vec!["bad space".into()])
            .await
            .err()
            .expect("invalid tag must fail");
        assert!(matches!(err, AppError::Validation(_)));
        // Validation happens before the transaction, so the prior tag survives.
        assert_eq!(list_tags(&db, "s1").await.unwrap(), vec!["existing"]);
    }

    // Deleting a server cascades to its tag rows when FK enforcement is on.
    #[tokio::test]
    async fn deleting_server_cascades_tags() {
        let (db, _tmp) = setup_test_db().await;
        // setup_test_db does not enable FK enforcement, so turn it on for this test.
        db.execute_unprepared("PRAGMA foreign_keys=ON")
            .await
            .unwrap();
        seed_server(&db, "s1").await;
        set_tags(&db, "s1", vec!["t1".into(), "t2".into()])
            .await
            .unwrap();
        server::Entity::delete_by_id("s1")
            .exec(&db)
            .await
            .unwrap();
        // Cascade removed all tag rows belonging to the deleted server.
        let remaining = server_tag::Entity::find().all(&db).await.unwrap();
        assert!(remaining.is_empty());
    }
}
