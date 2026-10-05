---
status: accepted
---

# Renewal tracking separates confirmed expiry from projected expiry

ServerBee's renewal feature manages operator-maintained records and reminders; payment and actual service renewal remain with the provider. Provider API integration and payment execution are outside this feature's scope.

An operator-recorded confirmed expiry and a projected expiry have different meanings and must remain distinguishable. An expected renewal can advance the projected expiry without overwriting the confirmed expiry or claiming that payment succeeded. This permits convenient forecasting while preserving the information needed to identify an unconfirmed renewal; automatically replacing the recorded deadline would erase that distinction.

Projection is enabled explicitly per server and is disabled by default. An enabled projection follows the expected renewal schedule independently of the Agent's online state; connectivity does not establish whether a provider charged for or renewed the service.

When the confirmed expiry passes, renewal reminders remain active even if the projected expiry has advanced into a future period. The projection does not resolve an unconfirmed renewal; the interface continues to identify it as awaiting confirmation.

This records the agreed design boundary, not a shipped implementation. Renewal entry, calendar rules, date precision, reminder delivery, and treatment of existing records remain to be decided.
