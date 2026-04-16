use sea_orm::DbErr;

pub(crate) fn is_unique_violation(err: &DbErr) -> bool {
    let message = err.to_string();
    message.contains("UNIQUE constraint failed") || message.contains("UNIQUE")
}

pub(crate) fn is_active_recovery_conflict(err: &DbErr) -> bool {
    let message = err.to_string();
    is_unique_violation(err) || message.contains("recovery_job_active_conflict")
}
