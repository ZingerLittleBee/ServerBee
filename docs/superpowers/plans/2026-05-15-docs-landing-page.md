# Docs Landing Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the placeholder `apps/docs/src/routes/$lang/index.tsx` with a Next.js-style, dark-only, bilingual marketing landing page that showcases ServerBee's features via pure CSS / inline SVG animations — no new dependencies.

**Architecture:** Single page composed of focused section components under `apps/docs/src/components/landing/`. All animations are HTML/CSS keyframes + inline SVG. A scoped `.serverbee-landing` wrapper forces dark mode and isolates landing CSS from the docs reader. i18n strings live in a single `translations.ts` keyed by the existing `$lang` route param.

**Tech Stack:** TanStack Start + Fumadocs `HomeLayout` (already in place), React 19, Tailwind v4, `lucide-react` (already a dep), plain CSS keyframes.

**Spec:** `docs/superpowers/specs/2026-05-15-docs-landing-page-design.md`

**Testing approach:** UI work for a marketing page doesn't lend itself to unit tests. The verification loop in this plan is:
1. `bun run typecheck` after every code change (catches API mismatches).
2. `bun x ultracite check apps/docs` for lint.
3. Manual browser verification via `bun --filter @serverbee/docs dev` on `http://localhost:4000` at the end of each milestone.
4. A final `bun run build` (production build) to make sure SSR works.

Commits land at the end of each task. The branch is already isolated (`toronto`).

---

## File Structure

Files this plan creates:

```
apps/docs/src/
  routes/$lang/index.tsx                  # REWRITE (existed)
  styles/landing.css                      # NEW — keyframes, scoped utilities
  styles/app.css                          # MODIFY — import landing.css
  components/landing/
    index.tsx                              # NEW — composes all sections
    translations.ts                        # NEW — en/cn strings + install command constant
    primitives/
      section.tsx                          # NEW
      gradient-heading.tsx                 # NEW
      code-copy.tsx                        # NEW
      hex-background.tsx                   # NEW
    sections/
      hero.tsx                             # NEW
      trust-strip.tsx                      # NEW
      pillars.tsx                          # NEW
      bento.tsx                            # NEW
      how-it-works.tsx                     # NEW
      final-cta.tsx                        # NEW
    animations/
      mini-dashboard.tsx                   # NEW
      install-binary.tsx                   # NEW
      data-stream.tsx                      # NEW
      orbit-icons.tsx                      # NEW
      ping-chart.tsx                       # NEW
      terminal-demo.tsx                    # NEW
      file-tree.tsx                        # NEW
      docker-stack.tsx                     # NEW
      color-ring.tsx                       # NEW
      alert-bell.tsx                       # NEW
      monitor-dots.tsx                     # NEW
      upgrade-loop.tsx                     # NEW
      light-band.tsx                       # NEW
```

---

## Task 1: Foundation — translations, scoped CSS, and the landing entry point

**Files:**
- Create: `apps/docs/src/styles/landing.css`
- Modify: `apps/docs/src/styles/app.css`
- Create: `apps/docs/src/components/landing/translations.ts`
- Create: `apps/docs/src/components/landing/index.tsx`
- Modify: `apps/docs/src/routes/$lang/index.tsx`

- [ ] **Step 1.1 — Create `apps/docs/src/styles/landing.css`**

```css
/* Landing-only keyframes and scoped utilities.
   All rules nest under .serverbee-landing so they never affect the docs reader. */
@layer landing {
  .serverbee-landing {
    --landing-bg: oklch(0.16 0.01 260);
    --landing-bg-elevated: oklch(0.20 0.012 260);
    --landing-fg: oklch(0.96 0.005 260);
    --landing-fg-muted: oklch(0.72 0.01 260);
    --landing-border: oklch(0.28 0.015 260);
    --landing-amber: #ffb300;
    --landing-amber-soft: #ffd166;
    --landing-cyan: #4cc9f0;
    background: var(--landing-bg);
    color: var(--landing-fg);
  }

  .serverbee-landing .gradient-text {
    background: linear-gradient(120deg, #fff 0%, #ffd166 40%, #ffb300 70%, #ff8a3d 100%);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
    color: #ffd166;
  }

  .serverbee-landing .hex-bg {
    background-image:
      radial-gradient(circle at 20% 10%, rgba(255, 179, 0, 0.10), transparent 40%),
      radial-gradient(circle at 80% 30%, rgba(76, 201, 240, 0.08), transparent 40%);
  }

  @keyframes landing-pulse {
    0%, 100% { box-shadow: 0 0 0 0 rgba(34, 197, 94, 0.55); }
    70%      { box-shadow: 0 0 0 10px rgba(34, 197, 94, 0); }
  }
  .serverbee-landing .pulse-dot {
    animation: landing-pulse 2s ease-out infinite;
  }

  @keyframes landing-ring {
    0%   { stroke-dashoffset: 220; }
    50%  { stroke-dashoffset: 70; }
    100% { stroke-dashoffset: 220; }
  }
  .serverbee-landing .ring-anim { animation: landing-ring 4s ease-in-out infinite; }

  @keyframes landing-spark {
    0%   { transform: translateX(0); }
    100% { transform: translateX(-50%); }
  }
  .serverbee-landing .spark-scroll { animation: landing-spark 6s linear infinite; }

  @keyframes landing-hex-drift {
    0%   { transform: translateY(0); }
    100% { transform: translateY(-40px); }
  }
  .serverbee-landing .hex-drift { animation: landing-hex-drift 30s linear infinite; }

  @keyframes landing-orbit {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
  .serverbee-landing .orbit-anim { animation: landing-orbit 14s linear infinite; }
  .serverbee-landing .orbit-counter { animation: landing-orbit 14s linear infinite reverse; }

  @keyframes landing-stream {
    0%   { transform: translateX(-100%); opacity: 0; }
    10%  { opacity: 1; }
    90%  { opacity: 1; }
    100% { transform: translateX(100%); opacity: 0; }
  }
  .serverbee-landing .stream-particle { animation: landing-stream 3s linear infinite; }

  @keyframes landing-blink {
    0%, 60%, 100% { opacity: 1; }
    30%           { opacity: 0; }
  }
  .serverbee-landing .blink { animation: landing-blink 1s steps(1) infinite; }

  @keyframes landing-shake {
    0%, 92%, 100% { transform: rotate(0); }
    94%           { transform: rotate(-14deg); }
    96%           { transform: rotate(12deg); }
    98%           { transform: rotate(-6deg); }
  }
  .serverbee-landing .bell-shake { animation: landing-shake 5s ease-in-out infinite; }

  @keyframes landing-uprotate {
    from { transform: rotate(0); }
    to   { transform: rotate(360deg); }
  }
  .serverbee-landing .upgrade-spin { animation: landing-uprotate 6s linear infinite; }

  @keyframes landing-band {
    0%   { transform: translateX(-110%); }
    100% { transform: translateX(110%); }
  }
  .serverbee-landing .light-band { animation: landing-band 3.5s ease-in-out infinite; }

  @keyframes landing-fade-cycle {
    0%, 25%   { opacity: 1; }
    35%, 90%  { opacity: 0.25; }
    100%      { opacity: 1; }
  }
  .serverbee-landing .fade-cycle { animation: landing-fade-cycle 4s ease-in-out infinite; }

  @keyframes landing-typewriter {
    0%, 5%    { width: 0; }
    35%, 60%  { width: 100%; }
    95%, 100% { width: 0; }
  }
  .serverbee-landing .typewriter {
    display: inline-block;
    overflow: hidden;
    white-space: nowrap;
    animation: landing-typewriter 8s steps(40, end) infinite;
  }

  @media (prefers-reduced-motion: reduce) {
    .serverbee-landing *,
    .serverbee-landing *::before,
    .serverbee-landing *::after {
      animation-duration: 0.001ms !important;
      animation-iteration-count: 1 !important;
      transition-duration: 0.001ms !important;
    }
  }
}
```

- [ ] **Step 1.2 — Wire the new CSS into `apps/docs/src/styles/app.css`**

Replace the entire file with:

```css
@import "tailwindcss";
@import "fumadocs-ui/css/neutral.css";
@import "fumadocs-ui/css/preset.css";
@import "./landing.css";
```

- [ ] **Step 1.3 — Create `apps/docs/src/components/landing/translations.ts`**

```ts
export const INSTALL_COMMAND =
  'curl -fsSL https://raw.githubusercontent.com/ZingerLittleBee/ServerBee/main/deploy/install.sh | sudo bash -s -- server'

export type LandingLang = 'en' | 'cn'

export const translations = {
  en: {
    hero: {
      eyebrow: 'Open source · MIT · Built with Rust',
      headline1: 'Self-hosted VPS monitoring,',
      headline2: 'in a single binary.',
      sub: 'ServerBee is a lightweight probe that streams CPU, memory, disk, network, and Docker metrics to a Rust dashboard in real time — no agents to babysit, no external database, no bloat.',
      primaryCta: 'Quick start',
      secondaryCta: 'View on GitHub',
      installLabel: 'Install on Linux'
    },
    trust: {
      binary: 'Single binary · ~10 MB',
      realtime: 'Sub-second WebSocket updates',
      deps: 'Zero external dependencies'
    },
    pillars: {
      one: {
        title: 'Lightweight Rust probe',
        body: 'A statically linked binary you drop on any Linux host. No JVM, no Python, no daemons — just a tiny process that idles near zero CPU.'
      },
      two: {
        title: 'Realtime over WebSocket',
        body: 'Agents stream metrics and events to the server with sub-second latency. The browser dashboard subscribes to the same fan-out, so what you see is what is actually happening.'
      },
      three: {
        title: 'One agent, full control',
        body: 'Terminal sessions, file manager, Docker operations, and remote command execution all run through the same encrypted channel — gated by per-server capabilities.'
      }
    },
    bento: {
      network: {
        title: 'Network quality monitoring',
        body: 'ICMP / TCP / HTTP probes, traceroute, packet loss and latency charts, CSV export, and preset targets — all from your agents.'
      },
      terminal: {
        title: 'Browser web terminal',
        body: 'Full PTY sessions over WebSocket. Multi-tab, copy-paste friendly, and audit-logged.'
      },
      file: {
        title: 'File manager',
        body: 'Browse, read, edit, upload and download files through path-sandboxed agents.'
      },
      docker: {
        title: 'Docker management',
        body: 'Containers, stats, events, logs, networks, volumes — when the capability is enabled.'
      },
      themes: {
        title: 'Custom dashboards & themes',
        body: 'Compose dashboards from widgets. Bring your own OKLCH palette, logo, favicon, and footer.'
      },
      alerts: {
        title: 'Alerts & notifications',
        body: 'Thresholds, debounce, maintenance windows. Delivered through Webhook, Telegram, Bark, Email and APNs.'
      },
      monitors: {
        title: 'Service monitors',
        body: 'SSL, DNS, HTTP keyword, TCP and WHOIS checks with history and notifications.'
      },
      upgrade: {
        title: 'Automatic upgrades',
        body: 'Server and agents update themselves. One CLI command, zero downtime restarts.'
      }
    },
    how: {
      title: 'Three commands to a live dashboard',
      step1: { title: 'Install the server', body: 'Run the install script on one host. Systemd takes over from there.' },
      step2: { title: 'Bootstrap an agent', body: 'Drop the agent binary on every VPS you want to monitor. Pair it once.' },
      step3: { title: 'Open the dashboard', body: 'Sign in, watch metrics stream in, and start composing dashboards.' }
    },
    finalCta: {
      title: 'Ship a monitor in five minutes.',
      sub: 'Open source, MIT licensed, and small enough to forget about.',
      readDocs: 'Read the docs',
      star: 'Star on GitHub'
    }
  },
  cn: {
    hero: {
      eyebrow: '开源 · MIT · Rust 构建',
      headline1: '自托管的 VPS 监控，',
      headline2: '只需一个二进制。',
      sub: 'ServerBee 是一个轻量探针：实时把 CPU、内存、磁盘、网络、Docker 指标推送到 Rust 仪表盘，没有外部依赖、没有冗余服务、不用伺候它。',
      primaryCta: '快速开始',
      secondaryCta: '在 GitHub 查看',
      installLabel: 'Linux 一键安装'
    },
    trust: {
      binary: '单二进制 · 约 10 MB',
      realtime: '亚秒级 WebSocket 更新',
      deps: '零外部依赖'
    },
    pillars: {
      one: {
        title: 'Rust 编写的轻量探针',
        body: '静态链接的二进制文件，丢到任何 Linux 主机上就能跑。没有 JVM、没有 Python、没有守护脚本，空载几乎不占 CPU。'
      },
      two: {
        title: 'WebSocket 实时通信',
        body: 'Agent 把指标和事件以亚秒级延迟推到 Server。浏览器订阅同一份广播流，看到的就是当下真实发生的事。'
      },
      three: {
        title: '一个 Agent 全栈掌控',
        body: '终端会话、文件管理、Docker 操作、远程命令执行都走同一条加密通道，并由每台服务器的能力位精细授权。'
      }
    },
    bento: {
      network: {
        title: '网络质量监控',
        body: 'ICMP / TCP / HTTP 探测、traceroute、丢包与延迟图表、CSV 导出、预设目标 —— 全部由你的 Agent 完成。'
      },
      terminal: {
        title: '浏览器终端',
        body: '基于 WebSocket 的完整 PTY 会话，多标签、易复制粘贴、全程审计日志。'
      },
      file: {
        title: '文件管理',
        body: '通过路径沙箱化的 Agent 浏览、读取、编辑、上传和下载文件。'
      },
      docker: {
        title: 'Docker 管理',
        body: '在开启能力位后管理容器、统计、事件、日志、网络与卷。'
      },
      themes: {
        title: '自定义仪表盘与主题',
        body: '从组件拼装仪表盘，自带 OKLCH 调色板、Logo、favicon 和页脚配置。'
      },
      alerts: {
        title: '告警与通知',
        body: '阈值、抖动抑制、维护窗口。通过 Webhook、Telegram、Bark、Email 和 APNs 推送。'
      },
      monitors: {
        title: '服务监控',
        body: 'SSL、DNS、HTTP 关键字、TCP 与 WHOIS 检查，含历史与通知。'
      },
      upgrade: {
        title: '自动升级',
        body: 'Server 和 Agent 自动更新。一条 CLI 命令，重启零停顿。'
      }
    },
    how: {
      title: '三步上线一个实时仪表盘',
      step1: { title: '安装 Server', body: '在一台主机上跑安装脚本，剩下交给 systemd。' },
      step2: { title: '部署 Agent', body: '把 Agent 二进制丢到每台要监控的 VPS 上，配对一次即可。' },
      step3: { title: '打开仪表盘', body: '登录，等指标自动流入，然后开始拼装你的仪表盘。' }
    },
    finalCta: {
      title: '五分钟跑起一个监控。',
      sub: '开源、MIT 协议，小到你会忘了它在跑。',
      readDocs: '阅读文档',
      star: '在 GitHub 上 Star'
    }
  }
} as const

export type Translations = typeof translations
export function t(lang: string): Translations['en'] {
  return (translations as Record<string, Translations['en']>)[lang] ?? translations.en
}
```

- [ ] **Step 1.4 — Create `apps/docs/src/components/landing/index.tsx` as a minimal stub**

```tsx
import type { LandingLang } from './translations'
import { t } from './translations'

export function LandingPage({ lang }: { lang: LandingLang }) {
  const copy = t(lang)
  return (
    <div className="serverbee-landing dark min-h-screen" style={{ colorScheme: 'dark' }}>
      <main className="mx-auto w-full max-w-6xl px-6 py-24">
        <h1 className="gradient-text font-semibold text-5xl tracking-tight">{copy.hero.headline1}</h1>
        <p className="mt-4 text-zinc-400">{copy.hero.sub}</p>
      </main>
    </div>
  )
}
```

(The stub is real enough to verify wiring; subsequent tasks fill it out.)

- [ ] **Step 1.5 — Rewrite `apps/docs/src/routes/$lang/index.tsx`**

```tsx
import { createFileRoute, useParams } from '@tanstack/react-router'
import { HomeLayout } from 'fumadocs-ui/layouts/home'

import { LandingPage } from '@/components/landing'
import type { LandingLang } from '@/components/landing/translations'
import { baseOptions } from '@/lib/layout.shared'

export const Route = createFileRoute('/$lang/')({
  component: Home
})

function Home() {
  const { lang } = useParams({ from: '/$lang/' })
  const landingLang: LandingLang = lang === 'cn' ? 'cn' : 'en'

  return (
    <HomeLayout {...baseOptions(lang)}>
      <LandingPage lang={landingLang} />
    </HomeLayout>
  )
}
```

- [ ] **Step 1.6 — Verify typecheck + lint**

Run:
```bash
cd apps/docs && bun run types:check && bun x ultracite check src
```
Expected: both pass. Fix any errors before moving on.

- [ ] **Step 1.7 — Smoke test in browser**

Run from repo root:
```bash
bun --filter @serverbee/docs dev
```
Open `http://localhost:4000/en`. Expected: dark page, gradient headline visible, no console errors. Stop the dev server with `Ctrl+C`.

- [ ] **Step 1.8 — Commit**

```bash
git add apps/docs/src/styles/landing.css apps/docs/src/styles/app.css \
  apps/docs/src/components/landing/translations.ts \
  apps/docs/src/components/landing/index.tsx \
  apps/docs/src/routes/\$lang/index.tsx
git commit -m "feat(docs): scaffold landing page with i18n and scoped dark styles"
```

---

## Task 2: Primitives — section, gradient heading, code-copy, hex background

**Files:**
- Create: `apps/docs/src/components/landing/primitives/section.tsx`
- Create: `apps/docs/src/components/landing/primitives/gradient-heading.tsx`
- Create: `apps/docs/src/components/landing/primitives/code-copy.tsx`
- Create: `apps/docs/src/components/landing/primitives/hex-background.tsx`

- [ ] **Step 2.1 — `primitives/section.tsx`**

```tsx
import type { PropsWithChildren } from 'react'

import { cn } from '@/lib/cn'

export function Section({
  id,
  className,
  children
}: PropsWithChildren<{ id?: string; className?: string }>) {
  return (
    <section
      id={id}
      className={cn(
        'relative w-full border-b border-white/5 px-6 py-20 sm:py-24 lg:py-28',
        className
      )}
    >
      <div className="mx-auto w-full max-w-6xl">{children}</div>
    </section>
  )
}
```

- [ ] **Step 2.2 — `primitives/gradient-heading.tsx`**

```tsx
import type { PropsWithChildren } from 'react'

import { cn } from '@/lib/cn'

export function GradientHeading({
  as: Tag = 'h2',
  className,
  children
}: PropsWithChildren<{ as?: 'h1' | 'h2' | 'h3'; className?: string }>) {
  return (
    <Tag
      className={cn(
        'gradient-text font-semibold tracking-tight',
        Tag === 'h1' ? 'text-5xl leading-tight sm:text-6xl lg:text-7xl' : 'text-3xl sm:text-4xl',
        className
      )}
    >
      {children}
    </Tag>
  )
}
```

- [ ] **Step 2.3 — `primitives/code-copy.tsx`**

```tsx
import { Check, Copy } from 'lucide-react'
import { useState } from 'react'

import { cn } from '@/lib/cn'

export function CodeCopy({ command, label, className }: { command: string; label?: string; className?: string }) {
  const [copied, setCopied] = useState(false)

  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(command)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch {
      // Clipboard API unavailable (e.g. insecure context). Silently no-op; the
      // command is still visible and copy-selectable.
    }
  }

  return (
    <div
      className={cn(
        'group flex w-full max-w-3xl items-center gap-3 rounded-xl border border-white/10 bg-white/[0.04] px-4 py-3 font-mono text-sm shadow-[inset_0_1px_0_rgba(255,255,255,0.04)] backdrop-blur',
        className
      )}
    >
      {label ? (
        <span className="select-none rounded-md bg-amber-400/15 px-2 py-0.5 text-amber-300 text-xs">
          {label}
        </span>
      ) : null}
      <code className="flex-1 overflow-x-auto whitespace-nowrap text-zinc-200">
        <span className="text-amber-400">$ </span>
        {command}
      </code>
      <button
        type="button"
        aria-label="Copy install command"
        onClick={onCopy}
        className="rounded-md p-1.5 text-zinc-400 transition hover:bg-white/10 hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-400"
      >
        {copied ? <Check className="h-4 w-4 text-emerald-400" /> : <Copy className="h-4 w-4" />}
      </button>
    </div>
  )
}
```

- [ ] **Step 2.4 — `primitives/hex-background.tsx`**

```tsx
import { cn } from '@/lib/cn'

export function HexBackground({ className }: { className?: string }) {
  // A sparse, drifting hexagon pattern rendered inline so we ship no extra request.
  return (
    <div aria-hidden className={cn('pointer-events-none absolute inset-0 overflow-hidden', className)}>
      <div className="hex-bg absolute inset-0" />
      <svg
        className="hex-drift absolute inset-x-0 -top-10 h-[140%] w-full opacity-[0.18]"
        viewBox="0 0 800 800"
        xmlns="http://www.w3.org/2000/svg"
      >
        <defs>
          <pattern id="hex" width="40" height="46" patternUnits="userSpaceOnUse" patternTransform="translate(0 0)">
            <polygon
              points="20,2 38,12 38,34 20,44 2,34 2,12"
              fill="none"
              stroke="currentColor"
              strokeOpacity="0.35"
              strokeWidth="0.6"
            />
          </pattern>
          <linearGradient id="hex-fade" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="white" stopOpacity="0.18" />
            <stop offset="100%" stopColor="white" stopOpacity="0" />
          </linearGradient>
          <mask id="hex-mask">
            <rect width="100%" height="100%" fill="url(#hex-fade)" />
          </mask>
        </defs>
        <rect width="100%" height="100%" fill="url(#hex)" mask="url(#hex-mask)" className="text-amber-300" />
      </svg>
    </div>
  )
}
```

- [ ] **Step 2.5 — Verify typecheck**

```bash
cd apps/docs && bun run types:check
```
Expected: passes.

- [ ] **Step 2.6 — Commit**

```bash
git add apps/docs/src/components/landing/primitives
git commit -m "feat(docs): add landing primitives — section, gradient heading, code-copy, hex bg"
```

---

## Task 3: Hero section + mini-dashboard animation

**Files:**
- Create: `apps/docs/src/components/landing/animations/mini-dashboard.tsx`
- Create: `apps/docs/src/components/landing/sections/hero.tsx`
- Modify: `apps/docs/src/components/landing/index.tsx`

- [ ] **Step 3.1 — `animations/mini-dashboard.tsx`**

```tsx
// A self-contained "mini server card" that visually mimics ServerBee's real
// server card: two donut rings (CPU/Memory) animated via stroke-dashoffset,
// a sparkline whose clip-path scrolls leftward, and a pulsing status dot.
export function MiniDashboard() {
  return (
    <div
      aria-label="Animated demo of a ServerBee server card"
      className="relative w-full max-w-md rounded-2xl border border-white/10 bg-white/[0.03] p-5 shadow-2xl shadow-amber-500/5 backdrop-blur"
    >
      <header className="mb-4 flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="pulse-dot inline-block h-2.5 w-2.5 rounded-full bg-emerald-400" />
          <span className="font-medium text-sm text-zinc-100">edge-tokyo-01</span>
        </div>
        <span className="rounded-md bg-white/5 px-2 py-0.5 font-mono text-xs text-zinc-400">linux/arm64</span>
      </header>

      <div className="grid grid-cols-2 gap-4">
        <Ring label="CPU" value={42} color="#ffb300" />
        <Ring label="MEM" value={61} color="#4cc9f0" />
      </div>

      <div className="mt-5">
        <div className="mb-2 flex items-center justify-between text-xs text-zinc-400">
          <span>Network</span>
          <span className="font-mono text-zinc-300">↑ 2.1 MB/s · ↓ 318 KB/s</span>
        </div>
        <Sparkline />
      </div>

      <footer className="mt-4 grid grid-cols-3 gap-2 text-center text-xs">
        <Stat label="Load" value="0.42" />
        <Stat label="Disk" value="58%" />
        <Stat label="Uptime" value="14d" />
      </footer>
    </div>
  )
}

function Ring({ label, value, color }: { label: string; value: number; color: string }) {
  const dash = 220
  return (
    <div className="flex items-center gap-3 rounded-xl bg-white/[0.03] p-3">
      <svg width="60" height="60" viewBox="0 0 80 80" aria-hidden>
        <circle cx="40" cy="40" r="34" stroke="rgba(255,255,255,0.08)" strokeWidth="8" fill="none" />
        <circle
          cx="40"
          cy="40"
          r="34"
          stroke={color}
          strokeWidth="8"
          fill="none"
          strokeLinecap="round"
          strokeDasharray={dash}
          className="ring-anim"
          transform="rotate(-90 40 40)"
        />
      </svg>
      <div>
        <div className="font-mono text-xs text-zinc-400">{label}</div>
        <div className="font-semibold text-xl text-zinc-100">{value}%</div>
      </div>
    </div>
  )
}

function Sparkline() {
  return (
    <div className="relative h-14 w-full overflow-hidden rounded-md bg-white/[0.03]">
      <svg
        className="spark-scroll absolute inset-y-0 left-0 h-full w-[200%]"
        viewBox="0 0 400 56"
        preserveAspectRatio="none"
        aria-hidden
      >
        <defs>
          <linearGradient id="spark-fill" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="#ffb300" stopOpacity="0.45" />
            <stop offset="100%" stopColor="#ffb300" stopOpacity="0" />
          </linearGradient>
        </defs>
        <path
          d="M0 38 L20 30 L40 34 L60 22 L80 28 L100 18 L120 26 L140 14 L160 24 L180 16 L200 30 L220 22 L240 32 L260 18 L280 28 L300 20 L320 34 L340 24 L360 30 L380 22 L400 28 L400 56 L0 56 Z"
          fill="url(#spark-fill)"
        />
        <path
          d="M0 38 L20 30 L40 34 L60 22 L80 28 L100 18 L120 26 L140 14 L160 24 L180 16 L200 30 L220 22 L240 32 L260 18 L280 28 L300 20 L320 34 L340 24 L360 30 L380 22 L400 28"
          fill="none"
          stroke="#ffb300"
          strokeWidth="1.5"
        />
      </svg>
    </div>
  )
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg bg-white/[0.03] py-2">
      <div className="text-[10px] text-zinc-500 uppercase tracking-wider">{label}</div>
      <div className="font-mono text-sm text-zinc-200">{value}</div>
    </div>
  )
}
```

- [ ] **Step 3.2 — `sections/hero.tsx`**

```tsx
import { ArrowRight, Github } from 'lucide-react'

import { MiniDashboard } from '../animations/mini-dashboard'
import { CodeCopy } from '../primitives/code-copy'
import { GradientHeading } from '../primitives/gradient-heading'
import { HexBackground } from '../primitives/hex-background'
import { Section } from '../primitives/section'
import { INSTALL_COMMAND, type LandingLang, t } from '../translations'

export function Hero({ lang }: { lang: LandingLang }) {
  const copy = t(lang).hero
  const docsHref = `/${lang}/docs/quick-start`
  return (
    <Section className="overflow-hidden pt-28">
      <HexBackground />
      <div className="relative grid items-center gap-12 lg:grid-cols-[1.1fr_1fr]">
        <div>
          <span className="inline-flex items-center rounded-full border border-amber-400/30 bg-amber-400/10 px-3 py-1 text-amber-300 text-xs">
            {copy.eyebrow}
          </span>
          <GradientHeading as="h1" className="mt-5">
            {copy.headline1}
            <br />
            {copy.headline2}
          </GradientHeading>
          <p className="mt-6 max-w-xl text-base text-zinc-400 leading-relaxed sm:text-lg">{copy.sub}</p>

          <div className="mt-8 flex flex-wrap items-center gap-3">
            <a
              href={docsHref}
              className="inline-flex items-center gap-2 rounded-lg bg-amber-400 px-5 py-2.5 font-medium text-amber-950 text-sm transition hover:bg-amber-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-300"
            >
              {copy.primaryCta} <ArrowRight className="h-4 w-4" />
            </a>
            <a
              href="https://github.com/ZingerLittleBee/ServerBee"
              target="_blank"
              rel="noreferrer"
              className="inline-flex items-center gap-2 rounded-lg border border-white/15 bg-white/[0.04] px-5 py-2.5 font-medium text-sm text-zinc-100 transition hover:bg-white/[0.08] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/40"
            >
              <Github className="h-4 w-4" /> {copy.secondaryCta}
            </a>
          </div>

          <div className="mt-6">
            <CodeCopy command={INSTALL_COMMAND} label={copy.installLabel} />
          </div>
        </div>

        <div className="relative flex justify-center lg:justify-end">
          <MiniDashboard />
        </div>
      </div>
    </Section>
  )
}
```

- [ ] **Step 3.3 — Update `components/landing/index.tsx` to mount Hero**

```tsx
import { Hero } from './sections/hero'
import type { LandingLang } from './translations'

export function LandingPage({ lang }: { lang: LandingLang }) {
  return (
    <div className="serverbee-landing dark" style={{ colorScheme: 'dark' }}>
      <Hero lang={lang} />
    </div>
  )
}
```

- [ ] **Step 3.4 — Typecheck + lint + browser smoke**

```bash
cd apps/docs && bun run types:check && bun x ultracite check src
```
Then run the dev server and visit `/en` and `/cn`. Expected: hero renders, install command pill is visible and copy button works, dashboard animations loop, no console errors.

- [ ] **Step 3.5 — Commit**

```bash
git add apps/docs/src/components/landing
git commit -m "feat(docs): add landing hero with mini-dashboard animation"
```

---

## Task 4: Trust strip + three pillars (with install-binary, data-stream, orbit-icons animations)

**Files:**
- Create: `apps/docs/src/components/landing/sections/trust-strip.tsx`
- Create: `apps/docs/src/components/landing/animations/install-binary.tsx`
- Create: `apps/docs/src/components/landing/animations/data-stream.tsx`
- Create: `apps/docs/src/components/landing/animations/orbit-icons.tsx`
- Create: `apps/docs/src/components/landing/sections/pillars.tsx`
- Modify: `apps/docs/src/components/landing/index.tsx`

- [ ] **Step 4.1 — `sections/trust-strip.tsx`**

```tsx
import { Cpu, Gauge, PackageCheck } from 'lucide-react'

import { Section } from '../primitives/section'
import { type LandingLang, t } from '../translations'

export function TrustStrip({ lang }: { lang: LandingLang }) {
  const copy = t(lang).trust
  const items = [
    { Icon: PackageCheck, label: copy.binary },
    { Icon: Gauge, label: copy.realtime },
    { Icon: Cpu, label: copy.deps }
  ]
  return (
    <Section className="!py-10 border-y border-white/5 bg-white/[0.015]">
      <ul className="grid grid-cols-1 gap-4 sm:grid-cols-3">
        {items.map(({ Icon, label }) => (
          <li
            key={label}
            className="flex items-center gap-3 rounded-xl border border-white/5 bg-white/[0.02] px-4 py-3"
          >
            <Icon className="h-5 w-5 text-amber-400" aria-hidden />
            <span className="text-sm text-zinc-300">{label}</span>
          </li>
        ))}
      </ul>
    </Section>
  )
}
```

- [ ] **Step 4.2 — `animations/install-binary.tsx`**

```tsx
// Visual metaphor: a "binary file" tile slides up, expands, and a status dot
// lights green. Loops every ~6s via staggered CSS animations.
export function InstallBinaryAnim() {
  return (
    <div aria-label="Animated demo of installing the ServerBee binary" className="flex h-40 items-center justify-center">
      <div className="relative flex flex-col items-center gap-3">
        <div className="rounded-lg border border-amber-400/40 bg-amber-400/10 px-4 py-2 font-mono text-amber-300 text-xs shadow-[0_0_30px_-12px_rgba(255,179,0,0.6)]">
          serverbee
        </div>
        <svg width="20" height="32" viewBox="0 0 20 32" aria-hidden>
          <path d="M10 2 L10 24 M4 18 L10 26 L16 18" stroke="#ffb300" strokeWidth="2" fill="none" strokeLinecap="round" />
        </svg>
        <div className="flex items-center gap-2 rounded-md border border-white/10 bg-white/[0.04] px-3 py-2 font-mono text-xs text-zinc-300">
          <span className="pulse-dot inline-block h-2 w-2 rounded-full bg-emerald-400" />
          systemd · active
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 4.3 — `animations/data-stream.tsx`**

```tsx
// Two endpoints (Server, Agent) with particles travelling both ways.
export function DataStreamAnim() {
  return (
    <div aria-label="Animated demo of real-time WebSocket streaming" className="relative flex h-40 items-center justify-between px-6">
      <Endpoint label="Server" color="#ffb300" />
      <div className="relative mx-3 h-px flex-1 bg-gradient-to-r from-amber-400/30 via-cyan-300/30 to-amber-400/30">
        <Particle delay="0s" colorClass="bg-amber-300" />
        <Particle delay="0.6s" colorClass="bg-cyan-300" reverse />
        <Particle delay="1.2s" colorClass="bg-amber-300" />
        <Particle delay="1.8s" colorClass="bg-cyan-300" reverse />
      </div>
      <Endpoint label="Agent" color="#4cc9f0" />
    </div>
  )
}

function Endpoint({ label, color }: { label: string; color: string }) {
  return (
    <div className="flex flex-col items-center gap-1">
      <div
        className="h-10 w-10 rounded-lg border border-white/10 bg-white/[0.04]"
        style={{ boxShadow: `0 0 24px -6px ${color}` }}
      />
      <span className="font-mono text-[10px] text-zinc-400 uppercase tracking-wider">{label}</span>
    </div>
  )
}

function Particle({ delay, colorClass, reverse }: { delay: string; colorClass: string; reverse?: boolean }) {
  return (
    <span
      aria-hidden
      className={`stream-particle absolute top-1/2 h-1.5 w-3 -translate-y-1/2 rounded-full ${colorClass}`}
      style={{ animationDelay: delay, animationDirection: reverse ? 'reverse' : 'normal' }}
    />
  )
}
```

- [ ] **Step 4.4 — `animations/orbit-icons.tsx`**

```tsx
import { FileCog, Layers, TerminalSquare } from 'lucide-react'

export function OrbitIconsAnim() {
  return (
    <div aria-label="Animated demo of terminal, file manager, and Docker orbiting the agent" className="relative flex h-40 items-center justify-center">
      <div className="relative h-32 w-32 rounded-full border border-white/10">
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="h-9 w-9 rounded-lg bg-amber-400/15 ring-1 ring-amber-400/40" />
        </div>
        <div className="orbit-anim absolute inset-0">
          <OrbitItem angle={0} icon={<TerminalSquare className="h-4 w-4" />} />
          <OrbitItem angle={120} icon={<FileCog className="h-4 w-4" />} />
          <OrbitItem angle={240} icon={<Layers className="h-4 w-4" />} />
        </div>
      </div>
    </div>
  )
}

function OrbitItem({ angle, icon }: { angle: number; icon: React.ReactNode }) {
  return (
    <div
      className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2"
      style={{ transform: `translate(-50%, -50%) rotate(${angle}deg) translate(60px) rotate(-${angle}deg)` }}
    >
      <div className="orbit-counter flex h-8 w-8 items-center justify-center rounded-md border border-white/10 bg-white/[0.05] text-amber-300">
        {icon}
      </div>
    </div>
  )
}
```

(Note: `.orbit-counter` re-applies the same animation in reverse on each icon so the icon itself doesn't appear to spin while the orbit ring rotates.)

- [ ] **Step 4.5 — `sections/pillars.tsx`**

```tsx
import { DataStreamAnim } from '../animations/data-stream'
import { InstallBinaryAnim } from '../animations/install-binary'
import { OrbitIconsAnim } from '../animations/orbit-icons'
import { Section } from '../primitives/section'
import { type LandingLang, t } from '../translations'

export function Pillars({ lang }: { lang: LandingLang }) {
  const copy = t(lang).pillars
  const cards = [
    { ...copy.one, Anim: InstallBinaryAnim },
    { ...copy.two, Anim: DataStreamAnim },
    { ...copy.three, Anim: OrbitIconsAnim }
  ]
  return (
    <Section>
      <div className="grid gap-6 md:grid-cols-3">
        {cards.map(({ title, body, Anim }) => (
          <article
            key={title}
            className="group relative overflow-hidden rounded-2xl border border-white/10 bg-white/[0.02] p-6 transition hover:border-amber-400/30 hover:bg-white/[0.04]"
          >
            <Anim />
            <h3 className="mt-2 font-semibold text-lg text-zinc-100">{title}</h3>
            <p className="mt-2 text-sm text-zinc-400 leading-relaxed">{body}</p>
          </article>
        ))}
      </div>
    </Section>
  )
}
```

- [ ] **Step 4.6 — Update `components/landing/index.tsx`**

```tsx
import { Hero } from './sections/hero'
import { Pillars } from './sections/pillars'
import { TrustStrip } from './sections/trust-strip'
import type { LandingLang } from './translations'

export function LandingPage({ lang }: { lang: LandingLang }) {
  return (
    <div className="serverbee-landing dark" style={{ colorScheme: 'dark' }}>
      <Hero lang={lang} />
      <TrustStrip lang={lang} />
      <Pillars lang={lang} />
    </div>
  )
}
```

- [ ] **Step 4.7 — Typecheck + lint + browser smoke**

```bash
cd apps/docs && bun run types:check && bun x ultracite check src
```
Browser: scroll past hero, verify the 3 cards render and animate.

- [ ] **Step 4.8 — Commit**

```bash
git add apps/docs/src/components/landing
git commit -m "feat(docs): add trust strip and three-pillar section with animations"
```

---

## Task 5: Bento grid scaffolding + 8 animation components

This task creates all 8 micro-animations and the bento layout. Animations are intentionally small (≤ 40 lines each).

**Files:**
- Create: `apps/docs/src/components/landing/animations/ping-chart.tsx`
- Create: `apps/docs/src/components/landing/animations/terminal-demo.tsx`
- Create: `apps/docs/src/components/landing/animations/file-tree.tsx`
- Create: `apps/docs/src/components/landing/animations/docker-stack.tsx`
- Create: `apps/docs/src/components/landing/animations/color-ring.tsx`
- Create: `apps/docs/src/components/landing/animations/alert-bell.tsx`
- Create: `apps/docs/src/components/landing/animations/monitor-dots.tsx`
- Create: `apps/docs/src/components/landing/animations/upgrade-loop.tsx`
- Create: `apps/docs/src/components/landing/sections/bento.tsx`
- Modify: `apps/docs/src/components/landing/index.tsx`

- [ ] **Step 5.1 — `animations/ping-chart.tsx`**

```tsx
export function PingChartAnim() {
  return (
    <div aria-label="Animated demo of network latency monitoring" className="flex h-full flex-col gap-3">
      <div className="relative h-32 w-full overflow-hidden rounded-lg bg-black/30">
        <svg className="spark-scroll absolute inset-0 h-full w-[200%]" viewBox="0 0 400 100" preserveAspectRatio="none" aria-hidden>
          <path
            d="M0 60 L25 50 L50 64 L75 38 L100 56 L125 30 L150 44 L175 24 L200 56 L225 42 L250 70 L275 32 L300 52 L325 26 L350 60 L375 38 L400 50"
            fill="none"
            stroke="#4cc9f0"
            strokeWidth="1.8"
          />
        </svg>
        <div className="pointer-events-none absolute inset-0 bg-gradient-to-r from-black/30 via-transparent to-black/30" />
      </div>
      <div className="grid grid-cols-6 gap-1.5">
        {Array.from({ length: 18 }).map((_, i) => (
          <span
            key={i}
            className="h-2 rounded-sm fade-cycle"
            style={{
              animationDelay: `${(i % 6) * 0.25}s`,
              background: i % 7 === 5 ? '#f59e0b' : '#22c55e'
            }}
          />
        ))}
      </div>
    </div>
  )
}
```

- [ ] **Step 5.2 — `animations/terminal-demo.tsx`**

```tsx
export function TerminalDemoAnim() {
  return (
    <div aria-label="Animated demo of the web terminal" className="overflow-hidden rounded-lg border border-white/10 bg-black/40">
      <header className="flex items-center gap-1.5 border-white/5 border-b px-3 py-2">
        <span className="h-2.5 w-2.5 rounded-full bg-red-400/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-amber-400/70" />
        <span className="h-2.5 w-2.5 rounded-full bg-emerald-400/70" />
        <span className="ml-2 font-mono text-[10px] text-zinc-500">edge-tokyo-01 ~ #</span>
      </header>
      <pre className="px-3 py-3 font-mono text-xs leading-relaxed text-zinc-300">
{`$ `}
        <span className="typewriter text-amber-300">serverbee agent --version</span>
        {`
serverbee-agent 0.3.0 (linux/arm64)
features: terminal,file,docker,ping,upgrade
$ `}<span className="blink text-zinc-500">▍</span>
      </pre>
    </div>
  )
}
```

- [ ] **Step 5.3 — `animations/file-tree.tsx`**

```tsx
import { File, FolderOpen, FolderTree } from 'lucide-react'

export function FileTreeAnim() {
  return (
    <div aria-label="Animated demo of the file manager" className="space-y-2 font-mono text-xs">
      <div className="space-y-1 rounded-lg border border-white/10 bg-black/30 p-3 text-zinc-300">
        <Row Icon={FolderTree} label="/var/log" />
        <Row Icon={FolderOpen} label="  ⤷ nginx" indent />
        <Row Icon={File} label="     access.log" indent />
        <Row Icon={File} label="     error.log" indent />
      </div>
      <div className="relative h-2 w-full overflow-hidden rounded-full bg-white/5">
        <span className="absolute inset-y-0 left-0 w-1/2 bg-gradient-to-r from-amber-400 to-amber-200 light-band" />
      </div>
    </div>
  )
}

function Row({ Icon, label, indent }: { Icon: React.ComponentType<{ className?: string }>; label: string; indent?: boolean }) {
  return (
    <div className={`flex items-center gap-2 ${indent ? 'pl-3' : ''}`}>
      <Icon className="h-3.5 w-3.5 text-amber-300" />
      <span className="text-zinc-300">{label}</span>
    </div>
  )
}
```

- [ ] **Step 5.4 — `animations/docker-stack.tsx`**

```tsx
import { Box } from 'lucide-react'

export function DockerStackAnim() {
  const containers = [
    { name: 'web', tag: 'caddy:2', delay: '0s' },
    { name: 'api', tag: 'rust:1.84', delay: '0.4s' },
    { name: 'cache', tag: 'redis:7', delay: '0.8s' }
  ]
  return (
    <div aria-label="Animated demo of Docker container management" className="space-y-2">
      {containers.map((c) => (
        <div
          key={c.name}
          className="flex items-center gap-3 rounded-lg border border-white/10 bg-white/[0.04] px-3 py-2 font-mono text-xs"
        >
          <Box className="h-4 w-4 text-cyan-300" />
          <span className="text-zinc-200">{c.name}</span>
          <span className="text-zinc-500">{c.tag}</span>
          <span className="ml-auto inline-flex items-center gap-1.5 text-emerald-300">
            <span className="pulse-dot inline-block h-1.5 w-1.5 rounded-full bg-emerald-400" style={{ animationDelay: c.delay }} />
            running
          </span>
        </div>
      ))}
    </div>
  )
}
```

- [ ] **Step 5.5 — `animations/color-ring.tsx`**

```tsx
export function ColorRingAnim() {
  const stops = ['#ffb300', '#4cc9f0', '#22c55e', '#a855f7', '#ef4444', '#ffb300']
  const gradient = stops.map((c, i) => `${c} ${(i / (stops.length - 1)) * 360}deg`).join(', ')
  return (
    <div aria-label="Animated demo of theme customization" className="flex h-full items-center justify-center">
      <div className="orbit-anim h-24 w-24 rounded-full" style={{ background: `conic-gradient(${gradient})` }}>
        <div className="m-2 h-20 w-20 rounded-full bg-zinc-950">
          <div className="m-2 h-16 w-16 rounded-full bg-amber-400 fade-cycle" />
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 5.6 — `animations/alert-bell.tsx`**

```tsx
import { Bell } from 'lucide-react'

export function AlertBellAnim() {
  const channels = ['Webhook', 'Telegram', 'Bark', 'Email', 'APNs']
  return (
    <div aria-label="Animated demo of multi-channel alerts" className="flex h-full flex-col items-center justify-center gap-3">
      <Bell className="bell-shake h-10 w-10 text-amber-300" />
      <div className="flex flex-wrap justify-center gap-1.5">
        {channels.map((c, i) => (
          <span
            key={c}
            className="fade-cycle rounded-full border border-white/10 bg-white/[0.04] px-2 py-0.5 font-mono text-[10px] text-zinc-300"
            style={{ animationDelay: `${i * 0.3}s` }}
          >
            {c}
          </span>
        ))}
      </div>
    </div>
  )
}
```

- [ ] **Step 5.7 — `animations/monitor-dots.tsx`**

```tsx
export function MonitorDotsAnim() {
  const probes = ['SSL', 'DNS', 'HTTP', 'TCP', 'WHOIS']
  return (
    <div aria-label="Animated demo of service monitors" className="flex h-full flex-col justify-center gap-2 font-mono text-xs">
      {probes.map((p, i) => (
        <div key={p} className="flex items-center justify-between rounded-md bg-white/[0.03] px-3 py-1.5">
          <span className="text-zinc-300">{p}</span>
          <span
            className="pulse-dot inline-block h-2 w-2 rounded-full bg-emerald-400"
            style={{ animationDelay: `${i * 0.35}s` }}
          />
        </div>
      ))}
    </div>
  )
}
```

- [ ] **Step 5.8 — `animations/upgrade-loop.tsx`**

```tsx
import { RotateCw } from 'lucide-react'

export function UpgradeLoopAnim() {
  return (
    <div aria-label="Animated demo of auto-upgrade" className="flex h-full flex-col items-center justify-center gap-3">
      <RotateCw className="upgrade-spin h-9 w-9 text-amber-300" />
      <div className="flex items-center gap-2 font-mono text-xs">
        <span className="text-zinc-500 line-through">v0.2.9</span>
        <span className="text-amber-300">→</span>
        <span className="text-emerald-300">v0.3.0</span>
      </div>
    </div>
  )
}
```

- [ ] **Step 5.9 — `sections/bento.tsx`**

The grid uses Tailwind's 12-col grid; tiles span columns and rows by responsibility.

```tsx
import type { ComponentType, ReactNode } from 'react'

import { AlertBellAnim } from '../animations/alert-bell'
import { ColorRingAnim } from '../animations/color-ring'
import { DockerStackAnim } from '../animations/docker-stack'
import { FileTreeAnim } from '../animations/file-tree'
import { MonitorDotsAnim } from '../animations/monitor-dots'
import { PingChartAnim } from '../animations/ping-chart'
import { TerminalDemoAnim } from '../animations/terminal-demo'
import { UpgradeLoopAnim } from '../animations/upgrade-loop'
import { GradientHeading } from '../primitives/gradient-heading'
import { Section } from '../primitives/section'
import { type LandingLang, t } from '../translations'

type Tile = {
  title: string
  body: string
  Anim: ComponentType
  span: string
}

export function Bento({ lang }: { lang: LandingLang }) {
  const copy = t(lang).bento
  const tiles: Tile[] = [
    { ...copy.network,  Anim: PingChartAnim,    span: 'md:col-span-6 md:row-span-2' },
    { ...copy.themes,   Anim: ColorRingAnim,    span: 'md:col-span-3' },
    { ...copy.alerts,   Anim: AlertBellAnim,    span: 'md:col-span-3' },
    { ...copy.monitors, Anim: MonitorDotsAnim,  span: 'md:col-span-6' },
    { ...copy.terminal, Anim: TerminalDemoAnim, span: 'md:col-span-6 md:row-span-2' },
    { ...copy.file,     Anim: FileTreeAnim,     span: 'md:col-span-6' },
    { ...copy.docker,   Anim: DockerStackAnim,  span: 'md:col-span-3' },
    { ...copy.upgrade,  Anim: UpgradeLoopAnim,  span: 'md:col-span-3' }
  ]

  return (
    <Section>
      <GradientHeading className="mb-10 max-w-2xl">{title(lang)}</GradientHeading>
      <div className="grid auto-rows-[200px] grid-cols-1 gap-4 md:grid-cols-12">
        {tiles.map(({ title: heading, body, Anim, span }) => (
          <Card key={heading} title={heading} body={body} span={span}>
            <Anim />
          </Card>
        ))}
      </div>
    </Section>
  )
}

function Card({ title, body, span, children }: { title: string; body: string; span: string; children: ReactNode }) {
  return (
    <article
      className={`group flex flex-col gap-4 overflow-hidden rounded-2xl border border-white/10 bg-white/[0.02] p-5 transition hover:border-amber-400/30 hover:bg-white/[0.04] ${span}`}
    >
      <div className="min-h-[120px] flex-1">{children}</div>
      <div>
        <h3 className="font-semibold text-base text-zinc-100">{title}</h3>
        <p className="mt-1 text-sm text-zinc-400 leading-relaxed">{body}</p>
      </div>
    </article>
  )
}

function title(lang: LandingLang): string {
  return lang === 'cn' ? '一个探针，覆盖运维的方方面面。' : 'One probe. Every job your VPS needs.'
}
```

- [ ] **Step 5.10 — Update `components/landing/index.tsx`**

```tsx
import { Bento } from './sections/bento'
import { Hero } from './sections/hero'
import { Pillars } from './sections/pillars'
import { TrustStrip } from './sections/trust-strip'
import type { LandingLang } from './translations'

export function LandingPage({ lang }: { lang: LandingLang }) {
  return (
    <div className="serverbee-landing dark" style={{ colorScheme: 'dark' }}>
      <Hero lang={lang} />
      <TrustStrip lang={lang} />
      <Pillars lang={lang} />
      <Bento lang={lang} />
    </div>
  )
}
```

- [ ] **Step 5.11 — Typecheck + lint + browser smoke**

```bash
cd apps/docs && bun run types:check && bun x ultracite check src
```
Browser: verify all 8 tiles, no overlap on `md` and `lg`, each animation is visible.

- [ ] **Step 5.12 — Commit**

```bash
git add apps/docs/src/components/landing
git commit -m "feat(docs): add 8-tile bento grid with feature animations"
```

---

## Task 6: How-it-works + Final CTA

**Files:**
- Create: `apps/docs/src/components/landing/animations/light-band.tsx`
- Create: `apps/docs/src/components/landing/sections/how-it-works.tsx`
- Create: `apps/docs/src/components/landing/sections/final-cta.tsx`
- Modify: `apps/docs/src/components/landing/index.tsx`

- [ ] **Step 6.1 — `animations/light-band.tsx`**

```tsx
export function LightBandArrow() {
  return (
    <div aria-hidden className="relative mx-2 hidden h-px flex-1 bg-white/10 md:block">
      <span className="absolute inset-y-0 left-0 w-1/3 bg-gradient-to-r from-transparent via-amber-300 to-transparent light-band" />
    </div>
  )
}
```

- [ ] **Step 6.2 — `sections/how-it-works.tsx`**

```tsx
import { LightBandArrow } from '../animations/light-band'
import { GradientHeading } from '../primitives/gradient-heading'
import { Section } from '../primitives/section'
import { type LandingLang, t } from '../translations'

export function HowItWorks({ lang }: { lang: LandingLang }) {
  const copy = t(lang).how
  const steps = [copy.step1, copy.step2, copy.step3]
  return (
    <Section>
      <GradientHeading className="mb-12 max-w-2xl">{copy.title}</GradientHeading>
      <div className="flex flex-col items-stretch gap-6 md:flex-row md:items-center">
        {steps.map((s, i) => (
          <div key={s.title} className="flex flex-1 items-center gap-4">
            <article className="flex-1 rounded-2xl border border-white/10 bg-white/[0.02] p-6">
              <div className="font-mono text-amber-300 text-xs">{`0${i + 1}`}</div>
              <h3 className="mt-2 font-semibold text-lg text-zinc-100">{s.title}</h3>
              <p className="mt-2 text-sm text-zinc-400 leading-relaxed">{s.body}</p>
            </article>
            {i < steps.length - 1 ? <LightBandArrow /> : null}
          </div>
        ))}
      </div>
    </Section>
  )
}
```

- [ ] **Step 6.3 — `sections/final-cta.tsx`**

```tsx
import { ArrowRight, Github } from 'lucide-react'

import { CodeCopy } from '../primitives/code-copy'
import { GradientHeading } from '../primitives/gradient-heading'
import { HexBackground } from '../primitives/hex-background'
import { Section } from '../primitives/section'
import { INSTALL_COMMAND, type LandingLang, t } from '../translations'

export function FinalCta({ lang }: { lang: LandingLang }) {
  const copy = t(lang).finalCta
  const docsHref = `/${lang}/docs/quick-start`
  return (
    <Section className="overflow-hidden">
      <HexBackground />
      <div className="relative mx-auto max-w-3xl text-center">
        <GradientHeading className="mx-auto">{copy.title}</GradientHeading>
        <p className="mx-auto mt-4 max-w-xl text-base text-zinc-400">{copy.sub}</p>
        <div className="mt-8 flex flex-wrap items-center justify-center gap-3">
          <a
            href={docsHref}
            className="inline-flex items-center gap-2 rounded-lg bg-amber-400 px-5 py-2.5 font-medium text-amber-950 text-sm transition hover:bg-amber-300 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-amber-300"
          >
            {copy.readDocs} <ArrowRight className="h-4 w-4" />
          </a>
          <a
            href="https://github.com/ZingerLittleBee/ServerBee"
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-2 rounded-lg border border-white/15 bg-white/[0.04] px-5 py-2.5 font-medium text-sm text-zinc-100 transition hover:bg-white/[0.08] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-white/40"
          >
            <Github className="h-4 w-4" /> {copy.star}
          </a>
        </div>
        <div className="mt-8 flex justify-center">
          <CodeCopy command={INSTALL_COMMAND} />
        </div>
      </div>
    </Section>
  )
}
```

- [ ] **Step 6.4 — Update `components/landing/index.tsx`**

```tsx
import { Bento } from './sections/bento'
import { FinalCta } from './sections/final-cta'
import { Hero } from './sections/hero'
import { HowItWorks } from './sections/how-it-works'
import { Pillars } from './sections/pillars'
import { TrustStrip } from './sections/trust-strip'
import type { LandingLang } from './translations'

export function LandingPage({ lang }: { lang: LandingLang }) {
  return (
    <div className="serverbee-landing dark" style={{ colorScheme: 'dark' }}>
      <Hero lang={lang} />
      <TrustStrip lang={lang} />
      <Pillars lang={lang} />
      <Bento lang={lang} />
      <HowItWorks lang={lang} />
      <FinalCta lang={lang} />
    </div>
  )
}
```

- [ ] **Step 6.5 — Typecheck + lint + browser smoke**

```bash
cd apps/docs && bun run types:check && bun x ultracite check src
```
Browser: visit `/en` and `/cn`, scroll the whole page, confirm all 6 sections render, animations loop, links work.

- [ ] **Step 6.6 — Commit**

```bash
git add apps/docs/src/components/landing
git commit -m "feat(docs): add how-it-works and final CTA sections"
```

---

## Task 7: Final QA — production build, reduced motion, accessibility

**Files:** none (verification only, plus any small fixes that surface).

- [ ] **Step 7.1 — Production build**

```bash
cd apps/docs && bun run build
```
Expected: builds successfully, no SSR errors. If `MiniDashboard` or any animation uses something hydration-incompatible (e.g. `Math.random()` in JSX), fix it in this step before continuing.

- [ ] **Step 7.2 — Reduced-motion verification**

In macOS System Settings → Accessibility → Display → enable "Reduce motion", then refresh `/en`. Expected: animations freeze on a static end-state and the page remains legible. If a tile collapses to an empty box (e.g. typewriter at `width: 0`), set its base width to `100%` in `landing.css` and revert via `@media (prefers-reduced-motion: no-preference)` only when motion is allowed. Disable Reduce motion when done.

- [ ] **Step 7.3 — Keyboard / focus pass**

Tab through the page. Expected: CTAs, copy buttons, and language switcher all show a visible amber focus ring. Fix any missing focus-visible state.

- [ ] **Step 7.4 — Bilingual pass**

Visit `/cn`. Expected: every translated string appears in Chinese; English-only technical nouns (Docker, WebSocket, SSL, etc.) remain English; no untranslated `t.xxx` fallback strings.

- [ ] **Step 7.5 — Final commit (if any fixes landed)**

```bash
git add -A && git commit -m "fix(docs): polish landing page accessibility and reduced-motion behavior"
```

If no fixes were needed, skip this step.

- [ ] **Step 7.6 — Done**

Run a final summary check:
```bash
git log --oneline -10
```
Expected: 6–7 landing-page commits stacked on top of `d4d30da`.

---

## Self-Review Notes

- **Spec coverage:** Every section in the spec (§3.1–§3.3, §4–§12) maps to a task above. Hero (Task 3), trust strip + pillars (Task 4), bento grid (Task 5), how-it-works + final CTA (Task 6), QA pass for accessibility/reduced motion/SSR (Task 7).
- **Placeholder scan:** No `TBD`/`TODO`/"implement later" entries. The install command is a real string sourced from `apps/docs/content/docs/en/quick-start.mdx`.
- **Type consistency:** `LandingLang` is defined once (Task 1.3) and reused. `t(lang)` always returns `Translations['en']` so consumers get the same shape regardless of language. All animation components are zero-prop (`() => JSX`) and used as `ComponentType` in `bento.tsx`.
- **Risk noted in spec §12.4 (CSS bundle growth):** `landing.css` is wrapped in `@layer landing` so its specificity stays predictable; everything is scoped under `.serverbee-landing` so docs reader pages don't inherit any of these styles.
