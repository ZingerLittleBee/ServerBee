# Custom Theme, E2E Manual Checklist

## Setup

- [ ] Start `cargo run -p serverbee-server` against a fresh database.
- [ ] Log in as an admin in one browser.
- [ ] Log in as a member in another browser or private window.

## Dashboard Themes

- [ ] `/settings/appearance` shows all preset cards and the `My themes` section.
- [ ] Click a preset card, the dashboard repaints and the card becomes active.
- [ ] Reload the member browser, it sees the same active dashboard theme.

## Create And Edit

- [ ] Click `New theme`, enter a name, and fork from `Tokyo Night`.
- [ ] The editor opens with preset variables prefilled.
- [ ] Change the `primary` light variable, the preview updates immediately.
- [ ] Switch to the dark tab, dark variables and preview are independent.
- [ ] Save, the app returns to the appearance page and the custom card appears.
- [ ] Activate the custom card, the dashboard repaints using the custom variables.

## Delete With References

- [ ] Activate a custom theme, then try deleting it.
- [ ] The dialog blocks deletion and lists the admin dashboard reference.
- [ ] Switch the admin theme back to a preset, then delete the custom theme successfully.
- [ ] Bind a custom theme to a status page, then try deleting it.
- [ ] The dialog blocks deletion and lists that status page.

## Import And Export

- [ ] Open a custom theme editor and click `Export`.
- [ ] The downloaded JSON includes `version`, `name`, `vars_light`, and `vars_dark`.
- [ ] Change the JSON name locally and import it from the appearance page.
- [ ] A new custom theme card appears.

## Public Status Pages

- [ ] Edit a status page and bind it to a custom theme.
- [ ] Reload `/status/<slug>`, the page uses the custom theme.
- [ ] Bind the same page to a preset, the public page uses that preset.
- [ ] Select `Follow admin default`, the public page follows the active dashboard theme.

## Legacy Migration

- [ ] In a fresh admin browser, set `localStorage.color-theme = 'tokyo-night'`.
- [ ] Reload `/settings/appearance`, the migration prompt appears.
- [ ] Click `Apply`, the active dashboard theme changes to Tokyo Night.
- [ ] Reload again, the prompt does not reappear.
- [ ] Repeat as a member, no prompt appears.

## Feature Flag

- [ ] Restart the server with `SERVERBEE_FEATURE__CUSTOM_THEMES=false`.
- [ ] Custom theme write endpoints reject mutations.
- [ ] Any active custom dashboard theme resolves to the default preset.
- [ ] Re-enable the flag, the previous custom theme can be used again.

## Mobile

- [ ] At a 375px viewport, the editor remains usable.
- [ ] Public status pages render correctly on mobile with a custom theme.
