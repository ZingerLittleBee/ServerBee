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

Enabling projection on an already-expired record, or resuming after several missed periods, advances to the first renewal deadline that has not expired. The calculation preserves the original calendar anchor rather than starting a new schedule from the current date, and missed projected periods do not create payment records.

Disabling projection freezes its current deadline instead of reverting to an older confirmed expiry. Reminders then use that frozen deadline, and its projected origin remains distinguishable from provider-confirmed information.

Changing the billing timezone preserves the selected local expiry date and recalculates its UTC boundary in the new timezone. It does not preserve the previous UTC instant at the expense of changing the displayed date.

The feature includes the Server, web dashboard, and native iOS client. Both clients must use the stored billing timezone for renewal entry and display rather than independently interpreting dates in the browser or device timezone.

Existing servers start with projection disabled and UTC as their billing timezone. Migration preserves existing expiry instants rather than silently correcting dates or extending service validity. The next explicit save of billing settings applies the selected local date, billing timezone, and end-of-date boundary rules. Existing operator-entered expiry values do not gain a new claim that the provider verified payment merely because the schema changed.

When projection is disabled, operators maintain the renewal deadline through the existing server billing-settings editor. This feature adds neither a dedicated manual-renewal button nor a per-period confirmation dialog. An explicit operator edit replaces the renewal deadline and its date anchor rather than requiring the operator to reconstruct the missed schedule.

Automatic advancement changes renewal dates and reminder targets, not price, currency, billing interval, traffic allowance, or traffic-reset rules. Existing cost and traffic period calculations remain unchanged, including their calendar-based quarterly and yearly boundaries. The interface distinguishes the renewal deadline from the cost-estimation period instead of presenting those independent calculations as the same provider-confirmed billing period.

## Delivery boundary

The agreed feature includes per-server opt-in projection, billing timezone selection, calendar-aware advancement and catch-up, freeze-on-disable behavior, and consistent renewal deadlines across Server responses, cost expiry advisories, existing expiration alerts, web views, and native iOS entry and display. Reminders continue to use the configured channels and rules; automatic advancement retargets them to the next projected deadline. Public status responses retain their existing exclusion of private billing information.

This is the finalized feature design, not a shipped implementation. Implementation must validate local date and UTC conversions, month-end and leap-year anchors, missed-period catch-up, enable/disable transitions, legacy-record preservation, reminder deadline changes, and cross-client consistency. Existing unrelated notification-scheduling defects and a redesign of cost or traffic periods are outside this feature's scope.
