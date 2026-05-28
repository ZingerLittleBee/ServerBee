# Custom Widget System — Plan 1: SDK + Registry + Asset Serving

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the foundation for the custom widget system: a stable `@serverbee/widget-sdk` package, a frontend Widget Registry + Loader, runtime shims that share the host's React/SDK singletons, and a backend `widget_module` table + endpoints that can list and serve any module's bundle and assets. No install UX, no builtin migration — those come in Plans 2/3.

**Architecture:** SDK is a workspace package consumed via TypeScript source (`packages/widget-sdk/src/index.ts`). Frontend Registry is a Zustand-style singleton; Loader uses native `import(url)` against `/api/widget-modules/{id}/{*path}`. Backend stores module packages as BLOB in SQLite, serves them with ETag-based caching, and statically extracts the `@serverbee-widget` JSDoc manifest at install time. Import-map shims re-export host singletons so loaded modules cannot ship duplicate React/SDK copies.

**Tech Stack:** Rust (Axum 0.8, sea-orm 1, utoipa 5, serde_json, regex, rust-embed, zip-rs) · TypeScript 5.9 (React 19, Vite 7, TanStack Query 5, Zustand 5, vitest 4) · bun 1.3 workspaces

**Spec:** `docs/superpowers/specs/2026-05-28-custom-widget-system-design.md`

---

## File Structure

### New files

```
packages/widget-sdk/
├── package.json
├── tsconfig.json
├── README.md
├── src/
│   ├── index.ts                        # re-exports public surface
│   ├── define-widget.ts                # defineWidget() + WidgetModule types
│   ├── manifest.ts                     # WidgetManifest, SizingStrategy types
│   ├── runtime-context.ts              # host context injection (createWidgetRuntime)
│   ├── z/
│   │   ├── index.ts                    # z.* exports
│   │   ├── primitives.ts               # string/number/boolean/enum/array/object
│   │   ├── extensions.ts               # serverId/metricPath/color/duration
│   │   └── validate.ts                 # parse/safeParse
│   ├── hooks/
│   │   ├── index.ts
│   │   ├── live.ts                     # useServers, useServer, useMetric, useCapability
│   │   ├── domain.ts                   # useHistory, useAlerts, useServiceMonitors, useTraffic, useUptime, useGeoIp
│   │   ├── escape-hatch.ts             # useApiQuery, useApiMutation
│   │   └── host.ts                     # useTheme, useConfigUpdate
│   ├── actions/
│   │   ├── index.ts                    # ActionDefinition type, ActionsHelper, render()
│   │   └── action-button.tsx           # default button + confirm dialog
│   ├── form/
│   │   ├── index.ts                    # renderConfigForm()
│   │   └── field-renderers.tsx        # one renderer per z.* type
│   └── version.ts                      # SDK_VERSION constant (build-injected later)
└── tests/
    ├── define-widget.test.ts
    ├── z.test.ts
    └── manifest.test.ts

apps/web/src/widgets-runtime/
├── registry.ts                         # WidgetRegistry singleton (Zustand store)
├── registry.test.ts
├── loader.ts                           # bootstrapLoader() — fetches list and imports each
├── loader.test.ts
├── runtime-bridge.ts                   # mounts SDK to globalThis.__SERVERBEE_SDK__
└── shim-template.ts                    # template strings for the 4 shim files

apps/web/public/runtime/                # served by Vite static
├── widget-sdk.js                       # re-exports globalThis.__SERVERBEE_SDK__
├── react.js                            # re-exports globalThis.__SERVERBEE_REACT__
├── react-dom.js                        # re-exports globalThis.__SERVERBEE_REACT_DOM__
└── react-jsx-runtime.js                # re-exports globalThis.__SERVERBEE_JSX_RUNTIME__

crates/server/src/entity/widget_module.rs           # sea-orm entity
crates/server/src/migration/m20260528_000050_create_widget_module.rs
crates/server/src/service/widget_module/
├── mod.rs                              # re-exports
├── extractor.rs                        # JSDoc → WidgetManifest
├── service.rs                          # WidgetModuleService: list/get/serve_asset
├── package.rs                          # package layout: single-file vs zip
└── error.rs                            # WidgetModuleError → AppError
crates/server/src/router/api/widget_module.rs       # /api/widget-modules routes

crates/server/tests/widget_module_integration.rs
```

### Modified files

```
package.json                            # (workspace catalog: no change needed; bun picks up packages/*)
apps/web/package.json                   # add "@serverbee/widget-sdk": "workspace:*"
apps/web/tsconfig.json                  # add path alias for @serverbee/widget-sdk in dev
apps/web/vite.config.ts                 # add resolve.alias for @serverbee/widget-sdk
apps/web/index.html                     # inject <script type="importmap">
apps/web/src/main.tsx                   # call mountSdkBridge() + bootstrapLoader() before render
crates/server/src/entity/mod.rs         # pub mod widget_module;
crates/server/src/migration/mod.rs      # register new migration
crates/server/src/service/mod.rs        # pub mod widget_module;
crates/server/src/router/api/mod.rs     # mount widget_module router
crates/server/src/openapi.rs            # register widget_module schemas + routes
```

---

## Conventions & Reusables

- API responses go through `crate::error::ApiResponse<T>` and `crate::error::ok(...)`.
- Admin-only mutating routes register under `write_router()` (already wraps `require_admin`).
- Migrations: implement `up()` only; `down()` returns `Ok(())` (project convention, CLAUDE.md).
- Frontend tests use Vitest + jsdom; setup in `apps/web/src/test/setup.ts`.
- Rust tests use the helper at `crates/server/tests/integration.rs::start_test_server()` (do not duplicate it — re-import it).

---

## Task 1: Create `@serverbee/widget-sdk` package skeleton

**Files:**
- Create: `packages/widget-sdk/package.json`
- Create: `packages/widget-sdk/tsconfig.json`
- Create: `packages/widget-sdk/src/index.ts`
- Create: `packages/widget-sdk/README.md`

- [ ] **Step 1: Write package.json**

```json
{
  "name": "@serverbee/widget-sdk",
  "version": "0.1.0",
  "type": "module",
  "exports": {
    ".": { "default": "./src/index.ts" },
    "./z": { "default": "./src/z/index.ts" },
    "./hooks": { "default": "./src/hooks/index.ts" },
    "./actions": { "default": "./src/actions/index.ts" }
  },
  "peerDependencies": {
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@tanstack/react-query": "^5.90.21",
    "@types/react": "^19.2.5",
    "react": "^19.2.0",
    "react-dom": "^19.2.0",
    "typescript": "~5.9.3",
    "vitest": "^4.1.0",
    "jsdom": "^28.1.0",
    "@testing-library/react": "^16.3.2"
  }
}
```

- [ ] **Step 2: Write tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "jsx": "react-jsx",
    "strict": true,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "allowSyntheticDefaultImports": true,
    "isolatedModules": true,
    "resolveJsonModule": true,
    "noEmit": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"]
  },
  "include": ["src/**/*", "tests/**/*"]
}
```

- [ ] **Step 3: Write minimal index.ts and README**

`packages/widget-sdk/src/index.ts`:

```ts
export const SDK_VERSION = '0.1.0'
```

`packages/widget-sdk/README.md`:

```markdown
# @serverbee/widget-sdk

Stable API surface for ServerBee custom widgets. Authors `import { defineWidget, z, useMetric } from '@serverbee/widget-sdk'`. See the developer guide at apps/docs/content/docs/{en,cn}/widgets/single-file-guide.mdx.
```

- [ ] **Step 4: Wire workspace, verify install**

In `apps/web/package.json`, add to `dependencies`:

```json
"@serverbee/widget-sdk": "workspace:*"
```

Run from repo root: `bun install`
Expected: success, `apps/web/node_modules/@serverbee/widget-sdk` exists as a symlink to `packages/widget-sdk`.

- [ ] **Step 5: Commit**

```bash
git add packages/widget-sdk apps/web/package.json bun.lock
git -c commit.gpgsign=false commit -m "feat(widget-sdk): scaffold workspace package"
```

---

## Task 2: Add Vite + tsconfig alias so apps/web imports resolve to source

**Files:**
- Modify: `apps/web/vite.config.ts`
- Modify: `apps/web/tsconfig.json`

- [ ] **Step 1: Add alias to vite.config.ts**

In `apps/web/vite.config.ts`, inside `resolve.alias`, add:

```ts
'@serverbee/widget-sdk': path.resolve(__dirname, '../../packages/widget-sdk/src/index.ts'),
'@serverbee/widget-sdk/z': path.resolve(__dirname, '../../packages/widget-sdk/src/z/index.ts'),
'@serverbee/widget-sdk/hooks': path.resolve(__dirname, '../../packages/widget-sdk/src/hooks/index.ts'),
'@serverbee/widget-sdk/actions': path.resolve(__dirname, '../../packages/widget-sdk/src/actions/index.ts'),
```

- [ ] **Step 2: Add path mapping to tsconfig.json**

In `apps/web/tsconfig.json`, under `compilerOptions.paths`, add:

```json
"@serverbee/widget-sdk":         ["../../packages/widget-sdk/src/index.ts"],
"@serverbee/widget-sdk/z":       ["../../packages/widget-sdk/src/z/index.ts"],
"@serverbee/widget-sdk/hooks":   ["../../packages/widget-sdk/src/hooks/index.ts"],
"@serverbee/widget-sdk/actions": ["../../packages/widget-sdk/src/actions/index.ts"]
```

- [ ] **Step 3: Verify typecheck and build still pass**

Run: `cd apps/web && bun run typecheck`
Expected: no errors.

Run: `cd apps/web && bun run build`
Expected: builds successfully (no widget code yet, so nothing imports SDK).

- [ ] **Step 4: Commit**

```bash
git add apps/web/vite.config.ts apps/web/tsconfig.json
git -c commit.gpgsign=false commit -m "build(web): alias @serverbee/widget-sdk to source"
```

---

## Task 3: Define manifest & sizing types

**Files:**
- Create: `packages/widget-sdk/src/manifest.ts`
- Create: `packages/widget-sdk/tests/manifest.test.ts`
- Modify: `packages/widget-sdk/src/index.ts`

- [ ] **Step 1: Write failing test for manifest types**

`packages/widget-sdk/tests/manifest.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { validateManifest } from '../src/manifest'

describe('validateManifest', () => {
  const base = {
    id: 'com.example.foo',
    version: '1.0.0',
    name: 'Foo',
    category: 'Real-time' as const,
    sizing: { defaultW: 3, defaultH: 3, minW: 2, minH: 2, strategy: 'aspect-square' as const },
    sdkVersion: '^0.1.0',
  }

  it('accepts a minimal valid manifest', () => {
    expect(validateManifest(base)).toEqual(base)
  })

  it('rejects missing id', () => {
    expect(() => validateManifest({ ...base, id: '' })).toThrow(/id/)
  })

  it('rejects unknown sizing strategy', () => {
    expect(() =>
      validateManifest({ ...base, sizing: { ...base.sizing, strategy: 'bogus' as any } })
    ).toThrow(/strategy/)
  })

  it('rejects invalid semver', () => {
    expect(() => validateManifest({ ...base, version: 'not-semver' })).toThrow(/version/)
  })
})
```

- [ ] **Step 2: Run test, verify failure**

Run: `cd packages/widget-sdk && bunx vitest run tests/manifest.test.ts`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement manifest.ts**

`packages/widget-sdk/src/manifest.ts`:

```ts
export type WidgetCategory = 'Real-time' | 'Charts' | 'Status'

export type SizingStrategy = 'fixed' | 'free' | 'aspect-square' | 'content-height'

export interface WidgetSizing {
  defaultW: number
  defaultH: number
  minW: number
  minH: number
  maxW?: number
  maxH?: number
  strategy: SizingStrategy
}

export interface WidgetManifest {
  id: string
  version: string
  name: string
  description?: string
  author?: string
  category: WidgetCategory
  sizing: WidgetSizing
  requiredCaps?: string[]
  sdkVersion: string
}

const SEMVER_RE = /^\d+\.\d+\.\d+(-[\w.]+)?$/
const SEMVER_RANGE_RE = /^[\^~]?\d+\.\d+\.\d+/
const VALID_CATEGORIES = new Set<WidgetCategory>(['Real-time', 'Charts', 'Status'])
const VALID_STRATEGIES = new Set<SizingStrategy>([
  'fixed', 'free', 'aspect-square', 'content-height',
])

export function validateManifest(input: unknown): WidgetManifest {
  if (!input || typeof input !== 'object') throw new Error('manifest must be an object')
  const m = input as Record<string, any>

  if (typeof m.id !== 'string' || m.id.length === 0) throw new Error('manifest.id required')
  if (typeof m.version !== 'string' || !SEMVER_RE.test(m.version))
    throw new Error('manifest.version must be valid semver')
  if (typeof m.name !== 'string' || m.name.length === 0) throw new Error('manifest.name required')
  if (!VALID_CATEGORIES.has(m.category))
    throw new Error('manifest.category invalid')
  if (!m.sizing || typeof m.sizing !== 'object') throw new Error('manifest.sizing required')

  const sz = m.sizing as Record<string, any>
  for (const k of ['defaultW', 'defaultH', 'minW', 'minH'] as const) {
    if (typeof sz[k] !== 'number') throw new Error(`manifest.sizing.${k} must be number`)
  }
  if (!VALID_STRATEGIES.has(sz.strategy)) throw new Error('manifest.sizing.strategy invalid')

  if (typeof m.sdkVersion !== 'string' || !SEMVER_RANGE_RE.test(m.sdkVersion))
    throw new Error('manifest.sdkVersion must be valid semver range')

  if (m.requiredCaps !== undefined && !Array.isArray(m.requiredCaps))
    throw new Error('manifest.requiredCaps must be array')

  return m as WidgetManifest
}
```

- [ ] **Step 4: Run test, verify pass**

Run: `cd packages/widget-sdk && bunx vitest run tests/manifest.test.ts`
Expected: PASS (4 tests).

- [ ] **Step 5: Re-export and commit**

In `packages/widget-sdk/src/index.ts`:

```ts
export const SDK_VERSION = '0.1.0'
export type { WidgetManifest, WidgetCategory, WidgetSizing, SizingStrategy } from './manifest'
export { validateManifest } from './manifest'
```

```bash
git add packages/widget-sdk
git -c commit.gpgsign=false commit -m "feat(widget-sdk): WidgetManifest types + validator"
```

---

## Task 4: `defineWidget` + `WidgetModule` types

**Files:**
- Create: `packages/widget-sdk/src/define-widget.ts`
- Create: `packages/widget-sdk/tests/define-widget.test.ts`
- Modify: `packages/widget-sdk/src/index.ts`

- [ ] **Step 1: Write failing test**

`packages/widget-sdk/tests/define-widget.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { defineWidget } from '../src/define-widget'

describe('defineWidget', () => {
  it('wraps the user input into a WidgetModule shape', () => {
    const mod = defineWidget({
      configSchema: { _kind: 'object', shape: {} } as any,
      component: () => null,
    })
    expect(mod.__brand).toBe('WidgetModule')
    expect(typeof mod.component).toBe('function')
    expect(mod.actions).toEqual([])
  })

  it('preserves user-supplied actions array', () => {
    const mod = defineWidget({
      configSchema: { _kind: 'object', shape: {} } as any,
      component: () => null,
      actions: [{ id: 'a', label: 'A', run: async () => {} }],
    })
    expect(mod.actions).toHaveLength(1)
    expect(mod.actions[0].id).toBe('a')
  })

  it('throws when component is missing', () => {
    expect(() => defineWidget({ configSchema: {} as any } as any)).toThrow(/component/)
  })
})
```

- [ ] **Step 2: Run test, verify failure**

Run: `cd packages/widget-sdk && bunx vitest run tests/define-widget.test.ts`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement define-widget.ts**

```ts
import type { ComponentType, ReactNode } from 'react'
import type { ZodTypeAny, Infer } from './z'

export interface ActionContext {
  apiMutation: <Req = unknown, Res = unknown>(method: string, path: string, body?: Req) => Promise<Res>
}

export interface ActionDefinition {
  id: string
  label: string
  icon?: string
  confirm?: { title: string; body?: string }
  run: (ctx: ActionContext) => Promise<void>
}

export interface ActionsHelper {
  render: (id: string) => ReactNode
}

export interface WidgetComponentProps<TConfig> {
  config: TConfig
  size: { w: number; h: number }
  isEditing: boolean
  actions: ActionsHelper
}

export interface DefineWidgetInput<TSchema extends ZodTypeAny> {
  configSchema: TSchema
  component: ComponentType<WidgetComponentProps<Infer<TSchema>>>
  actions?: ActionDefinition[]
}

export interface WidgetModule<TConfig = unknown> {
  __brand: 'WidgetModule'
  configSchema: ZodTypeAny
  component: ComponentType<WidgetComponentProps<TConfig>>
  actions: ActionDefinition[]
}

export function defineWidget<TSchema extends ZodTypeAny>(
  input: DefineWidgetInput<TSchema>,
): WidgetModule<Infer<TSchema>> {
  if (!input || typeof input.component !== 'function') {
    throw new Error('defineWidget: component is required')
  }
  return {
    __brand: 'WidgetModule',
    configSchema: input.configSchema,
    component: input.component as any,
    actions: input.actions ?? [],
  }
}
```

This file imports `ZodTypeAny` and `Infer` from `./z` — those will be created in Task 5. Add temporary stubs to compile:

`packages/widget-sdk/src/z/index.ts` (stub, replaced in Task 5):

```ts
export type ZodTypeAny = { _kind: string }
export type Infer<T extends ZodTypeAny> = any
```

- [ ] **Step 4: Run test, verify pass**

Run: `cd packages/widget-sdk && bunx vitest run tests/define-widget.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Re-export and commit**

In `packages/widget-sdk/src/index.ts`, add:

```ts
export { defineWidget } from './define-widget'
export type {
  WidgetModule, WidgetComponentProps,
  ActionDefinition, ActionContext, ActionsHelper,
} from './define-widget'
```

```bash
git add packages/widget-sdk
git -c commit.gpgsign=false commit -m "feat(widget-sdk): defineWidget + WidgetModule types"
```

---

## Task 5: `z` mini-validator — primitives

**Files:**
- Create: `packages/widget-sdk/src/z/primitives.ts`
- Create: `packages/widget-sdk/src/z/validate.ts`
- Modify: `packages/widget-sdk/src/z/index.ts`
- Create: `packages/widget-sdk/tests/z.test.ts`

- [ ] **Step 1: Write failing tests**

`packages/widget-sdk/tests/z.test.ts`:

```ts
import { describe, it, expect } from 'vitest'
import { z } from '../src/z'

describe('z primitives', () => {
  it('z.string() parses strings', () => {
    expect(z.string().parse('hi')).toBe('hi')
    expect(() => z.string().parse(1)).toThrow(/string/)
  })

  it('z.number() with min/max', () => {
    const s = z.number().min(0).max(10)
    expect(s.parse(5)).toBe(5)
    expect(() => s.parse(-1)).toThrow(/min/)
    expect(() => s.parse(11)).toThrow(/max/)
  })

  it('z.boolean()', () => {
    expect(z.boolean().parse(true)).toBe(true)
    expect(() => z.boolean().parse('true')).toThrow()
  })

  it('z.enum()', () => {
    const s = z.enum(['a', 'b', 'c'] as const)
    expect(s.parse('a')).toBe('a')
    expect(() => s.parse('d')).toThrow(/enum/)
  })

  it('z.array(inner)', () => {
    const s = z.array(z.number())
    expect(s.parse([1, 2])).toEqual([1, 2])
    expect(() => s.parse([1, 'x'])).toThrow()
  })

  it('z.object({ a, b }) applies defaults and rejects missing', () => {
    const s = z.object({
      a: z.string().default('hello'),
      b: z.number(),
    })
    expect(s.parse({ b: 1 })).toEqual({ a: 'hello', b: 1 })
    expect(() => s.parse({ a: 'x' })).toThrow(/b/)
  })

  it('.optional() allows undefined', () => {
    const s = z.string().optional()
    expect(s.parse(undefined)).toBeUndefined()
  })

  it('.describe() attaches label without affecting parse', () => {
    const s = z.string().describe('Server name')
    expect((s as any)._label).toBe('Server name')
    expect(s.parse('x')).toBe('x')
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd packages/widget-sdk && bunx vitest run tests/z.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement primitives + validate**

`packages/widget-sdk/src/z/validate.ts`:

```ts
export class ZError extends Error {
  constructor(public path: string[], message: string) {
    super(`${path.length ? path.join('.') + ': ' : ''}${message}`)
  }
}
```

`packages/widget-sdk/src/z/primitives.ts`:

```ts
import { ZError } from './validate'

export type Infer<T> = T extends ZodSchema<infer U> ? U : never
export type ZodTypeAny = ZodSchema<any>

export abstract class ZodSchema<T> {
  abstract _kind: string
  _label?: string
  _default?: T
  _optional = false

  abstract _parse(input: unknown, path: string[]): T

  parse(input: unknown): T {
    if (input === undefined) {
      if (this._default !== undefined) return this._default
      if (this._optional) return undefined as T
    }
    return this._parse(input, [])
  }

  describe(label: string): this {
    this._label = label
    return this
  }

  default(value: T): this {
    this._default = value
    return this
  }

  optional(): this {
    this._optional = true
    return this
  }
}

class ZString extends ZodSchema<string> {
  _kind = 'string'
  _parse(input: unknown, path: string[]): string {
    if (typeof input !== 'string') throw new ZError(path, 'expected string')
    return input
  }
}

class ZNumber extends ZodSchema<number> {
  _kind = 'number'
  private _min?: number
  private _max?: number

  min(v: number): this { this._min = v; return this }
  max(v: number): this { this._max = v; return this }

  _parse(input: unknown, path: string[]): number {
    if (typeof input !== 'number' || Number.isNaN(input))
      throw new ZError(path, 'expected number')
    if (this._min !== undefined && input < this._min)
      throw new ZError(path, `min ${this._min}`)
    if (this._max !== undefined && input > this._max)
      throw new ZError(path, `max ${this._max}`)
    return input
  }
}

class ZBoolean extends ZodSchema<boolean> {
  _kind = 'boolean'
  _parse(input: unknown, path: string[]): boolean {
    if (typeof input !== 'boolean') throw new ZError(path, 'expected boolean')
    return input
  }
}

class ZEnum<U extends readonly string[]> extends ZodSchema<U[number]> {
  _kind = 'enum'
  constructor(public values: U) { super() }
  _parse(input: unknown, path: string[]): U[number] {
    if (typeof input !== 'string' || !this.values.includes(input as any))
      throw new ZError(path, `enum: expected one of ${this.values.join(', ')}`)
    return input as U[number]
  }
}

class ZArray<T> extends ZodSchema<T[]> {
  _kind = 'array'
  constructor(public inner: ZodSchema<T>) { super() }
  _parse(input: unknown, path: string[]): T[] {
    if (!Array.isArray(input)) throw new ZError(path, 'expected array')
    return input.map((item, i) => this.inner._parse(item, [...path, String(i)]))
  }
}

class ZObject<Shape extends Record<string, ZodTypeAny>> extends ZodSchema<{
  [K in keyof Shape]: Infer<Shape[K]>
}> {
  _kind = 'object'
  constructor(public shape: Shape) { super() }
  _parse(input: unknown, path: string[]) {
    if (!input || typeof input !== 'object' || Array.isArray(input))
      throw new ZError(path, 'expected object')
    const obj = input as Record<string, unknown>
    const out: Record<string, unknown> = {}
    for (const key of Object.keys(this.shape)) {
      const schema = this.shape[key]
      const val = obj[key]
      if (val === undefined) {
        if (schema._default !== undefined) { out[key] = schema._default; continue }
        if (schema._optional) { out[key] = undefined; continue }
        throw new ZError([...path, key], 'required')
      }
      out[key] = schema._parse(val, [...path, key])
    }
    return out as any
  }
}

export const z = {
  string: () => new ZString(),
  number: () => new ZNumber(),
  boolean: () => new ZBoolean(),
  enum: <U extends readonly string[]>(values: U) => new ZEnum(values),
  array: <T>(inner: ZodSchema<T>) => new ZArray(inner),
  object: <S extends Record<string, ZodTypeAny>>(shape: S) => new ZObject(shape),
}
```

`packages/widget-sdk/src/z/index.ts` (replace stub):

```ts
export { z, ZodSchema, type ZodTypeAny, type Infer } from './primitives'
export { ZError } from './validate'
```

- [ ] **Step 4: Run, verify pass**

Run: `cd packages/widget-sdk && bunx vitest run tests/z.test.ts`
Expected: PASS (8 tests).

- [ ] **Step 5: Re-export and commit**

In `packages/widget-sdk/src/index.ts`, add:

```ts
export { z, ZodSchema, type ZodTypeAny, type Infer, ZError } from './z'
```

```bash
git add packages/widget-sdk
git -c commit.gpgsign=false commit -m "feat(widget-sdk): z mini-validator primitives"
```

---

## Task 6: `z` extensions — serverId/metricPath/color/duration

**Files:**
- Create: `packages/widget-sdk/src/z/extensions.ts`
- Modify: `packages/widget-sdk/src/z/index.ts`
- Modify: `packages/widget-sdk/tests/z.test.ts`

- [ ] **Step 1: Add failing tests**

Append to `packages/widget-sdk/tests/z.test.ts`:

```ts
describe('z extensions', () => {
  it('z.serverId() validates non-empty string + marks kind', () => {
    const s = z.serverId()
    expect((s as any)._kind).toBe('serverId')
    expect(s.parse('srv-1')).toBe('srv-1')
    expect(() => s.parse('')).toThrow()
  })

  it('z.metricPath() validates dot/bracket path', () => {
    const s = z.metricPath()
    expect(s.parse('cpu.usage')).toBe('cpu.usage')
    expect(s.parse('disks[0].used')).toBe('disks[0].used')
    expect(() => s.parse('--invalid--')).toThrow()
  })

  it('z.color() accepts hex/oklch/rgb strings', () => {
    const s = z.color()
    expect(s.parse('#fff')).toBe('#fff')
    expect(s.parse('oklch(0.5 0 0)')).toBe('oklch(0.5 0 0)')
  })

  it('z.duration() parses 5m / 1h / 30s', () => {
    const s = z.duration()
    expect(s.parse('5m')).toBe('5m')
    expect(s.parse('1h')).toBe('1h')
    expect(() => s.parse('bogus')).toThrow()
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd packages/widget-sdk && bunx vitest run tests/z.test.ts`
Expected: 4 new tests FAIL.

- [ ] **Step 3: Implement extensions**

`packages/widget-sdk/src/z/extensions.ts`:

```ts
import { ZodSchema } from './primitives'
import { ZError } from './validate'

class ZServerId extends ZodSchema<string> {
  _kind = 'serverId'
  _parse(input: unknown, path: string[]): string {
    if (typeof input !== 'string' || input.length === 0)
      throw new ZError(path, 'expected non-empty serverId')
    return input
  }
}

const METRIC_PATH_RE = /^[a-zA-Z_][a-zA-Z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*|\[\d+\])*$/

class ZMetricPath extends ZodSchema<string> {
  _kind = 'metricPath'
  _parse(input: unknown, path: string[]): string {
    if (typeof input !== 'string' || !METRIC_PATH_RE.test(input))
      throw new ZError(path, 'expected metric path like cpu.usage or disks[0].used')
    return input
  }
}

const COLOR_RE = /^(#[0-9a-fA-F]{3,8}|oklch\([^)]+\)|rgb[a]?\([^)]+\)|hsl[a]?\([^)]+\))$/

class ZColor extends ZodSchema<string> {
  _kind = 'color'
  _parse(input: unknown, path: string[]): string {
    if (typeof input !== 'string' || !COLOR_RE.test(input))
      throw new ZError(path, 'expected CSS color')
    return input
  }
}

const DURATION_RE = /^\d+(s|m|h|d)$/

class ZDuration extends ZodSchema<string> {
  _kind = 'duration'
  _parse(input: unknown, path: string[]): string {
    if (typeof input !== 'string' || !DURATION_RE.test(input))
      throw new ZError(path, 'expected duration like 5m / 1h')
    return input
  }
}

export const serverId = () => new ZServerId()
export const metricPath = () => new ZMetricPath()
export const color = () => new ZColor()
export const duration = () => new ZDuration()
```

In `packages/widget-sdk/src/z/index.ts`, merge extensions into `z`:

```ts
import { z as zPrimitives, ZodSchema, type ZodTypeAny, type Infer } from './primitives'
import { serverId, metricPath, color, duration } from './extensions'

export const z = Object.assign(zPrimitives, { serverId, metricPath, color, duration })
export { ZodSchema, type ZodTypeAny, type Infer }
export { ZError } from './validate'
```

- [ ] **Step 4: Run, verify pass**

Run: `cd packages/widget-sdk && bunx vitest run tests/z.test.ts`
Expected: PASS (12 tests).

- [ ] **Step 5: Commit**

```bash
git add packages/widget-sdk
git -c commit.gpgsign=false commit -m "feat(widget-sdk): z extensions (serverId/metricPath/color/duration)"
```

---

## Task 7: Runtime context (host bridge)

**Files:**
- Create: `packages/widget-sdk/src/runtime-context.ts`
- Modify: `packages/widget-sdk/src/index.ts`

- [ ] **Step 1: Write test stub**

`packages/widget-sdk/tests/runtime-context.test.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import { createWidgetRuntime, getRuntime, resetRuntime } from '../src/runtime-context'

describe('runtime-context', () => {
  beforeEach(() => resetRuntime())

  it('throws if hooks called before host installs runtime', () => {
    expect(() => getRuntime()).toThrow(/runtime not installed/)
  })

  it('returns installed runtime', () => {
    const runtime = createWidgetRuntime({
      apiBaseUrl: '/api',
      queryClient: {} as any,
      serversStore: () => [],
      themeStore: () => ({ mode: 'light', cssVar: () => '' }),
      onConfigUpdate: () => {},
    })
    expect(getRuntime()).toBe(runtime)
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd packages/widget-sdk && bunx vitest run tests/runtime-context.test.ts`
Expected: FAIL (module not found).

- [ ] **Step 3: Implement runtime-context.ts**

```ts
import type { QueryClient } from '@tanstack/react-query'

export interface ServerSummary {
  id: string
  name: string
  online: boolean
  lastSeen: number | null
  capabilities: number
}

export interface WidgetRuntime {
  apiBaseUrl: string
  queryClient: QueryClient
  serversStore: () => ServerSummary[]
  serverByIdStore: (id: string) => unknown    // ServerMetrics
  themeStore: () => { mode: 'light' | 'dark'; cssVar: (name: string) => string }
  onConfigUpdate: (instanceId: string, patch: Record<string, unknown>) => void
}

let _runtime: WidgetRuntime | null = null

export function createWidgetRuntime(rt: Omit<WidgetRuntime, 'serverByIdStore'> & {
  serverByIdStore?: WidgetRuntime['serverByIdStore']
}): WidgetRuntime {
  _runtime = {
    serverByIdStore: () => undefined,
    ...rt,
  }
  return _runtime
}

export function getRuntime(): WidgetRuntime {
  if (!_runtime) throw new Error('widget-sdk: runtime not installed (host bridge missing)')
  return _runtime
}

export function resetRuntime(): void {
  _runtime = null
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd packages/widget-sdk && bunx vitest run tests/runtime-context.test.ts`
Expected: PASS.

- [ ] **Step 5: Re-export and commit**

In `packages/widget-sdk/src/index.ts`, add:

```ts
export { createWidgetRuntime, getRuntime, resetRuntime } from './runtime-context'
export type { WidgetRuntime, ServerSummary } from './runtime-context'
```

```bash
git add packages/widget-sdk
git -c commit.gpgsign=false commit -m "feat(widget-sdk): host-runtime bridge"
```

---

## Task 8: Live hooks (`useServers`, `useServer`, `useMetric`, `useCapability`)

**Files:**
- Create: `packages/widget-sdk/src/hooks/live.ts`
- Create: `packages/widget-sdk/src/hooks/index.ts`
- Create: `packages/widget-sdk/tests/hooks-live.test.tsx`

- [ ] **Step 1: Write failing test**

`packages/widget-sdk/tests/hooks-live.test.tsx`:

```tsx
import { describe, it, expect, beforeEach } from 'vitest'
import { renderHook } from '@testing-library/react'
import { useServers, useMetric, useCapability } from '../src/hooks/live'
import { createWidgetRuntime, resetRuntime } from '../src/runtime-context'

describe('live hooks', () => {
  beforeEach(() => {
    resetRuntime()
    createWidgetRuntime({
      apiBaseUrl: '/api',
      queryClient: {} as any,
      serversStore: () => [
        { id: 's1', name: 'one', online: true, lastSeen: null, capabilities: 1 | 8 },
      ],
      serverByIdStore: (id) =>
        id === 's1' ? { id: 's1', cpu: { usage: 42 }, disks: [{ used: 100 }] } : undefined,
      themeStore: () => ({ mode: 'light', cssVar: () => '' }),
      onConfigUpdate: () => {},
    })
  })

  it('useServers returns runtime list', () => {
    const { result } = renderHook(() => useServers())
    expect(result.current).toHaveLength(1)
    expect(result.current[0].id).toBe('s1')
  })

  it('useMetric extracts dot path', () => {
    const { result } = renderHook(() => useMetric('s1', 'cpu.usage'))
    expect(result.current).toBe(42)
  })

  it('useMetric extracts bracket path', () => {
    const { result } = renderHook(() => useMetric('s1', 'disks[0].used'))
    expect(result.current).toBe(100)
  })

  it('useMetric returns undefined when serverId is null', () => {
    const { result } = renderHook(() => useMetric(null, 'cpu.usage'))
    expect(result.current).toBeUndefined()
  })

  it('useCapability checks bitmask', () => {
    const { result: ping } = renderHook(() => useCapability('s1', 'CAP_PING_ICMP'))
    expect(ping.current).toBe(true)
    const { result: term } = renderHook(() => useCapability('s1', 'CAP_TERMINAL'))
    expect(term.current).toBe(true)
    const { result: docker } = renderHook(() => useCapability('s1', 'CAP_DOCKER'))
    expect(docker.current).toBe(false)
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd packages/widget-sdk && bunx vitest run tests/hooks-live.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement live hooks**

`packages/widget-sdk/src/hooks/live.ts`:

```ts
import { getRuntime, type ServerSummary } from '../runtime-context'

const CAPS: Record<string, number> = {
  CAP_TERMINAL: 1,
  CAP_EXEC: 2,
  CAP_UPGRADE: 4,
  CAP_PING_ICMP: 8,
  CAP_PING_TCP: 16,
  CAP_PING_HTTP: 32,
  CAP_FILE: 64,
  CAP_DOCKER: 128,
  CAP_SECURITY_EVENTS: 256,
  CAP_FIREWALL_BLOCK: 512,
  CAP_IP_QUALITY: 1024,
}

export function useServers(): ServerSummary[] {
  return getRuntime().serversStore()
}

export function useServer(id: string | null): unknown {
  if (id === null) return undefined
  return getRuntime().serverByIdStore(id)
}

const PATH_TOKEN_RE = /[a-zA-Z_][a-zA-Z0-9_]*|\[\d+\]/g

export function useMetric(id: string | null, path: string): number | string | undefined {
  if (id === null) return undefined
  const server = getRuntime().serverByIdStore(id) as Record<string, any> | undefined
  if (!server) return undefined
  const tokens = path.match(PATH_TOKEN_RE) ?? []
  let cur: any = server
  for (const tok of tokens) {
    if (cur == null) return undefined
    cur = tok.startsWith('[') ? cur[Number(tok.slice(1, -1))] : cur[tok]
  }
  return cur
}

export function useCapability(id: string | null, cap: string): boolean {
  if (id === null) return false
  const bit = CAPS[cap]
  if (!bit) return false
  const server = getRuntime().serversStore().find((s) => s.id === id)
  return server ? (server.capabilities & bit) !== 0 : false
}
```

`packages/widget-sdk/src/hooks/index.ts`:

```ts
export { useServers, useServer, useMetric, useCapability } from './live'
```

- [ ] **Step 4: Run, verify pass**

Run: `cd packages/widget-sdk && bunx vitest run tests/hooks-live.test.tsx`
Expected: PASS (5 tests).

- [ ] **Step 5: Re-export and commit**

In `packages/widget-sdk/src/index.ts`:

```ts
export * from './hooks'
```

```bash
git add packages/widget-sdk
git -c commit.gpgsign=false commit -m "feat(widget-sdk): live hooks"
```

---

## Task 9: Escape-hatch hooks (`useApiQuery`, `useApiMutation`)

**Files:**
- Create: `packages/widget-sdk/src/hooks/escape-hatch.ts`
- Modify: `packages/widget-sdk/src/hooks/index.ts`
- Create: `packages/widget-sdk/tests/hooks-api.test.tsx`

- [ ] **Step 1: Write failing test**

`packages/widget-sdk/tests/hooks-api.test.tsx`:

```tsx
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderHook, waitFor } from '@testing-library/react'
import { createElement } from 'react'
import { useApiQuery, useApiMutation } from '../src/hooks/escape-hatch'
import { createWidgetRuntime, resetRuntime } from '../src/runtime-context'

describe('api hooks', () => {
  let qc: QueryClient
  beforeEach(() => {
    resetRuntime()
    qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    createWidgetRuntime({
      apiBaseUrl: '/api',
      queryClient: qc,
      serversStore: () => [],
      themeStore: () => ({ mode: 'light', cssVar: () => '' }),
      onConfigUpdate: () => {},
    })
    global.fetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ data: { hello: 'world' } }),
    }) as any
  })

  const wrapper = ({ children }: any) =>
    createElement(QueryClientProvider, { client: qc, children })

  it('useApiQuery unwraps {data}', async () => {
    const { result } = renderHook(() => useApiQuery<{ hello: string }>('/api/test'), { wrapper })
    await waitFor(() => expect(result.current.data).toEqual({ hello: 'world' }))
  })

  it('useApiMutation calls fetch with method+body', async () => {
    const { result } = renderHook(() => useApiMutation<{ ok: true }, { x: number }>('POST', '/api/do'), { wrapper })
    await result.current.mutateAsync({ x: 1 })
    expect(global.fetch).toHaveBeenCalledWith(
      '/api/do',
      expect.objectContaining({ method: 'POST', credentials: 'include' }),
    )
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd packages/widget-sdk && bunx vitest run tests/hooks-api.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

`packages/widget-sdk/src/hooks/escape-hatch.ts`:

```ts
import { useQuery, useMutation, type UseQueryResult, type UseMutationResult } from '@tanstack/react-query'

async function request<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method,
    credentials: 'include',
    headers: body ? { 'content-type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  })
  if (!res.ok) throw new Error(`${method} ${path}: ${res.status}`)
  const json = await res.json()
  return (json && typeof json === 'object' && 'data' in json) ? (json as { data: T }).data : (json as T)
}

export function useApiQuery<T>(
  path: string,
  opts?: { params?: Record<string, string | number | undefined>; enabled?: boolean },
): UseQueryResult<T> {
  const params = opts?.params
  const url = params
    ? `${path}?${new URLSearchParams(
        Object.fromEntries(
          Object.entries(params).filter(([, v]) => v !== undefined).map(([k, v]) => [k, String(v)]),
        ),
      ).toString()}`
    : path
  return useQuery<T>({
    queryKey: ['widget-api', url],
    queryFn: () => request<T>('GET', url),
    enabled: opts?.enabled,
  })
}

export function useApiMutation<TRes, TReq = void>(
  method: string,
  path: string,
): UseMutationResult<TRes, Error, TReq> {
  return useMutation<TRes, Error, TReq>({
    mutationFn: (body) => request<TRes>(method, path, body),
  })
}
```

In `packages/widget-sdk/src/hooks/index.ts`:

```ts
export { useServers, useServer, useMetric, useCapability } from './live'
export { useApiQuery, useApiMutation } from './escape-hatch'
```

- [ ] **Step 4: Run, verify pass**

Run: `cd packages/widget-sdk && bunx vitest run tests/hooks-api.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add packages/widget-sdk
git -c commit.gpgsign=false commit -m "feat(widget-sdk): useApiQuery/useApiMutation"
```

---

## Task 10: Domain hooks (alerts/service-monitors/traffic/uptime/geoip/history)

**Files:**
- Create: `packages/widget-sdk/src/hooks/domain.ts`
- Modify: `packages/widget-sdk/src/hooks/index.ts`

- [ ] **Step 1: Implement domain hooks (thin wrappers — tests come at integration layer)**

`packages/widget-sdk/src/hooks/domain.ts`:

```ts
import { useApiQuery, useApiMutation } from './escape-hatch'

export interface AlertEvent { id: string; severity: string; message: string; created_at: string }
export interface ServiceMonitor { id: string; name: string; status: string }

export function useAlerts(opts?: { limit?: number }) {
  return useApiQuery<AlertEvent[]>('/api/alert-events', { params: { limit: opts?.limit ?? 20 } })
}

export function useServiceMonitors() {
  return useApiQuery<ServiceMonitor[]>('/api/service-monitors')
}

export interface TrafficPoint { ts: number; rx: number; tx: number }

export function useTraffic(serverId: string | null, range?: string) {
  return useApiQuery<TrafficPoint[]>(
    serverId ? `/api/servers/${serverId}/traffic` : '/api/traffic/overview/daily',
    { params: { range }, enabled: true },
  )
}

export interface UptimeEntry { day: string; uptime_pct: number; incidents: number }

export function useUptime(serverId: string | null, days = 30) {
  return useApiQuery<UptimeEntry[]>(
    serverId ? `/api/servers/${serverId}/uptime-daily` : '/api/uptime/overview',
    { params: { days } },
  )
}

export interface HistoryPoint { ts: number; value: number }

export function useHistory(serverId: string | null, path: string, range: string) {
  return useApiQuery<HistoryPoint[]>('/api/metrics/history', {
    params: { server_id: serverId ?? undefined, path, range },
    enabled: serverId !== null,
  })
}

export function useGeoIp() {
  const status = useApiQuery<{ installed: boolean; source?: string }>('/api/geoip/status')
  const download = useApiMutation<{ success: boolean }>('POST', '/api/geoip/download')
  return { status, download }
}
```

In `packages/widget-sdk/src/hooks/index.ts`, append:

```ts
export {
  useAlerts, useServiceMonitors, useTraffic, useUptime, useHistory, useGeoIp,
  type AlertEvent, type ServiceMonitor, type TrafficPoint, type UptimeEntry, type HistoryPoint,
} from './domain'
```

- [ ] **Step 2: Verify typecheck**

Run: `cd packages/widget-sdk && bunx tsc --noEmit`
Expected: no errors.

- [ ] **Step 3: Run all SDK tests**

Run: `cd packages/widget-sdk && bunx vitest run`
Expected: all existing tests still PASS.

- [ ] **Step 4: Commit**

```bash
git add packages/widget-sdk
git -c commit.gpgsign=false commit -m "feat(widget-sdk): domain hooks (alerts/services/traffic/uptime/history/geoip)"
```

---

## Task 11: Host hooks (`useTheme`, `useConfigUpdate`)

**Files:**
- Create: `packages/widget-sdk/src/hooks/host.ts`
- Modify: `packages/widget-sdk/src/hooks/index.ts`

- [ ] **Step 1: Implement and re-export**

`packages/widget-sdk/src/hooks/host.ts`:

```ts
import { getRuntime } from '../runtime-context'

export function useTheme() {
  return getRuntime().themeStore()
}

export function useConfigUpdate<TConfig = Record<string, unknown>>(instanceId: string) {
  const runtime = getRuntime()
  return (patch: Partial<TConfig>) =>
    runtime.onConfigUpdate(instanceId, patch as Record<string, unknown>)
}
```

In `packages/widget-sdk/src/hooks/index.ts`, append:

```ts
export { useTheme, useConfigUpdate } from './host'
```

- [ ] **Step 2: Verify typecheck**

Run: `cd packages/widget-sdk && bunx tsc --noEmit`
Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add packages/widget-sdk
git -c commit.gpgsign=false commit -m "feat(widget-sdk): host hooks (useTheme/useConfigUpdate)"
```

---

## Task 12: Actions runner

**Files:**
- Create: `packages/widget-sdk/src/actions/index.ts`
- Create: `packages/widget-sdk/src/actions/action-button.tsx`
- Create: `packages/widget-sdk/tests/actions.test.tsx`

- [ ] **Step 1: Write test**

`packages/widget-sdk/tests/actions.test.tsx`:

```tsx
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { createActionsHelper } from '../src/actions'
import type { ActionDefinition } from '../src/define-widget'

describe('actions helper', () => {
  beforeEach(() => {
    global.fetch = vi.fn().mockResolvedValue({ ok: true, json: async () => ({ data: { ok: true } }) }) as any
  })

  it('renders a button that triggers run() and shows loading state', async () => {
    const run = vi.fn().mockResolvedValue(undefined)
    const actions: ActionDefinition[] = [{ id: 'a1', label: 'Do it', run }]
    const helper = createActionsHelper(actions)
    render(<>{helper.render('a1')}</>)
    fireEvent.click(screen.getByRole('button', { name: 'Do it' }))
    await waitFor(() => expect(run).toHaveBeenCalledOnce())
  })

  it('returns null for unknown id', () => {
    const helper = createActionsHelper([])
    const { container } = render(<>{helper.render('missing')}</>)
    expect(container.textContent).toBe('')
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd packages/widget-sdk && bunx vitest run tests/actions.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement**

`packages/widget-sdk/src/actions/action-button.tsx`:

```tsx
import { useState } from 'react'
import type { ActionDefinition } from '../define-widget'

interface Props {
  action: ActionDefinition
  onRun: () => Promise<void>
}

export function ActionButton({ action, onRun }: Props) {
  const [pending, setPending] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const trigger = async () => {
    if (action.confirm && !confirming) { setConfirming(true); return }
    setConfirming(false)
    setPending(true)
    try { await onRun() } finally { setPending(false) }
  }
  return (
    <button type="button" disabled={pending} onClick={trigger}>
      {confirming ? `Confirm: ${action.label}` : action.label}
      {pending ? '…' : ''}
    </button>
  )
}
```

`packages/widget-sdk/src/actions/index.ts`:

```tsx
import type { ReactNode } from 'react'
import type { ActionDefinition, ActionsHelper, ActionContext } from '../define-widget'
import { ActionButton } from './action-button'

async function apiMutation<Req = unknown, Res = unknown>(method: string, path: string, body?: Req): Promise<Res> {
  const res = await fetch(path, {
    method, credentials: 'include',
    headers: body ? { 'content-type': 'application/json' } : undefined,
    body: body ? JSON.stringify(body) : undefined,
  })
  if (!res.ok) throw new Error(`${method} ${path}: ${res.status}`)
  const json = await res.json()
  return (json && typeof json === 'object' && 'data' in json) ? (json as any).data : json
}

export function createActionsHelper(actions: ActionDefinition[]): ActionsHelper {
  const ctx: ActionContext = { apiMutation }
  return {
    render(id: string): ReactNode {
      const action = actions.find((a) => a.id === id)
      if (!action) return null
      return <ActionButton key={id} action={action} onRun={() => action.run(ctx)} />
    },
  }
}

export type { ActionDefinition, ActionsHelper, ActionContext } from '../define-widget'
```

- [ ] **Step 4: Run, verify pass**

Run: `cd packages/widget-sdk && bunx vitest run tests/actions.test.tsx`
Expected: PASS.

- [ ] **Step 5: Re-export and commit**

In `packages/widget-sdk/src/index.ts`:

```ts
export { createActionsHelper } from './actions'
```

```bash
git add packages/widget-sdk
git -c commit.gpgsign=false commit -m "feat(widget-sdk): actions runner + ActionButton"
```

---

## Task 13: `renderConfigForm` (minimal, primitives + enum + serverId)

**Files:**
- Create: `packages/widget-sdk/src/form/index.tsx`
- Create: `packages/widget-sdk/src/form/field-renderers.tsx`
- Create: `packages/widget-sdk/tests/form.test.tsx`

- [ ] **Step 1: Write test**

`packages/widget-sdk/tests/form.test.tsx`:

```tsx
import { describe, it, expect, beforeEach } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { useState } from 'react'
import { z } from '../src/z'
import { renderConfigForm } from '../src/form'
import { createWidgetRuntime, resetRuntime } from '../src/runtime-context'

function Wrapper({ schema, initial }: any) {
  const [value, setValue] = useState(initial)
  return <>{renderConfigForm(schema, value, setValue)}</>
}

describe('renderConfigForm', () => {
  beforeEach(() => {
    resetRuntime()
    createWidgetRuntime({
      apiBaseUrl: '/api', queryClient: {} as any,
      serversStore: () => [{ id: 's1', name: 'One', online: true, lastSeen: null, capabilities: 0 }],
      themeStore: () => ({ mode: 'light', cssVar: () => '' }),
      onConfigUpdate: () => {},
    })
  })

  it('renders a text input for z.string()', () => {
    const schema = z.object({ name: z.string().describe('Name') })
    render(<Wrapper schema={schema} initial={{ name: 'hi' }} />)
    const input = screen.getByLabelText('Name') as HTMLInputElement
    expect(input.value).toBe('hi')
    fireEvent.change(input, { target: { value: 'world' } })
    expect(input.value).toBe('world')
  })

  it('renders a number input for z.number()', () => {
    const schema = z.object({ count: z.number().describe('Count') })
    render(<Wrapper schema={schema} initial={{ count: 5 }} />)
    expect((screen.getByLabelText('Count') as HTMLInputElement).value).toBe('5')
  })

  it('renders a select for z.enum()', () => {
    const schema = z.object({ mode: z.enum(['a', 'b'] as const).describe('Mode') })
    render(<Wrapper schema={schema} initial={{ mode: 'a' }} />)
    expect(screen.getByLabelText('Mode')).toBeTruthy()
  })

  it('renders a server picker for z.serverId()', () => {
    const schema = z.object({ srv: z.serverId().describe('Server') })
    render(<Wrapper schema={schema} initial={{ srv: 's1' }} />)
    expect(screen.getByLabelText('Server')).toBeTruthy()
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd packages/widget-sdk && bunx vitest run tests/form.test.tsx`
Expected: FAIL.

- [ ] **Step 3: Implement renderers + form**

`packages/widget-sdk/src/form/field-renderers.tsx`:

```tsx
import type { ZodTypeAny } from '../z'
import { getRuntime } from '../runtime-context'

interface FieldProps {
  schema: ZodTypeAny
  value: unknown
  onChange: (v: unknown) => void
  id: string
  label: string
}

export function renderField(props: FieldProps) {
  const kind = (props.schema as any)._kind
  switch (kind) {
    case 'string': return <input id={props.id} type="text" value={(props.value as string) ?? ''} onChange={(e) => props.onChange(e.target.value)} />
    case 'number': return <input id={props.id} type="number" value={(props.value as number) ?? ''} onChange={(e) => props.onChange(e.target.value === '' ? undefined : Number(e.target.value))} />
    case 'boolean': return <input id={props.id} type="checkbox" checked={!!props.value} onChange={(e) => props.onChange(e.target.checked)} />
    case 'enum': {
      const opts = (props.schema as any).values as string[]
      return (
        <select id={props.id} value={(props.value as string) ?? ''} onChange={(e) => props.onChange(e.target.value)}>
          {opts.map((o) => <option key={o} value={o}>{o}</option>)}
        </select>
      )
    }
    case 'serverId': {
      const servers = getRuntime().serversStore()
      return (
        <select id={props.id} value={(props.value as string) ?? ''} onChange={(e) => props.onChange(e.target.value)}>
          <option value="">— choose —</option>
          {servers.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
        </select>
      )
    }
    case 'metricPath':
    case 'color':
    case 'duration':
    default:
      return <input id={props.id} type="text" value={(props.value as string) ?? ''} onChange={(e) => props.onChange(e.target.value)} />
  }
}
```

`packages/widget-sdk/src/form/index.tsx`:

```tsx
import type { ReactNode } from 'react'
import type { ZodTypeAny } from '../z'
import { renderField } from './field-renderers'

export function renderConfigForm(
  schema: ZodTypeAny,
  value: Record<string, unknown>,
  onChange: (v: Record<string, unknown>) => void,
): ReactNode {
  if ((schema as any)._kind !== 'object') {
    return <em>Top-level schema must be z.object()</em>
  }
  const shape = (schema as any).shape as Record<string, ZodTypeAny>
  return (
    <div>
      {Object.entries(shape).map(([key, fieldSchema]) => {
        const label = (fieldSchema as any)._label ?? key
        const id = `cfg-${key}`
        return (
          <div key={key} style={{ marginBottom: 8 }}>
            <label htmlFor={id} style={{ display: 'block' }}>{label}</label>
            {renderField({
              schema: fieldSchema,
              value: value[key],
              onChange: (v) => onChange({ ...value, [key]: v }),
              id,
              label,
            })}
          </div>
        )
      })}
    </div>
  )
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd packages/widget-sdk && bunx vitest run tests/form.test.tsx`
Expected: PASS.

- [ ] **Step 5: Re-export and commit**

In `packages/widget-sdk/src/index.ts`:

```ts
export { renderConfigForm } from './form'
```

```bash
git add packages/widget-sdk
git -c commit.gpgsign=false commit -m "feat(widget-sdk): renderConfigForm + field renderers"
```

---

## Task 14: Frontend Widget Registry

**Files:**
- Create: `apps/web/src/widgets-runtime/registry.ts`
- Create: `apps/web/src/widgets-runtime/registry.test.ts`

- [ ] **Step 1: Write failing test**

`apps/web/src/widgets-runtime/registry.test.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import type { WidgetModule, WidgetManifest } from '@serverbee/widget-sdk'
import { useWidgetRegistry, registryActions } from './registry'

const fakeManifest: WidgetManifest = {
  id: 'com.test.foo', version: '1.0.0', name: 'Foo', category: 'Real-time',
  sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' },
  sdkVersion: '^0.1.0',
}
const fakeModule: WidgetModule = {
  __brand: 'WidgetModule',
  configSchema: {} as any,
  component: () => null,
  actions: [],
}

describe('registry', () => {
  beforeEach(() => useWidgetRegistry.setState({ modules: new Map(), failures: new Map() }))

  it('registers and retrieves a module', () => {
    registryActions.register('com.test.foo', fakeModule, fakeManifest)
    const entry = registryActions.get('com.test.foo')
    expect(entry?.manifest.name).toBe('Foo')
    expect(entry?.module).toBe(fakeModule)
  })

  it('records load failures', () => {
    registryActions.recordLoadFailure('com.test.bad', new Error('boom'))
    expect(registryActions.list()).toEqual([])
    expect(useWidgetRegistry.getState().failures.get('com.test.bad')?.message).toBe('boom')
  })

  it('unregister removes the module', () => {
    registryActions.register('com.test.foo', fakeModule, fakeManifest)
    registryActions.unregister('com.test.foo')
    expect(registryActions.get('com.test.foo')).toBeUndefined()
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd apps/web && bunx vitest run src/widgets-runtime/registry.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement**

`apps/web/src/widgets-runtime/registry.ts`:

```ts
import { create } from 'zustand'
import type { WidgetModule, WidgetManifest } from '@serverbee/widget-sdk'

export interface RegistryEntry {
  manifest: WidgetManifest
  module: WidgetModule
}

interface RegistryState {
  modules: Map<string, RegistryEntry>
  failures: Map<string, Error>
}

export const useWidgetRegistry = create<RegistryState>(() => ({
  modules: new Map(),
  failures: new Map(),
}))

export const registryActions = {
  register(id: string, module: WidgetModule, manifest: WidgetManifest) {
    useWidgetRegistry.setState((state) => {
      const modules = new Map(state.modules)
      modules.set(id, { manifest, module })
      const failures = new Map(state.failures)
      failures.delete(id)
      return { modules, failures }
    })
  },
  unregister(id: string) {
    useWidgetRegistry.setState((state) => {
      const modules = new Map(state.modules)
      modules.delete(id)
      return { modules }
    })
  },
  get(id: string): RegistryEntry | undefined {
    return useWidgetRegistry.getState().modules.get(id)
  },
  list(): RegistryEntry[] {
    return Array.from(useWidgetRegistry.getState().modules.values())
  },
  recordLoadFailure(id: string, err: Error) {
    useWidgetRegistry.setState((state) => {
      const failures = new Map(state.failures)
      failures.set(id, err)
      return { failures }
    })
  },
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd apps/web && bunx vitest run src/widgets-runtime/registry.test.ts`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/widgets-runtime
git -c commit.gpgsign=false commit -m "feat(web): Widget Registry singleton"
```

---

## Task 15: Frontend Loader

**Files:**
- Create: `apps/web/src/widgets-runtime/loader.ts`
- Create: `apps/web/src/widgets-runtime/loader.test.ts`

- [ ] **Step 1: Write failing test**

`apps/web/src/widgets-runtime/loader.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from 'vitest'
import { bootstrapLoader } from './loader'
import { useWidgetRegistry } from './registry'

describe('bootstrapLoader', () => {
  beforeEach(() => {
    useWidgetRegistry.setState({ modules: new Map(), failures: new Map() })
  })

  it('lists modules, imports each, registers them', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: true, json: async () => ({ data: [
        { id: 'com.test.foo', version: '1.0.0', entry_path: 'index.js', manifest: {
          id: 'com.test.foo', version: '1.0.0', name: 'Foo', category: 'Real-time',
          sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' },
          sdkVersion: '^0.1.0',
        } },
      ] }),
    }) as any
    const fakeModule = { __brand: 'WidgetModule', configSchema: {}, component: () => null, actions: [] }
    const importer = vi.fn().mockResolvedValue({ default: fakeModule })
    await bootstrapLoader({ importer })
    expect(useWidgetRegistry.getState().modules.size).toBe(1)
    expect(useWidgetRegistry.getState().modules.get('com.test.foo')?.manifest.name).toBe('Foo')
  })

  it('isolates one module failure', async () => {
    global.fetch = vi.fn().mockResolvedValue({
      ok: true, json: async () => ({ data: [
        { id: 'a', version: '1.0.0', entry_path: 'a.js', manifest: { id: 'a', version: '1.0.0', name: 'A', category: 'Real-time', sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' }, sdkVersion: '^0.1.0' } },
        { id: 'b', version: '1.0.0', entry_path: 'b.js', manifest: { id: 'b', version: '1.0.0', name: 'B', category: 'Real-time', sizing: { defaultW: 2, defaultH: 2, minW: 1, minH: 1, strategy: 'free' }, sdkVersion: '^0.1.0' } },
      ] }),
    }) as any
    const importer = vi.fn().mockImplementation((url: string) =>
      url.includes('a.js')
        ? Promise.resolve({ default: { __brand: 'WidgetModule', configSchema: {}, component: () => null, actions: [] } })
        : Promise.reject(new Error('boom')),
    )
    await bootstrapLoader({ importer })
    expect(useWidgetRegistry.getState().modules.has('a')).toBe(true)
    expect(useWidgetRegistry.getState().failures.has('b')).toBe(true)
  })
})
```

- [ ] **Step 2: Run, verify failure**

Run: `cd apps/web && bunx vitest run src/widgets-runtime/loader.test.ts`
Expected: FAIL.

- [ ] **Step 3: Implement**

`apps/web/src/widgets-runtime/loader.ts`:

```ts
import type { WidgetManifest, WidgetModule } from '@serverbee/widget-sdk'
import { registryActions } from './registry'

interface ListEntry {
  id: string
  version: string
  entry_path: string
  manifest: WidgetManifest
}

export interface BootstrapOptions {
  /** Override the import function (used in tests). */
  importer?: (url: string) => Promise<{ default: WidgetModule }>
  baseUrl?: string
}

export async function bootstrapLoader(opts: BootstrapOptions = {}): Promise<void> {
  const base = opts.baseUrl ?? '/api/widget-modules'
  const importer = opts.importer ?? ((url: string) => import(/* @vite-ignore */ url))

  const res = await fetch(base, { credentials: 'include' })
  if (!res.ok) throw new Error(`bootstrapLoader: list failed ${res.status}`)
  const body = await res.json() as { data: ListEntry[] }
  const modules = body.data

  await Promise.allSettled(
    modules.map(async (entry) => {
      try {
        const url = `${base}/${entry.id}/${entry.entry_path}`
        const mod = await importer(url)
        if (!mod.default || mod.default.__brand !== 'WidgetModule') {
          throw new Error(`module ${entry.id} did not export a WidgetModule via default`)
        }
        registryActions.register(entry.id, mod.default, entry.manifest)
      } catch (err) {
        registryActions.recordLoadFailure(entry.id, err instanceof Error ? err : new Error(String(err)))
      }
    }),
  )
}
```

- [ ] **Step 4: Run, verify pass**

Run: `cd apps/web && bunx vitest run src/widgets-runtime/loader.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/widgets-runtime
git -c commit.gpgsign=false commit -m "feat(web): bootstrap loader (fetch list, import, isolate failures)"
```

---

## Task 16: Runtime bridge — mount SDK + React to globalThis

**Files:**
- Create: `apps/web/src/widgets-runtime/runtime-bridge.ts`
- Create: `apps/web/public/runtime/widget-sdk.js`
- Create: `apps/web/public/runtime/react.js`
- Create: `apps/web/public/runtime/react-dom.js`
- Create: `apps/web/public/runtime/react-jsx-runtime.js`
- Modify: `apps/web/index.html`
- Modify: `apps/web/src/main.tsx`

- [ ] **Step 1: Write shim files**

`apps/web/public/runtime/widget-sdk.js`:

```js
const ns = globalThis.__SERVERBEE_SDK__
if (!ns) throw new Error('widget-sdk shim: host did not mount __SERVERBEE_SDK__')
export const defineWidget = ns.defineWidget
export const z = ns.z
export const createActionsHelper = ns.createActionsHelper
export const renderConfigForm = ns.renderConfigForm
export const useServers = ns.useServers
export const useServer = ns.useServer
export const useMetric = ns.useMetric
export const useCapability = ns.useCapability
export const useApiQuery = ns.useApiQuery
export const useApiMutation = ns.useApiMutation
export const useAlerts = ns.useAlerts
export const useServiceMonitors = ns.useServiceMonitors
export const useTraffic = ns.useTraffic
export const useUptime = ns.useUptime
export const useHistory = ns.useHistory
export const useGeoIp = ns.useGeoIp
export const useTheme = ns.useTheme
export const useConfigUpdate = ns.useConfigUpdate
export const SDK_VERSION = ns.SDK_VERSION
```

`apps/web/public/runtime/react.js`:

```js
const r = globalThis.__SERVERBEE_REACT__
if (!r) throw new Error('react shim: host did not mount __SERVERBEE_REACT__')
export default r
export const useState = r.useState
export const useEffect = r.useEffect
export const useMemo = r.useMemo
export const useCallback = r.useCallback
export const useRef = r.useRef
export const useContext = r.useContext
export const useReducer = r.useReducer
export const useLayoutEffect = r.useLayoutEffect
export const createContext = r.createContext
export const Fragment = r.Fragment
export const memo = r.memo
export const forwardRef = r.forwardRef
export const Component = r.Component
export const Children = r.Children
export const cloneElement = r.cloneElement
export const createElement = r.createElement
export const isValidElement = r.isValidElement
```

`apps/web/public/runtime/react-dom.js`:

```js
const rd = globalThis.__SERVERBEE_REACT_DOM__
if (!rd) throw new Error('react-dom shim: host did not mount __SERVERBEE_REACT_DOM__')
export default rd
export const createPortal = rd.createPortal
export const flushSync = rd.flushSync
```

`apps/web/public/runtime/react-jsx-runtime.js`:

```js
const j = globalThis.__SERVERBEE_JSX_RUNTIME__
if (!j) throw new Error('jsx-runtime shim: host did not mount __SERVERBEE_JSX_RUNTIME__')
export const jsx = j.jsx
export const jsxs = j.jsxs
export const Fragment = j.Fragment
```

- [ ] **Step 2: Write runtime-bridge.ts**

`apps/web/src/widgets-runtime/runtime-bridge.ts`:

```ts
import * as React from 'react'
import * as ReactDOM from 'react-dom'
import * as JsxRuntime from 'react/jsx-runtime'
import * as Sdk from '@serverbee/widget-sdk'
import type { QueryClient } from '@tanstack/react-query'
import type { ServerSummary } from '@serverbee/widget-sdk'

export interface BridgeInputs {
  queryClient: QueryClient
  serversStore: () => ServerSummary[]
  serverByIdStore: (id: string) => unknown
  themeStore: () => { mode: 'light' | 'dark'; cssVar: (n: string) => string }
  onConfigUpdate: (instanceId: string, patch: Record<string, unknown>) => void
}

export function mountRuntimeBridge(inputs: BridgeInputs): void {
  ;(globalThis as any).__SERVERBEE_REACT__ = React
  ;(globalThis as any).__SERVERBEE_REACT_DOM__ = ReactDOM
  ;(globalThis as any).__SERVERBEE_JSX_RUNTIME__ = JsxRuntime
  ;(globalThis as any).__SERVERBEE_SDK__ = Sdk

  Sdk.createWidgetRuntime({
    apiBaseUrl: '/api',
    queryClient: inputs.queryClient,
    serversStore: inputs.serversStore,
    serverByIdStore: inputs.serverByIdStore,
    themeStore: inputs.themeStore,
    onConfigUpdate: inputs.onConfigUpdate,
  })
}
```

- [ ] **Step 3: Inject importmap in index.html**

In `apps/web/index.html`, **before** the existing `<script type="module" src="/src/main.tsx">`:

```html
<script type="importmap">
  {
    "imports": {
      "@serverbee/widget-sdk":  "/runtime/widget-sdk.js",
      "react":                  "/runtime/react.js",
      "react-dom":              "/runtime/react-dom.js",
      "react/jsx-runtime":      "/runtime/react-jsx-runtime.js"
    }
  }
</script>
```

- [ ] **Step 4: Wire main.tsx**

In `apps/web/src/main.tsx`, after `QueryClient` is created and before `ReactDOM.createRoot(...).render(...)`, add:

```ts
import { mountRuntimeBridge } from './widgets-runtime/runtime-bridge'
import { bootstrapLoader } from './widgets-runtime/loader'

mountRuntimeBridge({
  queryClient,
  serversStore: () => [],            // wired in a later plan (Plan 2A)
  serverByIdStore: () => undefined,  // wired in Plan 2A
  themeStore: () => ({ mode: document.documentElement.classList.contains('dark') ? 'dark' : 'light', cssVar: (n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim() }),
  onConfigUpdate: () => {},          // wired in Plan 2A
})

// Best-effort: don't block rendering on module list (endpoint may not exist yet during Plan 1).
bootstrapLoader().catch((e) => console.warn('widget bootstrap failed', e))
```

Note: in Plan 1 there are no modules to load (the endpoint returns empty), so the loader is essentially a no-op. Real builtin loading happens after Plan 2A.

- [ ] **Step 5: Build + commit**

Run: `cd apps/web && bun run build`
Expected: builds successfully.

```bash
git add apps/web/index.html apps/web/src/main.tsx apps/web/src/widgets-runtime/runtime-bridge.ts apps/web/public/runtime
git -c commit.gpgsign=false commit -m "feat(web): runtime bridge + import-map shims for widget SDK"
```

---

## Task 17: Backend — `widget_module` entity

**Files:**
- Create: `crates/server/src/entity/widget_module.rs`
- Modify: `crates/server/src/entity/mod.rs`

- [ ] **Step 1: Write entity**

`crates/server/src/entity/widget_module.rs`:

```rust
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize, utoipa::ToSchema)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(32))")]
pub enum SourceType {
    #[sea_orm(string_value = "Builtin")]
    Builtin,
    #[sea_orm(string_value = "Url")]
    Url,
    #[sea_orm(string_value = "Upload")]
    Upload,
    #[sea_orm(string_value = "BundledByTheme")]
    BundledByTheme,
}

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize, utoipa::ToSchema)]
#[sea_orm(table_name = "widget_module")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub version: String,
    pub source_type: SourceType,
    pub source_url: Option<String>,
    pub bundled_by_theme_id: Option<String>,
    pub manifest_json: String,
    pub code_sha256: String,
    pub entry_path: String,
    #[serde(skip)]
    pub package_blob: Option<Vec<u8>>,
    pub installed_by: Option<i64>,
    pub installed_at: DateTimeUtc,
    pub enabled: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
```

- [ ] **Step 2: Register entity**

In `crates/server/src/entity/mod.rs`, add:

```rust
pub mod widget_module;
```

- [ ] **Step 3: Build, verify compile**

Run: `cargo build -p serverbee-server`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/entity
git -c commit.gpgsign=false commit -m "feat(server): widget_module sea-orm entity"
```

---

## Task 18: Backend — migration for `widget_module`

**Files:**
- Create: `crates/server/src/migration/m20260528_000050_create_widget_module.rs`
- Modify: `crates/server/src/migration/mod.rs`

- [ ] **Step 1: Write migration**

`crates/server/src/migration/m20260528_000050_create_widget_module.rs`:

```rust
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(r#"
            CREATE TABLE IF NOT EXISTS widget_module (
                id                    TEXT NOT NULL PRIMARY KEY,
                version               TEXT NOT NULL,
                source_type           TEXT NOT NULL,
                source_url            TEXT,
                bundled_by_theme_id   TEXT,
                manifest_json         TEXT NOT NULL,
                code_sha256           TEXT NOT NULL,
                entry_path            TEXT NOT NULL,
                package_blob          BLOB,
                installed_by          INTEGER,
                installed_at          TEXT NOT NULL,
                enabled               INTEGER NOT NULL DEFAULT 1
            )
        "#).await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_widget_module_source_type ON widget_module(source_type)",
        ).await?;
        db.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_widget_module_theme ON widget_module(bundled_by_theme_id) WHERE bundled_by_theme_id IS NOT NULL",
        ).await?;
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}
```

- [ ] **Step 2: Register migration**

In `crates/server/src/migration/mod.rs`, find the `migrations()` vec and append the new module + entry. Example:

```rust
mod m20260528_000050_create_widget_module;
// inside Migrator::migrations():
Box::new(m20260528_000050_create_widget_module::Migration),
```

- [ ] **Step 3: Build & run server-side migration test (if any)**

Run: `cargo build -p serverbee-server`
Expected: success.

Run: `cargo test -p serverbee-server migration --no-run` (compile only; quick check)
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/migration
git -c commit.gpgsign=false commit -m "feat(server): migration for widget_module table"
```

---

## Task 19: Backend — `WidgetManifest` Rust struct + JSDoc extractor

**Files:**
- Create: `crates/server/src/service/widget_module/mod.rs`
- Create: `crates/server/src/service/widget_module/extractor.rs`
- Modify: `crates/server/src/service/mod.rs`

- [ ] **Step 1: Scaffold `mod.rs`**

`crates/server/src/service/widget_module/mod.rs`:

```rust
pub mod extractor;
pub mod error;
pub mod service;

pub use error::WidgetModuleError;
pub use service::WidgetModuleService;
```

`crates/server/src/service/widget_module/error.rs`:

```rust
use crate::error::AppError;

#[derive(Debug, thiserror::Error)]
pub enum WidgetModuleError {
    #[error("manifest extraction failed: {0}")]
    ManifestExtraction(String),
    #[error("manifest validation failed: {0}")]
    ManifestValidation(String),
    #[error("module id conflict: {0}")]
    IdConflict(String),
    #[error("module not found: {0}")]
    NotFound(String),
    #[error("invalid asset path")]
    InvalidAssetPath,
    #[error("database: {0}")]
    Db(#[from] sea_orm::DbErr),
}

impl From<WidgetModuleError> for AppError {
    fn from(err: WidgetModuleError) -> Self {
        match err {
            WidgetModuleError::NotFound(_) => AppError::not_found(err.to_string()),
            WidgetModuleError::IdConflict(_) => AppError::conflict(err.to_string()),
            WidgetModuleError::InvalidAssetPath
            | WidgetModuleError::ManifestExtraction(_)
            | WidgetModuleError::ManifestValidation(_) => AppError::bad_request(err.to_string()),
            WidgetModuleError::Db(e) => AppError::internal(format!("db error: {e}")),
        }
    }
}
```

- [ ] **Step 2: Write extractor tests**

`crates/server/src/service/widget_module/extractor.rs`:

```rust
use serde::{Deserialize, Serialize};
use regex::Regex;
use once_cell::sync::Lazy;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct WidgetSizing {
    pub default_w: u32,
    pub default_h: u32,
    pub min_w: u32,
    pub min_h: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_w: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_h: Option<u32>,
    pub strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, utoipa::ToSchema)]
pub struct WidgetManifest {
    pub id: String,
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    pub category: String,
    pub sizing: WidgetSizing,
    #[serde(default)]
    pub required_caps: Option<Vec<String>>,
    pub sdk_version: String,
}

static JSDOC_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)/\*\*[\s\S]*?@serverbee-widget\s+(\{[\s\S]*?\})[\s\S]*?\*/").unwrap()
});

static LINE_DECOR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s*\*\s?").unwrap());
static SEMVER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d+\.\d+\.\d+(-[\w.]+)?$").unwrap());
static SEMVER_RANGE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\^~]?\d+\.\d+\.\d+").unwrap());

pub fn extract_manifest(source: &str) -> Result<WidgetManifest, super::WidgetModuleError> {
    use super::WidgetModuleError as E;
    let captures = JSDOC_RE.captures(source)
        .ok_or_else(|| E::ManifestExtraction("no @serverbee-widget JSDoc block found".into()))?;
    let raw_json = captures.get(1).unwrap().as_str();
    let cleaned = LINE_DECOR_RE.replace_all(raw_json, "").to_string();

    let manifest: WidgetManifest = serde_json::from_str(&cleaned)
        .map_err(|e| E::ManifestExtraction(format!("invalid JSON: {e}")))?;

    if manifest.id.is_empty() { return Err(E::ManifestValidation("id required".into())); }
    if !SEMVER_RE.is_match(&manifest.version) {
        return Err(E::ManifestValidation("version must be semver".into()));
    }
    if manifest.name.is_empty() { return Err(E::ManifestValidation("name required".into())); }
    if !matches!(manifest.category.as_str(), "Real-time" | "Charts" | "Status") {
        return Err(E::ManifestValidation("category invalid".into()));
    }
    if !matches!(manifest.sizing.strategy.as_str(), "fixed" | "free" | "aspect-square" | "content-height") {
        return Err(E::ManifestValidation("sizing.strategy invalid".into()));
    }
    if !SEMVER_RANGE_RE.is_match(&manifest.sdk_version) {
        return Err(E::ManifestValidation("sdkVersion must be semver range".into()));
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"/**
 * @serverbee-widget {
 *   "id": "com.example.cpu",
 *   "version": "1.0.0",
 *   "name": "CPU",
 *   "category": "Real-time",
 *   "sizing": { "defaultW": 3, "defaultH": 3, "minW": 2, "minH": 2, "strategy": "aspect-square" },
 *   "sdkVersion": "^0.1.0"
 * }
 */
export default {};
"#;

    #[test]
    fn extracts_a_valid_manifest() {
        let m = extract_manifest(GOOD).unwrap();
        assert_eq!(m.id, "com.example.cpu");
        assert_eq!(m.sizing.strategy, "aspect-square");
    }

    #[test]
    fn rejects_missing_block() {
        let res = extract_manifest("export default {};");
        assert!(matches!(res.unwrap_err(), super::super::WidgetModuleError::ManifestExtraction(_)));
    }

    #[test]
    fn rejects_invalid_category() {
        let src = GOOD.replace(r#""category": "Real-time""#, r#""category": "Bogus""#);
        let res = extract_manifest(&src);
        assert!(res.is_err());
    }

    #[test]
    fn rejects_invalid_semver() {
        let src = GOOD.replace(r#""version": "1.0.0""#, r#""version": "not-semver""#);
        assert!(extract_manifest(&src).is_err());
    }
}
```

- [ ] **Step 3: Register module**

In `crates/server/src/service/mod.rs`:

```rust
pub mod widget_module;
```

In `crates/server/Cargo.toml`, ensure dependencies present (most should already exist):
- `regex = "1"` (likely present)
- `once_cell = "1"` (likely present)
- `thiserror = "1"` (likely present)

If any missing, add to `[dependencies]`.

- [ ] **Step 4: Run extractor tests**

Run: `cargo test -p serverbee-server service::widget_module::extractor --lib`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/service crates/server/Cargo.toml
git -c commit.gpgsign=false commit -m "feat(server): widget manifest JSDoc extractor + validator"
```

---

## Task 20: Backend — `WidgetModuleService` (list / get / serve_asset)

**Files:**
- Create: `crates/server/src/service/widget_module/service.rs`
- Create: `crates/server/src/service/widget_module/package.rs`

- [ ] **Step 1: Write package layout helper**

`crates/server/src/service/widget_module/package.rs`:

```rust
use std::collections::HashMap;
use std::io::{Cursor, Read};
use super::WidgetModuleError;

/// In-memory representation of a module package, addressable by path.
pub struct UnpackedPackage {
    pub entries: HashMap<String, Vec<u8>>,
}

impl UnpackedPackage {
    /// A single-file package: entry_path is treated as the only file name.
    pub fn from_single_file(entry_path: &str, code: Vec<u8>) -> Self {
        let mut entries = HashMap::new();
        entries.insert(entry_path.to_string(), code);
        Self { entries }
    }

    /// Unpack a zip blob (defends against zip-slip and oversize entries).
    pub fn from_zip(blob: &[u8]) -> Result<Self, WidgetModuleError> {
        const MAX_ENTRY_BYTES: u64 = 5 * 1024 * 1024;
        let reader = Cursor::new(blob);
        let mut zip = zip::ZipArchive::new(reader)
            .map_err(|e| WidgetModuleError::ManifestExtraction(format!("invalid zip: {e}")))?;
        let mut entries = HashMap::new();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)
                .map_err(|e| WidgetModuleError::ManifestExtraction(format!("zip entry: {e}")))?;
            if entry.is_dir() { continue; }
            let name = entry.enclosed_name()
                .ok_or(WidgetModuleError::InvalidAssetPath)?
                .to_string_lossy().to_string();
            if entry.size() > MAX_ENTRY_BYTES {
                return Err(WidgetModuleError::ManifestExtraction(format!("entry too large: {name}")));
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buf)
                .map_err(|e| WidgetModuleError::ManifestExtraction(format!("read: {e}")))?;
            entries.insert(name, buf);
        }
        Ok(Self { entries })
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        let normalised = path.trim_start_matches('/');
        if normalised.contains("..") { return None; }
        self.entries.get(normalised).map(|v| v.as_slice())
    }
}
```

If `zip` crate is not in `Cargo.toml`, add: `zip = { version = "2", default-features = false, features = ["deflate"] }`.

- [ ] **Step 2: Write service**

`crates/server/src/service/widget_module/service.rs`:

```rust
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use serde::Serialize;

use super::{WidgetModuleError, extractor::WidgetManifest, package::UnpackedPackage};
use crate::entity::widget_module::{self, Entity as WidgetModuleEntity, SourceType};

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct WidgetModuleListEntry {
    pub id: String,
    pub version: String,
    pub source_type: SourceType,
    pub entry_path: String,
    pub code_sha256: String,
    pub manifest: serde_json::Value,
    pub enabled: bool,
}

pub struct WidgetModuleService;

impl WidgetModuleService {
    pub async fn list(db: &DatabaseConnection) -> Result<Vec<WidgetModuleListEntry>, WidgetModuleError> {
        let rows = WidgetModuleEntity::find()
            .filter(widget_module::Column::Enabled.eq(true))
            .order_by_asc(widget_module::Column::Id)
            .all(db)
            .await?;
        rows.into_iter().map(|r| {
            let manifest: serde_json::Value = serde_json::from_str(&r.manifest_json)
                .map_err(|e| WidgetModuleError::ManifestValidation(format!("stored manifest invalid: {e}")))?;
            Ok(WidgetModuleListEntry {
                id: r.id,
                version: r.version,
                source_type: r.source_type,
                entry_path: r.entry_path,
                code_sha256: r.code_sha256,
                manifest,
                enabled: r.enabled,
            })
        }).collect()
    }

    pub async fn get(db: &DatabaseConnection, id: &str) -> Result<widget_module::Model, WidgetModuleError> {
        WidgetModuleEntity::find_by_id(id.to_string())
            .one(db)
            .await?
            .ok_or_else(|| WidgetModuleError::NotFound(id.to_string()))
    }

    /// Loads a package from BLOB and returns the bytes of a single asset path.
    /// `entry_path` is the module's main file; other paths may be relative imports or assets.
    pub async fn serve_asset(
        db: &DatabaseConnection, id: &str, requested: &str,
    ) -> Result<(Vec<u8>, String), WidgetModuleError> {
        let row = Self::get(db, id).await?;
        let blob = row.package_blob.ok_or_else(|| WidgetModuleError::NotFound(format!("{id}: no blob (builtin?)")))?;

        // Heuristic: single-file packages store the entire entry as their blob.
        // Multi-file (zip) packages are decoded via zip crate.
        let package = if blob.starts_with(b"PK\x03\x04") {
            UnpackedPackage::from_zip(&blob)?
        } else {
            UnpackedPackage::from_single_file(&row.entry_path, blob)
        };

        let bytes = package.get(requested)
            .ok_or(WidgetModuleError::InvalidAssetPath)?
            .to_vec();
        let mime = mime_for(requested);
        Ok((bytes, mime))
    }
}

fn mime_for(path: &str) -> String {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".js") || lower.ends_with(".mjs") { "text/javascript; charset=utf-8" }
    else if lower.ends_with(".json") { "application/json" }
    else if lower.ends_with(".css") { "text/css" }
    else if lower.ends_with(".svg") { "image/svg+xml" }
    else if lower.ends_with(".png") { "image/png" }
    else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { "image/jpeg" }
    else if lower.ends_with(".webp") { "image/webp" }
    else { "application/octet-stream" }
    .to_string()
}
```

- [ ] **Step 3: Build, verify compile**

Run: `cargo build -p serverbee-server`
Expected: success.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/service/widget_module crates/server/Cargo.toml
git -c commit.gpgsign=false commit -m "feat(server): WidgetModuleService (list/get/serve_asset) + package extractor"
```

---

## Task 21: Backend — API routes `/api/widget-modules`

**Files:**
- Create: `crates/server/src/router/api/widget_module.rs`
- Modify: `crates/server/src/router/api/mod.rs`
- Modify: `crates/server/src/openapi.rs`

- [ ] **Step 1: Write routes file**

`crates/server/src/router/api/widget_module.rs`:

```rust
use std::sync::Arc;
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};

use crate::{
    error::{AppError, ApiResponse, ok},
    service::widget_module::{WidgetModuleService, service::WidgetModuleListEntry},
    state::AppState,
};

pub fn read_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/widget-modules", get(list_modules))
        .route("/widget-modules/{id}/{*asset_path}", get(serve_asset))
}

#[utoipa::path(
    get,
    path = "/api/widget-modules",
    tag = "widget-modules",
    responses((status = 200, body = Vec<WidgetModuleListEntry>)),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn list_modules(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ApiResponse<Vec<WidgetModuleListEntry>>>, AppError> {
    let modules = WidgetModuleService::list(&state.db).await?;
    ok(modules)
}

#[utoipa::path(
    get,
    path = "/api/widget-modules/{id}/{asset_path}",
    tag = "widget-modules",
    params(
        ("id" = String, Path),
        ("asset_path" = String, Path)
    ),
    responses((status = 200, description = "Asset bytes"), (status = 404)),
    security(("session_cookie" = []), ("api_key" = []))
)]
async fn serve_asset(
    State(state): State<Arc<AppState>>,
    Path((id, asset_path)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let (bytes, mime) = WidgetModuleService::serve_asset(&state.db, &id, &asset_path).await?;
    let row = WidgetModuleService::get(&state.db, &id).await?;
    let etag = format!("\"{}-{}\"", row.version, &row.code_sha256[..8]);

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap());
    headers.insert(header::ETAG, HeaderValue::from_str(&etag).unwrap());
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=86400, immutable"));
    Ok((StatusCode::OK, headers, bytes))
}
```

- [ ] **Step 2: Mount router**

In `crates/server/src/router/api/mod.rs`, in the function that composes read routes, merge the new router:

```rust
pub mod widget_module;
// inside the read router builder:
.merge(widget_module::read_router())
```

- [ ] **Step 3: Register OpenAPI**

In `crates/server/src/openapi.rs`, add the new paths and schemas to the `OpenApi` derive list:

```rust
paths(
    // ... existing paths,
    crate::router::api::widget_module::list_modules,
    crate::router::api::widget_module::serve_asset,
),
components(schemas(
    // ... existing schemas,
    crate::service::widget_module::service::WidgetModuleListEntry,
    crate::entity::widget_module::SourceType,
)),
```

- [ ] **Step 4: Build, verify compile**

Run: `cargo build -p serverbee-server`
Expected: success.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/router/api crates/server/src/openapi.rs
git -c commit.gpgsign=false commit -m "feat(server): /api/widget-modules list + asset endpoints"
```

---

## Task 22: Backend — integration test for list + asset roundtrip

**Files:**
- Create: `crates/server/tests/widget_module_integration.rs`

- [ ] **Step 1: Write integration test**

`crates/server/tests/widget_module_integration.rs`:

```rust
// Reuse the in-process test server from the main integration suite.
// Project convention: each integration test file is its own cargo target,
// but we re-use the helper module by path.

#[path = "integration.rs"]
mod integration;

use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, DatabaseConnection};
use chrono::Utc;
use serverbee_server::entity::widget_module::{self, SourceType};
use serverbee_server::service::widget_module::extractor::extract_manifest;

#[tokio::test]
async fn list_returns_seeded_module() {
    let (base, _tmp, db) = integration::start_test_server().await.expect("server");
    seed_module(&db, "com.test.foo").await;

    let client = reqwest::Client::new();
    let res = client.get(format!("{base}/api/widget-modules"))
        .send().await.unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    let list = body["data"].as_array().unwrap();
    assert!(list.iter().any(|m| m["id"] == "com.test.foo"));
}

#[tokio::test]
async fn serve_asset_returns_entry_bytes() {
    let (base, _tmp, db) = integration::start_test_server().await.expect("server");
    seed_module(&db, "com.test.foo").await;

    let res = reqwest::get(format!("{base}/api/widget-modules/com.test.foo/index.js"))
        .await.unwrap();
    assert_eq!(res.status(), 200);
    assert!(res.headers().get("content-type").unwrap().to_str().unwrap().contains("javascript"));
    assert!(res.headers().contains_key("etag"));
    let body = res.text().await.unwrap();
    assert!(body.contains("@serverbee-widget"));
}

#[tokio::test]
async fn serve_asset_rejects_path_traversal() {
    let (base, _tmp, db) = integration::start_test_server().await.expect("server");
    seed_module(&db, "com.test.foo").await;

    let res = reqwest::get(format!("{base}/api/widget-modules/com.test.foo/../secret"))
        .await.unwrap();
    assert!(res.status().is_client_error() || res.status() == reqwest::StatusCode::NOT_FOUND);
}

async fn seed_module(db: &DatabaseConnection, id: &str) {
    let code = format!(r#"/**
 * @serverbee-widget {{
 *   "id": "{id}",
 *   "version": "1.0.0",
 *   "name": "Foo",
 *   "category": "Real-time",
 *   "sizing": {{ "defaultW": 3, "defaultH": 3, "minW": 2, "minH": 2, "strategy": "aspect-square" }},
 *   "sdkVersion": "^0.1.0"
 * }}
 */
export default {{}};"#);
    let manifest = extract_manifest(&code).expect("manifest");
    let sha = sha256_hex(code.as_bytes());

    widget_module::ActiveModel {
        id: Set(id.to_string()),
        version: Set("1.0.0".into()),
        source_type: Set(SourceType::Upload),
        source_url: Set(None),
        bundled_by_theme_id: Set(None),
        manifest_json: Set(serde_json::to_string(&manifest).unwrap()),
        code_sha256: Set(sha),
        entry_path: Set("index.js".into()),
        package_blob: Set(Some(code.into_bytes())),
        installed_by: Set(None),
        installed_at: Set(Utc::now()),
        enabled: Set(true),
    }
    .insert(db).await.unwrap();
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
```

Update `integration.rs::start_test_server()` if needed so it returns `(String, TempDir, DatabaseConnection)` instead of `(String, TempDir)`. If the existing signature differs, instead of changing it, expose a helper there: `pub async fn start_test_server_with_db() -> ...`. Inspect the existing file before editing.

In `crates/server/Cargo.toml` `[dev-dependencies]`, ensure `sha2`, `reqwest = { version = "...", features = ["json"] }` are present (likely already).

- [ ] **Step 2: Run integration tests**

Run: `cargo test -p serverbee-server --test widget_module_integration`
Expected: 3 tests PASS.

- [ ] **Step 3: Run full test suite**

Run: `cargo test -p serverbee-server`
Expected: existing tests unaffected.

- [ ] **Step 4: Commit**

```bash
git add crates/server/tests/widget_module_integration.rs crates/server/Cargo.toml crates/server/tests/integration.rs
git -c commit.gpgsign=false commit -m "test(server): integration tests for widget_module list + asset"
```

---

## Task 23: Wire SDK exports + final typecheck/build sanity

**Files:**
- Modify: `packages/widget-sdk/src/index.ts`

- [ ] **Step 1: Make sure index.ts exports the full public surface**

`packages/widget-sdk/src/index.ts` (final form):

```ts
export const SDK_VERSION = '0.1.0'

// Types & validators
export { validateManifest } from './manifest'
export type { WidgetManifest, WidgetCategory, WidgetSizing, SizingStrategy } from './manifest'

// defineWidget
export { defineWidget } from './define-widget'
export type {
  WidgetModule, WidgetComponentProps,
  ActionDefinition, ActionContext, ActionsHelper,
} from './define-widget'

// Schema
export { z, ZodSchema, type ZodTypeAny, type Infer, ZError } from './z'

// Runtime context
export { createWidgetRuntime, getRuntime, resetRuntime } from './runtime-context'
export type { WidgetRuntime, ServerSummary } from './runtime-context'

// Hooks
export * from './hooks'

// Actions
export { createActionsHelper } from './actions'

// Form
export { renderConfigForm } from './form'
```

- [ ] **Step 2: Full SDK test suite + typecheck**

Run: `cd packages/widget-sdk && bunx vitest run`
Expected: all tests PASS.

Run: `cd packages/widget-sdk && bunx tsc --noEmit`
Expected: no errors.

- [ ] **Step 3: Full apps/web test + build**

Run: `cd apps/web && bun run test`
Expected: all existing tests + new registry/loader tests PASS.

Run: `cd apps/web && bun run typecheck && bun run build`
Expected: success.

- [ ] **Step 4: Full Rust workspace check**

Run: `cargo clippy --workspace -- -D warnings`
Expected: success (0 warnings).

Run: `cargo test --workspace`
Expected: all tests PASS.

- [ ] **Step 5: Commit & celebrate Plan 1 completion**

```bash
git add packages/widget-sdk/src/index.ts
git -c commit.gpgsign=false commit -m "feat(widget-sdk): finalise public exports surface"
```

---

## Plan 1 — Completion Criteria

After all 23 tasks, the following must hold:

- `bun install` succeeds at repo root with `@serverbee/widget-sdk` symlinked into `apps/web/node_modules`.
- `cd packages/widget-sdk && bunx vitest run` — all SDK tests pass.
- `cd apps/web && bun run test && bun run typecheck && bun run build` — frontend builds.
- `cargo clippy --workspace -- -D warnings` — clean.
- `cargo test --workspace` — passes, including the new `widget_module_integration` suite.
- `GET /api/widget-modules` returns `{ "data": [] }` on a fresh DB.
- A seeded module can be served byte-perfect through `GET /api/widget-modules/{id}/index.js`, with `ETag` and `Cache-Control: immutable` headers.
- The `<script type="importmap">` is in `index.html`; the four shim files exist under `apps/web/public/runtime/`.

What is intentionally **not** delivered in Plan 1 (handled in later plans):

- Built-in widget compilation pipeline (Plan 2A).
- Wiring `serversStore` / `serverByIdStore` / `onConfigUpdate` to the real WS/dashboard state (Plan 2A).
- URL/Upload install UX (Plan 3A).
- Theme system (Plan 3B).
- Deletion of legacy `spa_theme` / `custom_theme` (Plan 3C).
- Documentation site updates (Plan 4).

---

## Self-Review Notes (for executor)

- Tasks 5–6 require Task 4's stub file to compile; do not skip ahead.
- Task 7's `runtime-context.ts` import in Task 8's `live.ts` is essential — verify with `tsc --noEmit` after Task 8.
- Task 16's shim files must be in `public/`, not `src/`, so Vite copies them through.
- Task 18's migration filename includes the timestamp `m20260528_000050` — keep it lexicographically after the most recent existing migration so the runtime ordering is correct. Verify by `ls crates/server/src/migration | tail -3` before adding.
- Task 22's `start_test_server()` signature change is invasive — prefer adding a second helper to the existing test file rather than editing the signature.
