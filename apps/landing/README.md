# ServerBee landing page

The marketing site for ServerBee: one bilingual page, built with Astro into static files. It is a site of its own,
built and deployed apart from the documentation app in [apps/docs](../docs/README.md). The two share no code, and
every link from this page to the documentation is absolute.

| Task | Start here |
| --- | --- |
| Copy, in either language | [Copy](src/i18n/copy.ts) |
| Links, install commands, section order per language | [Site constants](src/lib/site.ts) |
| The release and iOS version the page names | [Release facts](src/lib/release.ts), read from the workspace at build time |
| Page structure | [Page](src/components/landing.astro), [sections](src/components/sections/) |
| Colors, type, layout | [Stylesheet](src/styles/global.css) |
| Theme toggle, install tabs, copy buttons | [Client script](src/scripts/landing.ts) |
| Head tags, fonts, canonical origin | [Layout](src/layouts/base.astro), [Astro configuration](astro.config.ts) |
| Product screenshots | [Web captures](scripts/capture.ts), [iOS captures](scripts/capture-ios.ts), [link previews](scripts/capture-og.ts) |
| Contracts with the rest of the repository | [Contract checker](scripts/check-contracts.ts), [Landing CI](../../.github/workflows/landing.yml) |

## Stack

- Astro with static output and no UI framework. The page ships one small script for the theme toggle, the install
  tabs and the copy buttons, and works without it.
- Plain CSS in one stylesheet. Both themes come from `light-dark()` tokens; a stored choice sets `data-theme` on the
  root element.
- Latin fonts are self-hosted through Astro's fonts API. Chinese text uses the reader's system fonts.
- Screenshots are real captures of the product, kept lossless in `src/assets`. The build resizes them to AVIF, with
  WebP as the fallback, and each one has a light and a dark variant that follows the theme.
- English is served at `/` and Simplified Chinese at `/zh/`. Both locales render the same components from one copy
  file, and TypeScript requires every string in both.

## Develop and build

Use Node 24 and the repository's Bun lockfile. From the repository root:

```bash
bun install --frozen-lockfile
cd apps/landing
bun run dev        # http://localhost:4100
bun run build      # writes dist/
bun run preview    # serves dist/ at http://localhost:4100
```

## Deploy

`bun run build` writes a self-contained static site to `dist/`. Any static host can serve it; nothing runs on a
server. The build reads `Cargo.toml` and `apps/ios/project.yml`, so build from a checkout of the whole repository.

Set `SITE_URL` when the page is served from an origin other than `https://serverbee.app`. Canonical links, `hreflang`
alternates, link preview images, the sitemap and `robots.txt` all use it.

## Checks

```bash
bun run types:check
bun run check:contracts
bun run build
```

`check:contracts` covers what the build cannot see: every documentation page and heading the copy links to exists in
that language, the license the page names is the workspace license, and the install commands only pass options that
`deploy/install.sh` parses. Lint with `bun x ultracite check` from the repository root.

## Screenshots

Recapture when the product's interface changes. The web captures need a local demo server and a built web bundle:

```bash
bun run build:web                                                    # repository root
SERVERBEE_RATE_LIMIT__LOGIN_MAX=1000 scripts/server-dev-demo.sh     # repository root, keep it running
bun x playwright install chromium                                    # apps/landing, once
bun run capture                                                      # apps/landing
```

`capture` renames the demo fleet and builds a dashboard through the demo server's API, then serves the remaining data
as fixtures; the header of [the script](scripts/capture.ts) lists which. `--only=<name>` recaptures one shot.

The iOS captures come from the iOS Simulator, signed in to the same demo server. Install a Debug build of
[apps/ios](../ios/) on a booted simulator, run `capture` first, then:

```bash
bun run capture:ios [simulator-udid]
```

The link preview images are laid out inside the running page. With `bun run dev` or `bun run preview` running:

```bash
bun run capture:og
```
