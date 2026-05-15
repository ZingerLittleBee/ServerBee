# ServerBee Docs Landing Page Design

**Date:** 2026-05-15
**Status:** Draft
**Owner:** zingerbee
**Scope:** Replace the placeholder home page at `apps/docs/src/routes/$lang/index.tsx` with a Next.js-style marketing landing page that lives inside the existing Fumadocs site.

---

## 1. Goals

1. Present ServerBee to a first-time visitor in under one screen: what it is, why it's lightweight, how to install it.
2. Showcase the breadth of ServerBee's feature surface (monitoring, terminal, file manager, Docker, ping, alerts, themes, auto-upgrade) with **HTML/CSS animated demos**, not screenshots.
3. Match the visual language of modern Rust/JS project landing pages (Next.js, Bun, Vercel) — large gradient headlines, dark background, Bento grid, restrained motion.
4. Ship bilingual (`en`, `cn`) under the existing `$lang` segment with zero hard-coded copy.
5. Be **dark-mode only** on the landing route, regardless of the user's Fumadocs theme preference, without breaking the rest of the docs site.

## 2. Non-Goals

- No theme switcher on the landing page (always dark).
- No real-time data fetched from the live API — every animation is self-contained.
- No new runtime dependencies (no framer-motion, no lottie). Pure CSS keyframes + minimal inline SVG.
- No redesign of the `/$lang/docs/...` reader experience.
- No analytics integration in this iteration.

## 3. Information Architecture

The page is composed of 7 stacked sections inside the Fumadocs `HomeLayout` (which keeps the existing top nav + language switcher + GitHub link). Section anatomy from top to bottom:

| # | Section | Purpose | Key visual element |
|---|---------|---------|--------------------|
| 1 | **Hero** | Headline + value prop + dual CTA + install command | Mini live dashboard animation on the right |
| 2 | **Trust strip** | 3 quick stats / labels reinforcing "small + fast + zero deps" | Static iconography, hover lift only |
| 3 | **Three pillars** | Three large feature cards covering the core narrative | One animation per card |
| 4 | **Bento grid (8 cells)** | Full feature surface, varying tile sizes | One micro-animation per tile |
| 5 | **How it works** | 3 numbered steps with flowing arrows | Animated light-band moving across arrows |
| 6 | **Final CTA** | Big "Read the Docs" / "Star on GitHub" + install command | Hex-glow background |
| 7 | **Footer** | Provided by Fumadocs `HomeLayout` | — |

### 3.1 Three pillars (Section 3)

1. **Lightweight Rust probe** — animation: a "binary file" tile slides in, expands, and a green status dot lights up.
2. **Real-time over WebSocket** — animation: particles travel both ways between a Server icon and an Agent icon, looping every ~3s.
3. **One agent, full control** — animation: terminal / file / docker icons orbit a central node, slowly rotating.

### 3.2 Bento grid (Section 4 — 8 cells)

Layout (12-col grid, 4 rows on desktop; collapses to 1-col on `< md` even though we are dark-only, the page must still be responsive):

```
┌──────────────────────────┬──────────────┬──────────────┐
│  Network quality (2x2)   │ Themes 1x1   │ Alerts 1x1   │
│                          ├──────────────┼──────────────┤
│                          │ Service monitors 1x2        │
├──────────────────────────┼──────────────┴──────────────┤
│  Web Terminal (2x2)      │ File Manager 1x2            │
│                          ├──────────────┬──────────────┤
│                          │ Docker 1x1   │ Auto-upgrade │
└──────────────────────────┴──────────────┴──────────────┘
```

Per-cell animations:

- **Network quality** — small line chart whose newest point ticks every 1s, plus a column of latency dots that pulse from green → amber when a "loss" event scripts in.
- **Web Terminal** — typewriter prints `serverbee agent --version` → multi-line output, then clears and loops.
- **File Manager** — file tree expanding one folder, an upload progress bar fills, resets.
- **Docker** — three stacked container cards; status dots cycle running → restart → running.
- **Themes** — OKLCH color ring rotates slowly; the swatch under "primary" changes through 4 presets.
- **Alerts** — bell icon shakes briefly every ~5s; chip badges for Webhook / Telegram / Bark / Email / APNs fade in sequentially.
- **Service monitors** — 5 dots labeled SSL/DNS/HTTP/TCP/WHOIS, each blinks green at staggered intervals.
- **Auto-upgrade** — circular arrow rotates; version label morphs `v0.2.9` → `v0.3.0` on each full revolution.

### 3.3 Hero animation (Section 1)

A self-contained "mini server card" composed of:

- Two SVG donut rings (CPU, Memory) whose `stroke-dasharray` animates with `@keyframes` between 30% and 75% on a 4s loop.
- A network sparkline drawn with an inline `<svg><path>` whose `d` attribute is fixed; we translate a clip-path mask leftward to create the illusion of scrolling.
- A status dot that pulses via `box-shadow` keyframes.
- Background: layered radial gradients + a sparse honeycomb hex pattern (inline SVG, `position: absolute`, drifting upward at 0.5 px/frame via `transform: translateY` keyframes over 30s).

## 4. Tech Choices

| Concern | Choice | Reason |
|---------|--------|--------|
| Framework | Existing TanStack Start + Fumadocs `HomeLayout` | The page must remain part of the docs site so the nav and language switch are free. |
| Styling | Tailwind v4 (already imported) + a single `landing.css` for keyframes | Keeps animation primitives in one place, no JS overhead. |
| Animations | Pure CSS keyframes, inline SVG, `transform`/`opacity` only | Zero new deps; GPU-friendly; SSR-safe. |
| Icons | `lucide-react` (already a dep) | Consistent with the rest of docs. |
| Dark mode | Force `class="dark"` and `style="color-scheme: dark"` on the landing root | Fumadocs respects `.dark`; this scope is local to the page. |
| i18n | Local `translations.ts` keyed by `lang` param, no new i18n runtime | Matches the existing `$lang/index.tsx` pattern. |
| Copy-to-clipboard | `navigator.clipboard.writeText` behind a `'use client'`-equivalent island | TanStack Start hydrates the page; a small button component handles clicks. |

### 4.1 Install command UX

Hero and final CTA both show:

```
curl -fsSL https://serverbee.app/install.sh | sh
```

(Placeholder URL — to be replaced when the actual install script is published. Until then the command must remain visually present but the **click-to-copy** must copy whatever real command is documented in `quick-start.mdx`. The exact command will be sourced from `quick-start.mdx` during implementation; if not yet available there, fall back to `cargo install serverbee-server` as the placeholder that is also valid today.)

A `<CodeCopy>` component shows the command in a monospace pill with a copy icon that swaps to a check mark for 1.5s after click.

## 5. File / Component Layout

```
apps/docs/src/
  routes/$lang/index.tsx                  # rewrite: render <LandingPage lang={...} />
  components/landing/
    index.tsx                              # composes all sections
    translations.ts                        # { en: {...}, cn: {...} } strings
    sections/
      hero.tsx
      trust-strip.tsx
      pillars.tsx
      bento.tsx
      how-it-works.tsx
      final-cta.tsx
    primitives/
      gradient-heading.tsx                 # reused gradient text
      code-copy.tsx                        # copy-to-clipboard pill
      section.tsx                          # outer wrapper with consistent padding
    animations/
      mini-dashboard.tsx                   # Hero right-side
      hex-background.tsx                   # repeated decorative bg
      data-stream.tsx                      # Server↔Agent particles (pillar 2)
      orbit-icons.tsx                      # pillar 3
      install-binary.tsx                   # pillar 1
      ping-chart.tsx                       # bento: network
      terminal-demo.tsx                    # bento: terminal
      file-tree.tsx                        # bento: file manager
      docker-stack.tsx                     # bento: docker
      color-ring.tsx                       # bento: themes
      alert-bell.tsx                       # bento: alerts
      monitor-dots.tsx                     # bento: service monitors
      upgrade-loop.tsx                     # bento: auto-upgrade
      light-band.tsx                       # how-it-works arrow flow
  styles/landing.css                       # @keyframes + utility classes used by animations
```

`landing.css` is imported from `app.css` so it ships with every page but only the landing route uses the classes. Total CSS size budget: **≤ 8 KB min+gz** (see §12.4).

## 6. i18n Strategy

`translations.ts` exports a flat object per language, keyed by short dot-paths:

```ts
export const t = {
  en: {
    hero: { eyebrow: 'Open source · MIT', headline: 'Self-hosted VPS monitoring, in a single binary.', sub: '...', primaryCta: 'Quick start', secondaryCta: 'View on GitHub' },
    trust: { binary: 'Single binary · ~10MB', realtime: 'Sub-second updates', deps: 'Zero external deps' },
    pillars: { /* ... */ },
    bento: { network: { title: 'Network quality', body: '...' }, /* ... */ },
    how: { step1: '...', step2: '...', step3: '...' },
    finalCta: { /* ... */ }
  },
  cn: { /* mirror */ }
} as const
```

Components import `t` and read via `t[lang as 'en' | 'cn']`, with `en` as fallback. No new runtime, no extra deps.

Translation guidelines:
- Chinese copy is concise and uses the marketing register already present in `cn/index.mdx`.
- Technical nouns (`WebSocket`, `Docker`, `SSL`, `WHOIS`) stay in English in both languages.
- Avoid colon-prefixed dynamic substitution (no `i18next`-style interpolation needed).

## 7. Dark-mode Enforcement

The landing component renders:

```tsx
<div className="dark serverbee-landing" style={{ colorScheme: 'dark' }}>
  {/* sections */}
</div>
```

The `.serverbee-landing` selector scopes any landing-specific CSS resets so we never bleed into the Fumadocs docs reader. Fumadocs' theme switcher continues to work everywhere else; on this page it remains visible in the nav but flipping it has no visual effect (we accept this minor inconsistency rather than mutating the global toggle).

## 8. Responsiveness

Even though we are dark-only, layout must remain usable on mobile:

- Hero: stacks (text above, animation below). Mini dashboard scales to 100% width with a max-height clamp.
- Three pillars: 3-col → 1-col at `< md`.
- Bento grid: 12-col → 6-col at `md` → 1-col at `< sm`. Large tiles stay first.
- All animations respect `prefers-reduced-motion: reduce` by collapsing to a static end-state.

## 9. Accessibility

- All decorative SVGs use `aria-hidden="true"`.
- Each animated demo is wrapped in a region with a textual `aria-label` describing what it shows ("Animated demo of the ServerBee web terminal").
- Color contrast: all body text meets WCAG AA against the dark background; primary gradient text has a solid fallback color via `-webkit-text-fill-color: transparent` + `color:` for older browsers.
- Focus rings: every CTA and copy button has a visible focus ring (`focus-visible:ring-2 ring-amber-400`).
- `prefers-reduced-motion: reduce` pauses every keyframe animation (`animation-play-state: paused`) and shows a static representative frame.

## 10. Performance Budget

- No new npm dependencies.
- Inline SVGs total `≤ 15 KB` uncompressed.
- Landing route LCP target: **< 1.5s** on a fast 3G simulation (the page is fully SSR-rendered; animations begin after hydration).
- All animations run on `transform` / `opacity` only — no layout thrashing.

## 11. Out of Scope (Explicit)

- A `/install.sh` script. The install command is shown verbatim; the actual script publication is a separate workstream.
- A blog or changelog feed on the landing page.
- A "Featured users" / logos section. We have none to publish.
- Light-mode design. We may revisit later; for now `prefers-color-scheme: light` is intentionally ignored on this route.

## 12. Risks & Open Questions

1. **`HomeLayout` chrome height** — Fumadocs' nav reserves vertical space. We will measure during implementation and adjust hero padding accordingly.
2. **Install command source of truth** — the exact recommended install command lives in `quick-start.mdx`. During implementation we will lift it from there to keep one canonical source. If `quick-start.mdx` shows multiple options, we use the first/most-common one.
3. **`prefers-reduced-motion` coverage** — we must verify every animation in this spec has a sensible reduced-motion fallback before merging.
4. **CSS bundle growth** — `landing.css` is shared with the docs reader; we will use cascade-layer scoping (`@layer landing`) and `.serverbee-landing` scoping so docs pages don't pay for unused keyframes at runtime (the bytes still ship, but they don't execute). Total added bytes target: **≤ 8 KB min+gz**.

## 13. Acceptance Criteria

- Navigating to `/` redirects to `/en` (existing behavior preserved).
- `/en` and `/cn` render the new landing page with every section, animation, and translated string.
- Fumadocs nav remains functional (lang switch, GitHub link, search trigger if present).
- `bun run typecheck` and `bun x ultracite check` pass.
- The page is fully usable with JS disabled (text + static end-state of every animation visible).
- Lighthouse mobile score ≥ 90 for Performance and Accessibility on the landing route.
- `prefers-reduced-motion: reduce` users see no animation but no broken layout.
