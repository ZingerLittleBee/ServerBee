# Documentation app

This app serves the landing page and bilingual documentation with TanStack Start and Fumadocs. Published content
lives in the [English](content/docs/en/) and [Chinese](content/docs/zh/) content directories. The app's
[package manifest](package.json) and [Vite configuration](vite.config.ts) are the source of truth for its framework
versions and build preset; dated specs under [docs/superpowers](../../docs/superpowers/) describe earlier designs.

| Task | Start here |
| --- | --- |
| Landing layout, copy, colors | [Components](src/components/landing/), [styles](src/styles/landing.css) |
| Docs layout and navigation | [Docs route](src/routes/$lang/docs/$.tsx), [framework links](src/components/framework-link.tsx) |
| Search and Markdown exports | [Search route](src/routes/api/search.ts), [content source](src/lib/source.ts), [LLM exports](src/lib/llms.ts), [MDX configuration](source.config.ts) |
| Scroll restoration or a router upgrade | [Scroll restoration](src/lib/scroll-restoration.ts), [browser runner](scripts/check-browser.ts), [manual checklist](../../tests/manual/docs-scroll-restoration.md) |
| Content and route contracts | [Content checker](scripts/check-contracts.ts), [route checker](scripts/check-routes.ts), [Documentation CI](../../.github/workflows/docs.yml) |
| Production preview | [Node server](scripts/start-production.ts), [package commands](package.json), [Vercel build configuration](vite.config.ts) |

## Build and preview

Use Node 24 and the repository's Bun lockfile. From the repository root:

```bash
bun install --frozen-lockfile
cd apps/docs
bun run build
bun run preview
```

`start` and `preview` both serve the production build at `http://127.0.0.1:4000`. Set `HOST` and `PORT` to override
the listener. They run `.vercel/output/functions/__server.func/index.mjs` and serve `.vercel/output/static` using
Node's built-in HTTP server. No Vercel login, global server package, or session scratchpad is needed. The preview
closes each HTTP connection because this Nitro Vercel handler defines a socket property once per request and
cannot handle a reused socket. SSR responses stream directly from the generated handler.

For development, use `bun run dev`. Keep Node 24: the existing Node 26 dev-server header issue is documented in the
manual checklist. Production preview is the validation target used by Documentation CI.

## Checks

With the production preview running, use a second terminal in `apps/docs`:

```bash
bun run types:check
bun run check:contracts
SERVERBEE_DOCS_BASE_URL=http://127.0.0.1:4000 bun run check:routes
bun x playwright install chromium
BASE_URL=http://127.0.0.1:4000 bun run check:browser
```

The production preview, route checks, and browser checks execute on Node 24. Bun installs dependencies and dispatches
these commands. Route checks start with two 404 responses followed by a normal SSR page, consuming each body before
the next request. This covers Bun 1.3.4's reuse of the preview's `Connection: close` socket, which returned 503.

The browser runner checks real navigation, scrolling before hydration in both locales, reloads, and Back/Forward
through long pages, landing anchors, notes, and the docs table of contents. It exits nonzero on a failed scenario
or an uncaught page error, and prints the number of scenarios actually executed. Documentation CI runs Chromium.
For a local cross-engine pass:

```bash
bun x playwright install chromium firefox webkit
BASE_URL=http://127.0.0.1:4000 BROWSERS=chromium,firefox,webkit bun run check:browser
```

The [manual checklist](../../tests/manual/docs-scroll-restoration.md) covers additional cases such as text
fragments, language switching, search results, and repeated anchors. Run it after changing scroll restoration or
upgrading TanStack Router/Start. The runner does not replace those checks.

## Hosting and deployed revision

The following account navigation was confirmed during the 2026-10-02 deployment investigation. Project names,
domain assignments, Git connections, and the deployed revision are external state: check them live before a
deployment decision.

| Surface | Account pointer and purpose |
| --- | --- |
| Docs and landing | [Vercel `serverbee-docs`](https://vercel.com/zingerlittlebees-projects/serverbee-docs), serving `docs.serverbee.app`; app root `apps/docs` |
| DNS | Cloudflare manages `serverbee.app` DNS. DNS ownership does not identify the application's runtime host. |
| Monitoring backend | Railway runs the Rust Server used by `demo.serverbee.app`. Its GitHub deployment statuses do not identify the deployed docs revision. |

The historical deployment label `server-bee-docs` differs from the project name `serverbee-docs`. Resolve the
current custom-domain deployment rather than deriving a project name from an old deployment URL.

After signing in to Vercel CLI separately, these commands read deployment information:

```bash
vercel project inspect serverbee-docs --scope zingerlittlebees-projects
vercel inspect docs.serverbee.app --scope zingerlittlebees-projects
vercel inspect docs.serverbee.app --scope zingerlittlebees-projects --format json \
  | jq '{url, readyState, target, source, gitSha: .meta.githubCommitSha, gitRef: .meta.githubCommitRef}'
gh api repos/ZingerLittleBee/ServerBee/commits/main --jq .sha
```

Compare the deployment's Git SHA with the intended commit. If Git metadata is absent (for example, a CLI
deployment), inspect the deployment's Source and build records in the project dashboard; the domain's HTML or
an old successful GitHub CI run is insufficient to establish a SHA. Also confirm the current project's Root
Directory, Git repository/production branch, and Node runtime against the [Vite configuration](vite.config.ts). These read-only commands
do not link a project, redeploy it, or change domain assignments.

References: [Vercel inspect](https://vercel.com/docs/cli/inspect), [Nitro Vercel output](https://nitro.build/deploy/providers/vercel),
[Playwright browser installation](https://playwright.dev/docs/browsers).
