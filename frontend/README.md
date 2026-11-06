# claude-proxy frontend

Svelte SPA for inspecting traffic through the Rust `claude-proxy`. Phases 1–5
are all shipped: request list, request detail, real-time streaming updates,
session grouping, analysis dashboard, and a compose modal for sending prompts
through the proxy via `claude -p`. Since Phase 2.6.5 the SPA runs as an
independent `vite preview` process and is fully decoupled from the Rust
binary, so shipping a UI change never touches the proxy.

## Style Guide (MUST FOLLOW)

All visual and component work is governed by two documents:

- [Storybook's Foundations stories](http://localhost:6006/?path=/story/foundations-style-guide--core) — **FUNK**, the design system, drawn with the app's real
  components and live tokens (`mike-stack-storybook`). The only source for any visual decision.
  [`docs/frontend/STYLE_GUIDE.html`](../docs/frontend/STYLE_GUIDE.html) is the design export it started from, reference only.
- [`docs/frontend/CONVENTIONS.md`](../docs/frontend/CONVENTIONS.md) — code conventions: two-tier structure,
  `cn()`, `$props()` shape, `data-slot`, `tailwind-variants`, bits-ui wrapping, `@lucide/svelte`, story rules.

UI work dispatches both `ui-designer` (visual layer) and `frontend-developer` (implementation layer). See the project CLAUDE.md for the full rule.

## Stack

- **SvelteKit 2.x** with `@sveltejs/adapter-static` (SPA mode, `index.html`
  fallback — served by an independent `vite preview` process on `:9990`,
  decoupled from the Rust binary since Phase 2.6.5)
- **Svelte 5 runes** — state lives in `$state`, derived values in `$derived`,
  effects in `$effect`. Module-level state uses `.svelte.ts` files.
- **TypeScript strict** (`strict: true`, `noUncheckedIndexedAccess: true`)
- **Tailwind CSS v4** via `@tailwindcss/postcss`
- **Vitest** for unit tests, **Playwright** for e2e
- **ESLint (flat config) + Prettier**

## Dev loop

Three terminals, three processes — proxy, inspect API (same binary), and the SPA host.

```bash
# terminal 1 — Rust proxy + inspect API (one binary, two binds)
cd ..
./scripts/start-proxy.sh    # proxy on :9988, inspect API on :9989

# terminal 2 — SPA dev server with hot reload
cd frontend
pnpm install
pnpm run dev                 # vite dev server on :5173, proxies /api* to :9989

# (or, for the production-ish loop instead of `pnpm run dev`:)
# terminal 2' — built SPA via vite preview
pnpm run build && pnpm run start   # vite preview on :9990
```

Open <http://localhost:5173> for the dev loop (hot reload) or <http://localhost:9990>
for the built SPA. Either way, vite proxies `/api/*` and `/api/ws` to
`http://127.0.0.1:9989` (the Rust inspect bind). The SPA never talks to the proxy
bind on `:9988` — that's the Claude API surface, used only by Claude Code:

```bash
# terminal 3 (optional) — exercise the proxy with a real Claude Code session
ANTHROPIC_BASE_URL=http://localhost:9988 claude
```

To point vite at a different inspect backend (e.g. when running the proxy on
another machine):

```bash
INSPECT_BACKEND=http://proxy-host.example.com:9989 pnpm run dev
```

## Scripts

| Script           | What it does                                            |
| ---------------- | ------------------------------------------------------- |
| `pnpm run dev`    | Vite dev server on :5173 with `/api*` proxy to :9989    |
| `pnpm run build`  | Build static SPA into `build/` (adapter-static)         |
| `pnpm run start`  | Serve `build/` via vite preview on :9990                |
| `pnpm run preview`| Alias for `pnpm run start` (vite default behavior)       |
| `pnpm run check`  | `svelte-kit sync` + `svelte-check` + `storybook:test`   |
| `pnpm run test`   | Run Vitest unit project                                 |
| `pnpm run storybook`       | Start Storybook dev server on :6006 (the stack already runs it as `mike-stack-storybook`) |
| `pnpm run storybook:build` | Build static Storybook                         |
| `pnpm run storybook:test`  | Run Storybook a11y + interaction tests (gates `check`) |
| `pnpm run lint`   | ESLint over `src/`                                      |
| `pnpm run format` | Prettier write                                          |

**Component story policy**: every new reusable component under `src/lib/components/` MUST ship with a sibling `*.stories.svelte` file — the Storybook harness runs axe-core under vitest (`a11y: { test: 'error' }`) and fails `pnpm run check` on violations. See `docs/style/component-patterns.md §13`.

## shadcn-svelte primitives (local, not a dependency)

The primitives under `src/lib/components/ui/` are **generated locally** via `pnpm exec shadcn-svelte@latest add <name>` and committed to this repo as regular source. CI never contacts the shadcn registry; `package.json`'s `scripts` contain no `shadcn-svelte add` invocation. Any addition or modification of a primitive goes through a normal PR with review — same bar as any other component. Runtime deps that the primitives rely on (`bits-ui`, `clsx`, `tailwind-merge`, `tailwind-variants`, `@lucide/svelte`) are exact-pinned in `package.json` (phase 2.8 S7); bump them intentionally, not as a side effect of `pnpm install`.

## Project layout

```
frontend/
├── src/
│   ├── app.css              # Tailwind entry + body styles
│   ├── app.html             # SvelteKit shell
│   ├── app.d.ts             # App namespace types
│   ├── lib/
│   │   ├── types.ts         # Wire types (mirror of Rust structs)
│   │   ├── api.ts           # Typed REST client
│   │   └── ws.svelte.ts     # Svelte 5 runes-based WebSocket store
│   └── routes/
│       ├── +layout.ts       # SPA mode (ssr = false, prerender = false)
│       ├── +layout.svelte   # Header + connection indicator
│       └── +page.svelte     # Placeholder home page + debug panel
├── static/
│   └── favicon.svg
├── svelte.config.js         # adapter-static + fallback
├── vite.config.ts           # dev + preview proxy /api, /api/ws → :9989
├── tsconfig.json            # strict TS
├── eslint.config.js         # flat config
├── postcss.config.js        # tailwindcss/postcss plugin
├── playwright.config.ts     # e2e scaffold (no tests yet)
└── README.md
```

## Build output

```bash
pnpm run build
```

produces a fully static site in `frontend/build/`:

- `index.html` — the SPA fallback (all routes resolve to this)
- `_app/` — hashed JS/CSS chunks
- `favicon.svg`, etc.

Served by a separate `vite preview` process on `:9990` (see
`scripts/start-proxy.sh`) — never embedded in the Rust binary. The
Phase 2.3 `rust-embed` approach was rolled back in Phase 2.6.5.

## Wire types

`src/lib/types.ts` is the source of truth for what the REST and WebSocket
endpoints return. When the Rust types in
`crates/claude-proxy/src/api/handlers.rs`, `crates/claude-proxy/src/store/models.rs`,
or `crates/claude-proxy/src/types.rs` change, update this file to match.

Key gotchas derived from the Rust code:

- `RequestSummary.status` is a plain `string` (Rust `Display` impl), so an
  error looks like `"Error: <msg>"` — not a tagged object.
- `RequestDetail.status` is the raw `RequestStatus` enum which becomes either
  `"Pending" | "Streaming" | "Complete"` or `{ "Error": "<msg>" }`.
- `RequestDetail.has_request_body` / `has_response_body` are booleans (not the
  raw bytes) via a custom serializer on the Rust side.
- `ProxyEvent` frames on the WebSocket are tagged with a top-level `"type"`
  field; `Lagged` is synthetic (sent by `api/ws.rs`) and carries `skipped`
  instead of `id`.

## What's shipped

- **Request list, request detail, real-time streaming updates** — the original Phase 2 delivery
- **Process split** — the embedded `rust-embed` approach was rolled back in favour of an independent `vite preview` process so UI changes never require `cargo build` or a proxy restart
- **Session grouping** — `/sessions/[id]` and `/sessions/[id]/requests/[requestId]` routes, `sessions.svelte.ts` store, `SessionRow` component, unified around a single `derive_session_id(headers, body)` key
- **Analysis dashboard** — `/analysis` with `DiagnosticsCard` on request detail
- **Compose sandbox** — `ComposeModal` + `ComposeResultCard` on the home page (gated behind `MIKE_ALLOW_COMPOSE=1` on the backend)

Remaining:

- **2.8** — polish + responsive design (UI-only)
