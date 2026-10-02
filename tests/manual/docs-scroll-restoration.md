# Docs Scroll Restoration — Manual Verification

Run this checklist after touching `apps/docs/src/lib/scroll-restoration.ts` or `apps/docs/src/router.tsx`, and after
any upgrade of `@tanstack/react-router` or `@tanstack/react-start`. The fixes rely on router internals (the history
state keys `__TSR_key` and `__hashScrollIntoViewOptions`, and the router's wrapper around `history.replaceState`), so
an upgrade can break them without a type error. Check Chrome, Safari and Firefox.

## Setup

The reproducible production preview and browser runner are documented in
[`apps/docs/README.md`](../../apps/docs/README.md). Run its automated scenarios first:

```bash
cd apps/docs
bun run build
bun run preview
# In a second terminal in apps/docs:
bun x playwright install chromium
BASE_URL=http://127.0.0.1:4000 bun run check:browser
```

The runner covers hydration, reloads, long-page restoration, and several fragment history paths. Continue with the
remaining manual cases below on Chrome, Safari, and Firefox; passing Chromium does not establish the other engines.
For iterative development, you can instead start:

```bash
cd apps/docs && bun run dev
```

Run it on Node 22 or 24: a Node 26 dev server drops the response headers the app sets, which breaks client-side
navigation. Open `http://localhost:4000`. For the steps that scroll before hydration, throttle the network in the
browser's developer tools so that there is time to scroll before the page becomes interactive.

## Scrolling before hydration

- [ ] A fresh visit to `/en` and to `/en/docs/quick-start` starts at the top.
- [ ] Open `/en`, scroll down before hydration: once the page hydrates it stays where you scrolled.
- [ ] Same on `/en/docs/quick-start` and `/zh/docs/quick-start`.
- [ ] After scrolling before hydration, follow a link to another page and press Back: you return to that position.
- [ ] Chrome: open `/en/docs/quick-start#:~:text=Verify%20the%20connection`. The highlighted text stays in view after
      hydration.
- [ ] Reload a scrolled page, plain and with a fragment (`/en#faq`): it keeps its position.
- [ ] Open `/en#faq` and `/en/docs/quick-start#manage-the-agent`: each lands on its target.

## Back to a long page

- [ ] On `/en/docs/configuration`, scroll most of the way down, follow a sidebar link, then press Back: the same line is
      at the top as when you left. Safari landed lower by the room that the code blocks and tables above make for their
      horizontal scrollbars.

## Back and Forward to entries with a fragment

For each entry point: follow it, scroll on by about a screen, leave through a link to another page, then press Back.
You must return to where you were, not to the fragment.

- [ ] A note mark (the small numbers) in the landing's "Two commands and you're live" section.
- [ ] A landing nav anchor, such as IP quality.
- [ ] A docs table of contents entry, and a heading anchor.
- [ ] A docs link to another page's heading: on `/en/docs/alerts`, "Capabilities → Temporary grants".
- [ ] A search result for a heading (Cmd/Ctrl+K, "temporary grants"): Back, then Forward, each return to where you were.
- [ ] The docs language switch on `/en/docs/quick-start#manage-the-agent` after scrolling: Back returns to the scrolled
      English page, and the same from Chinese to English.
- [ ] Forward into a fragment entry you first reached by a jump lands on the fragment.
- [ ] The landing's brand link (`#top`) while already at the top, then a docs link, then Back: the landing at the top.
- [ ] Following the same table of contents entry twice, then Back, returns to where you were.

## Known limitations

- History entries with a fragment that were created before hydration, other than the current one, have no saved
  position, so Back lands on their fragment.
- On a URL with a fragment, a reader who scrolls before hydration is returned to the fragment when the page hydrates.
- Firefox: following the table of contents entry of the fragment you are already on adds a history entry the router
  never sees, so Back from it lands on the fragment.
