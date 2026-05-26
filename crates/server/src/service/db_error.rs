use sea_orm::DbErr;

pub(crate) fn is_unique_violation(err: &DbErr) -> bool {
    let message = err.to_string();
    message.contains("UNIQUE constraint failed") || message.contains("UNIQUE")
}
