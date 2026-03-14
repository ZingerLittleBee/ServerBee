# Ultracite Code Standards

This project uses **Ultracite**, a zero-config preset that enforces strict code quality standards through automated formatting and linting.

## Quick Reference

- **Format code**: `bun x ultracite fix`
- **Check for issues**: `bun x ultracite check`
- **Diagnose setup**: `bun x ultracite doctor`

Biome (the underlying engine) provides robust linting and formatting. Most issues are automatically fixable.

---

## Core Principles

Write code that is **accessible, performant, type-safe, and maintainable**. Focus on clarity and explicit intent over brevity.

### Type Safety & Explicitness

- Use explicit types for function parameters and return values when they enhance clarity
- Prefer `unknown` over `any` when the type is genuinely unknown
- Use const assertions (`as const`) for immutable values and literal types
- Leverage TypeScript's type narrowing instead of type assertions
- Use meaningful variable names instead of magic numbers - extract constants with descriptive names

### Modern JavaScript/TypeScript

- Use arrow functions for callbacks and short functions
- Prefer `for...of` loops over `.forEach()` and indexed `for` loops
- Use optional chaining (`?.`) and nullish coalescing (`??`) for safer property access
- Prefer template literals over string concatenation
- Use destructuring for object and array assignments
- Use `const` by default, `let` only when reassignment is needed, never `var`

### Async & Promises

- Always `await` promises in async functions - don't forget to use the return value
- Use `async/await` syntax instead of promise chains for better readability
- Handle errors appropriately in async code with try-catch blocks
- Don't use async functions as Promise executors

### React & JSX

- Use function components over class components
- Call hooks at the top level only, never conditionally
- Specify all dependencies in hook dependency arrays correctly
- Use the `key` prop for elements in iterables (prefer unique IDs over array indices)
- Nest children between opening and closing tags instead of passing as props
- Don't define components inside other components
- Use semantic HTML and ARIA attributes for accessibility:
  - Provide meaningful alt text for images
  - Use proper heading hierarchy
  - Add labels for form inputs
  - Include keyboard event handlers alongside mouse events
  - Use semantic elements (`<button>`, `<nav>`, etc.) instead of divs with roles

### Error Handling & Debugging

- Remove `console.log`, `debugger`, and `alert` statements from production code
- Throw `Error` objects with descriptive messages, not strings or other values
- Use `try-catch` blocks meaningfully - don't catch errors just to rethrow them
- Prefer early returns over nested conditionals for error cases

### Code Organization

- Keep functions focused and under reasonable cognitive complexity limits
- Extract complex conditions into well-named boolean variables
- Use early returns to reduce nesting
- Prefer simple conditionals over nested ternary operators
- Group related code together and separate concerns

### Security

- Add `rel="noopener"` when using `target="_blank"` on links
- Avoid `dangerouslySetInnerHTML` unless absolutely necessary
- Don't use `eval()` or assign directly to `document.cookie`
- Validate and sanitize user input

### Performance

- Avoid spread syntax in accumulators within loops
- Use top-level regex literals instead of creating them in loops
- Prefer specific imports over namespace imports
- Avoid barrel files (index files that re-export everything)
- Use proper image components (e.g., Next.js `<Image>`) over `<img>` tags

### Framework-Specific Guidance

**Next.js:**

- Use Next.js `<Image>` component for images
- Use `next/head` or App Router metadata API for head elements
- Use Server Components for async data fetching instead of async Client Components

**React 19+:**

- Use ref as a prop instead of `React.forwardRef`

**Solid/Svelte/Vue/Qwik:**

- Use `class` and `for` attributes (not `className` or `htmlFor`)

---

## When Biome Can't Help

Biome's linter will catch most issues automatically. Focus your attention on:

1. **Business logic correctness** - Biome can't validate your algorithms
2. **Meaningful naming** - Use descriptive names for functions, variables, and types
3. **Architecture decisions** - Component structure, data flow, and API design
4. **Edge cases** - Handle boundary conditions and error states
5. **User experience** - Accessibility, performance, and usability considerations
6. **Documentation** - Add comments for complex logic, but prefer self-documenting code

---

Most formatting and common issues are automatically fixed by Biome. Run `bun x ultracite fix` before committing to ensure compliance.

---

## Project Structure

ServerBee is a VPS monitoring probe system (Rust backend + React frontend).

```
crates/
  common/    — Shared types, protocol messages, capability constants
  server/    — Axum HTTP/WS server, sea-orm entities, services, background tasks
  agent/     — System metrics collector, WS reporter, PTY terminal, ping probes
apps/
  web/       — React 19 SPA (TanStack Router + Query, shadcn/ui, Recharts)
  fumadocs/  — Documentation site (TanStack Start + Fumadocs MDX, CN+EN bilingual)
```

### Key Commands

```bash
cargo build --workspace                    # Build all Rust crates
cargo run -p serverbee-server              # Run server (port 9527)
cargo run -p serverbee-agent               # Run agent
cd apps/web && bun install && bun run build # Build frontend (embedded into server binary)
cargo clippy --workspace -- -D warnings    # Lint Rust (CI enforced, 0 warnings)
bun x ultracite check                      # Lint frontend
bun run typecheck                          # TypeScript check (web + fumadocs)
```

### Rust Conventions

- **Error handling**: `AppError` enum with `thiserror`, return `Result<T, AppError>`
- **Database**: SQLite via sea-orm, migrations in `crates/server/src/migration/`
- **API annotations**: All endpoints use `#[utoipa::path]`, all DTOs use `#[derive(ToSchema)]`
- **Config**: Figment with `SB_` env prefix, `__` (double underscore) as nested separator (e.g., `SB_ADMIN__PASSWORD` maps to `admin.password`)
- **Capabilities**: u32 bitmask — `CAP_TERMINAL=1, CAP_EXEC=2, CAP_UPGRADE=4, CAP_PING_ICMP=8, CAP_PING_TCP=16, CAP_PING_HTTP=32`

### Frontend Conventions

- shadcn/ui components in `apps/web/src/components/ui/`
- API client in `apps/web/src/lib/api-client.ts` (auto-unwraps `{ data: T }`)
- WebSocket hooks in `apps/web/src/hooks/`
- Route files in `apps/web/src/routes/` (TanStack Router file-based routing)

---

## Testing

See `TESTING.md` for the full testing guide (commands, coverage, manual verification checklist).

- Write assertions inside `it()` or `test()` blocks
- Avoid done callbacks in async tests - use async/await instead
- Don't use `.only` or `.skip` in committed code
- Keep test suites reasonably flat - avoid excessive `describe` nesting

**Keep `TESTING.md` in sync with code changes.** When adding/removing/modifying tests, API endpoints, or testable features, update `TESTING.md` accordingly — test counts, file locations, coverage tables, and the manual verification checklist must reflect the current state of the codebase.
