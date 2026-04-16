use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{ConnectionTrait, DbBackend, FromQueryResult, Statement};
use serde_json::Value;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260416_000017_migrate_email_to_resend"
    }
}

#[derive(FromQueryResult)]
struct EmailRow {
    id: String,
    name: String,
    config_json: String,
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();

        let rows: Vec<EmailRow> = EmailRow::find_by_statement(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            "SELECT id, name, config_json FROM notification WHERE notify_type = 'email'",
            [],
        ))
        .all(db)
        .await?;

        if rows.is_empty() {
            return Ok(());
        }

        for row in rows {
            match convert_email_config(&row.config_json) {
                Ok(new_json) => {
                    db.execute(Statement::from_sql_and_values(
                        DbBackend::Sqlite,
                        "UPDATE notification SET config_json = ? WHERE id = ?",
                        [new_json.into(), row.id.clone().into()],
                    ))
                    .await?;
                }
                Err(reason) => {
                    tracing::warn!(
                        "Disabling email notification {} ({}): unconvertable legacy config ({reason})",
                        row.id,
                        row.name,
                    );
                    let new_name = format!("{} (needs reconfiguration)", row.name);
                    db.execute(Statement::from_sql_and_values(
                        DbBackend::Sqlite,
                        "UPDATE notification SET name = ?, enabled = 0 WHERE id = ?",
                        [new_name.into(), row.id.clone().into()],
                    ))
                    .await?;
                }
            }
        }

        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

fn convert_email_config(old_json: &str) -> Result<String, String> {
    let val: Value = serde_json::from_str(old_json).map_err(|e| format!("invalid JSON: {e}"))?;

    let from = val
        .get("from")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing 'from' field".to_string())?;
    let to = val
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing 'to' field".to_string())?;

    if from.is_empty() || to.is_empty() {
        return Err("empty from/to".to_string());
    }

    Ok(serde_json::json!({
        "from": from,
        "to": [to],
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_email_config_happy_path() {
        let old = r#"{"smtp_host":"smtp.gmail.com","smtp_port":587,"username":"u","password":"p","from":"a@b.com","to":"c@d.com"}"#;
        let new = convert_email_config(old).expect("should convert");
        let v: Value = serde_json::from_str(&new).unwrap();
        assert_eq!(v["from"], "a@b.com");
        assert_eq!(v["to"][0], "c@d.com");
        assert_eq!(v["to"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_convert_email_config_missing_from() {
        let old = r#"{"to":"c@d.com"}"#;
        assert!(convert_email_config(old).is_err());
    }

    #[test]
    fn test_convert_email_config_missing_to() {
        let old = r#"{"from":"a@b.com"}"#;
        assert!(convert_email_config(old).is_err());
    }

    #[test]
    fn test_convert_email_config_empty_from() {
        let old = r#"{"from":"","to":"c@d.com"}"#;
        assert!(convert_email_config(old).is_err());
    }

    #[test]
    fn test_convert_email_config_garbage_json() {
        assert!(convert_email_config("not json").is_err());
    }
}
