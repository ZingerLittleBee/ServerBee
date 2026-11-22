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
| Entrance, scroll reveal, animated scenes | [Motion styles](src/styles/motion.css), [motion runtime](src/scripts/motion/index.ts) |
| Where a scene finds things inside a screenshot | [Capture regions](src/assets/shots/regions.json), written by the web capture script |
| Head tags, fonts, canonical origin | [Layout](src/layouts/base.astro), [Astro configuration](astro.config.ts) |
| Product screenshots | [Web captures](scripts/capture.ts), [iOS captures](scripts/capture-ios.ts), [link previews](scripts/capture-og.ts) |
| The drawings in the kit section | [Sketches](src/components/sketch.astro), their words under `more.sketch` in the [copy](src/i18n/copy.ts), their runtime in [kit.ts](src/scripts/motion/kit.ts) |
| Contracts with the rest of the repository | [Contract checker](scripts/check-contracts.ts), [Landing CI](../../.github/workflows/landing.yml) |

## Stack

- Astro with static output and no UI framework. The page ships one script, for the theme toggle, the install tabs,
  the copy buttons and the motion, and works without it.
- Plain CSS in two stylesheets: the page, and its motion. Both themes come from `light-dark()` tokens; a stored
  choice sets `data-theme` on the root element.
- Latin fonts are self-hosted through Astro's fonts API. Chinese text uses the reader's system fonts.
- Screenshots are real captures of the product, kept lossless in `src/assets`. The build resizes them to AVIF, with
  WebP as the fallback, and each one has a light and a dark variant that follows the theme.
- The kit section draws each feature instead of capturing it: a sketch is markup and CSS around sample data, in
  the product's own terms. Change a number there only together with the rule it shows, such as an alert firing on
  7 of 10 samples.
- English is served at `/` and Simplified Chinese at `/zh/`. Both locales render the same components from one copy
  file, and TypeScript requires every string in both.

## Motion

The page animates without a library: CSS for whatever runs on its own, and the Web Animations API for what the
script times.

- The entrance is CSS. An inline script in the layout sets `enter` on the root element before the first paint, and
  only for a reader who has not asked for reduced motion. Arriving through the cross-fade between the two
  languages removes it again.
- Nothing is hidden by a stylesheet alone. Scroll reveal and the scenes start from attributes the runtime sets, so
  the page reads complete when the script never runs.
- A scene animates a real screenshot. It lays a layer over the capture, covers a region with the color sampled
  beside it, and moves a copy of that region. The regions come from `regions.json`, which the capture script
  reads from the product's own DOM, so a scene stays aligned when its capture is retaken.
- A sketch in the kit section starts empty and draws itself once, when it is in view. Its transitions live on the
  finished state, and the runtime's `data-empty` mark holds the first frame, so removing the mark plays it.
- With reduced motion the hero's routes are drawn and still, the page rail follows the scroll, and nothing else
  mounts.

Check motion against `bun run build` and `bun run preview`, not the dev server: the entrance is timed from the
first paint, and the production bundle is what readers get. The hero's frame is the largest element on the page,
so its fade decides the largest contentful paint; keep it early.

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

Production is the Vercel project
[`serverbee-landing`](https://vercel.com/zingerlittlebees-projects/serverbee-landing): root directory `apps/landing`,
the Astro preset, Node 24.x, no environment variables. It serves `serverbee.app`, redirects `www.serverbee.app` to
it, and deploys every push to `main`. This was the state on 2026-11-22. Project settings and domain assignments are
external state: check them live before a deployment decision.

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

Each run also rewrites `src/assets/shots/regions.json`, the positions the animated scenes read. Commit it with the
captures it describes. `--regions-only` records the regions again without retaking any screenshot.

The repository's [README](../../README.md) and its [Chinese version](../../README.zh-CN.md) show the `fleet` captures
straight from `src/assets/shots`, so those four files keep their names.

The iOS captures come from the iOS Simulator, signed in to the same demo server. Install a Debug build of
[apps/ios](../ios/) on a booted simulator, run `capture` first, then:

```bash
bun run capture:ios [simulator-udid]
```

The link preview images are laid out inside the running page. With `bun run dev` or `bun run preview` running:

```bash
bun run capture:og
```
