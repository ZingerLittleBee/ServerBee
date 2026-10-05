---
status: accepted
---

# Renewal tracking separates confirmed expiry from projected expiry

ServerBee's renewal feature manages operator-maintained records and reminders; payment and actual service renewal remain with the provider. Provider API integration and payment execution are outside this feature's scope.

An operator-recorded confirmed expiry and a projected expiry have different meanings and must remain distinguishable. An expected renewal can advance the projected expiry without overwriting the confirmed expiry or claiming that payment succeeded. This permits convenient forecasting while retaining the last operator-confirmed provider information.

Projection is enabled explicitly per server and is disabled by default. An enabled projection follows the expected renewal schedule independently of the Agent's online state; connectivity does not establish whether a provider charged for or renewed the service.

An enabled projection advances automatically when its renewal boundary is reached, without requiring a per-period confirmation action. Automatic mode uses the projected expiry as the current renewal-reminder deadline. After advancement, reminders concern the next period rather than continuously asking the operator to confirm an earlier period; the confirmed expiry remains historical information. This supersedes the earlier decision to retain an awaiting-confirmation reminder after every projected renewal. The operational assumption does not establish that the provider received payment.

Renewal intervals retain the existing monthly, quarterly, and yearly choices and advance by one, three, or twelve calendar months. Calculations preserve the original day-of-month anchor and clamp to the target month's final day only when that month lacks the anchor day, so a January 31 monthly schedule returns to March 31 after February.

Expiry is entered as a date rather than an exact provider timestamp, and the service remains valid through the end of that date in an operator-selected billing timezone. Calendar calculations use that timezone, while stored expiry instants use UTC. The billing timezone must remain available alongside the instants so later renewals preserve the intended local calendar, including changes in UTC offset.

Reminders use the existing configured expiration alerts and page indicators; this feature does not introduce a new default recurring notification schedule or notification-channel configuration.

This records the agreed design boundary, not a shipped implementation. Catch-up behavior, disabling projection, historical-record handling, manual renewal entry, timezone changes, and the relationship to cost and traffic periods remain to be decided.
