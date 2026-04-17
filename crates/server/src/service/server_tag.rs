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
}
