---
status: accepted
---

# Renewal tracking separates confirmed expiry from projected expiry

ServerBee's renewal feature manages operator-maintained records and reminders; payment and actual service renewal remain with the provider. Provider API integration and payment execution are outside this feature's scope.

An operator-recorded confirmed expiry and a projected expiry have different meanings and must remain distinguishable. An expected renewal can advance the projected expiry without overwriting the confirmed expiry or claiming that payment succeeded. This permits convenient forecasting while preserving the information needed to identify an unconfirmed renewal; automatically replacing the recorded deadline would erase that distinction.

This records the agreed design boundary, not a shipped implementation. Projection activation, online-state handling, reminder policy, renewal entry, calendar rules, and treatment of existing records remain to be decided.
