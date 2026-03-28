use std::sync::Arc;

use chrono::Utc;
use sea_orm::prelude::Expr;
use sea_orm::*;

use crate::entity::{mobile_device_registration, mobile_push_delivery};
use crate::state::AppState;

const EXPO_RECEIPTS_URL: &str = "https://exp.host/--/api/v2/push/getReceipts";

/// Runs every 15 minutes. Fetches Expo receipts for deliveries
/// that have a ticket_id but no receipt_status yet, and were sent
/// more than 15 minutes ago.
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(900));
    loop {
        interval.tick().await;

        let cutoff = Utc::now() - chrono::Duration::minutes(15);

        let pending = match mobile_push_delivery::Entity::find()
            .filter(mobile_push_delivery::Column::TicketId.is_not_null())
            .filter(mobile_push_delivery::Column::ReceiptStatus.is_null())
            .filter(mobile_push_delivery::Column::SentAt.lt(cutoff))
            .all(&state.db)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("Receipt checker query error: {e}");
                continue;
            }
        };

        if pending.is_empty() {
            continue;
        }

        for chunk in pending.chunks(1000) {
            let ids: Vec<String> = chunk
                .iter()
                .filter_map(|d| d.ticket_id.clone())
                .collect();

            let client = reqwest::Client::new();
            let resp = client
                .post(EXPO_RECEIPTS_URL)
                .json(&serde_json::json!({ "ids": ids }))
                .send()
                .await;

            match resp {
                Ok(r) if r.status().is_success() => {
                    if let Ok(body) = r.json::<serde_json::Value>().await
                        && let Some(data) = body.get("data").and_then(|d| d.as_object())
                    {
                        let now = Utc::now();
                        for delivery in chunk {
                            if let Some(tid) = &delivery.ticket_id
                                && let Some(receipt) = data.get(tid.as_str())
                            {
                                let status = receipt
                                    .get("status")
                                    .and_then(|s| s.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let message = receipt
                                    .get("message")
                                    .and_then(|m| m.as_str())
                                    .map(|s| s.to_string());

                                let mut update: mobile_push_delivery::ActiveModel =
                                    delivery.clone().into();
                                update.receipt_status = Set(Some(status.clone()));
                                update.receipt_message = Set(message);
                                update.receipt_checked_at = Set(Some(now));
                                let _ = update.update(&state.db).await;

                                // If DeviceNotRegistered, disable the device
                                if status == "error"
                                    && let Some(details) = receipt
                                        .get("details")
                                        .and_then(|d| d.get("error"))
                                        .and_then(|e| e.as_str())
                                    && details == "DeviceNotRegistered"
                                {
                                    disable_device(
                                        &state.db,
                                        &delivery.device_registration_id,
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                }
                Ok(r) => {
                    tracing::error!("Expo receipts API returned {}", r.status());
                }
                Err(e) => {
                    tracing::error!("Expo receipts request failed: {e}");
                }
            }
        }
    }
}

async fn disable_device(db: &DatabaseConnection, device_registration_id: &str) {
    let now = Utc::now();
    let _ = mobile_device_registration::Entity::update_many()
        .filter(mobile_device_registration::Column::Id.eq(device_registration_id))
        .col_expr(
            mobile_device_registration::Column::PushToken,
            Expr::value(Option::<String>::None),
        )
        .col_expr(
            mobile_device_registration::Column::DisabledAt,
            Expr::value(Some(now)),
        )
        .col_expr(
            mobile_device_registration::Column::UpdatedAt,
            Expr::value(now),
        )
        .exec(db)
        .await;
}
