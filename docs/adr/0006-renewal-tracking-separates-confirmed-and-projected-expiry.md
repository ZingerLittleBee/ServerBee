---
status: accepted
---

# Renewal tracking separates confirmed expiry from projected expiry

ServerBee's renewal feature manages operator-maintained records and reminders; payment and actual service renewal remain with the provider. Provider API integration and payment execution are outside this feature's scope.

An operator-recorded confirmed expiry and a projected expiry have different meanings and must remain distinguishable. An expected renewal can advance the projected expiry without overwriting the confirmed expiry or claiming that payment succeeded. This permits convenient forecasting while preserving the information needed to identify an unconfirmed renewal; automatically replacing the recorded deadline would erase that distinction.

Projection is enabled explicitly per server and is disabled by default. An enabled projection follows the expected renewal schedule independently of the Agent's online state; connectivity does not establish whether a provider charged for or renewed the service.

When the confirmed expiry passes, renewal reminders remain active even if the projected expiry has advanced into a future period. The projection does not resolve an unconfirmed renewal; the interface continues to identify it as awaiting confirmation.

An enabled projection advances automatically when its renewal boundary is reached, without requiring a per-period confirmation action. How that advancement affects the awaiting-confirmation indicator and the reminder deadline is being reconsidered; the preceding reminder rule remains an earlier decision, not a settled implementation contract.

Renewal intervals retain the existing monthly, quarterly, and yearly choices and advance by one, three, or twelve calendar months. Calculations preserve the original day-of-month anchor and clamp to the target month's final day only when that month lacks the anchor day, so a January 31 monthly schedule returns to March 31 after February.

Expiry is entered as a date rather than an exact provider timestamp, and the service remains valid through the end of that date. The timezone defining that day remains to be decided. Reminders use the existing configured expiration alerts and page indicators; this feature does not introduce a new default recurring notification schedule or notification-channel configuration.

This records the agreed design boundary, not a shipped implementation. Automatic advancement's confirmation and reminder semantics, timezone, manual renewal entry, historical-record handling, and the relationship to cost and traffic periods remain to be decided.
