# mike frontend

The Svelte SPA for inspecting traffic through mike: requests, sessions, fleets,
steering, memory, settings and the rest of the pages under `src/routes/`. It talks
only to the inspect API (`/api/*`, `/api/ws`).

Two ways serve it:

- **Development stack** (`scripts/start-proxy.sh`): an independent `vite dev` process
  on `:9990` (`mike-stack-spa`), with `mike` started with `MIKE_SPA_ENABLED=0`. A UI
  change never touches the proxy.
- **Installed or plain `mike` launch**: `mike` serves `frontend/build/` itself on the
  SPA bind, embedded into the binary with `rust-embed` (`crates/mike/src/spa.rs`). Run
  `make spa` (or `pnpm build` here) before building `mike`, or the SPA bind answers 503.

## Style guide

All visual and component work is governed by two documents:

- Storybook's Foundations stories (`src/lib/components/__foundations__/`, served by
  `pnpm run storybook` on `:6006`) are **FUNK**, the design system, drawn with the
  app's real components and live tokens. They are the only source for a visual
  decision. [`docs/frontend/STYLE_GUIDE.html`](../docs/frontend/STYLE_GUIDE.html) is
  the design export they started from, reference only (open it from a checkout;
  GitHub shows HTML as source).
- [`docs/frontend/CONVENTIONS.md`](../docs/frontend/CONVENTIONS.md) covers code
  conventions: two-tier structure, `cn()`, `$props()` shape, `data-slot`,
  `tailwind-variants`, bits-ui wrapping, `@lucide/svelte` and story rules.

## Stack

- **SvelteKit 2.x** with `@sveltejs/adapter-static` (SPA mode, `index.html`
  fallback)
- **Svelte 5 runes** — state lives in `$state`, derived values in `$derived`,
  effects in `$effect`. Module-level state uses `.svelte.ts` files.
- **TypeScript strict** (`strict: true`, `noUncheckedIndexedAccess: true`)
- **Tailwind CSS v4** via `@tailwindcss/vite`
- **Vitest** for unit tests, **Playwright** for e2e
- **ESLint (flat config) + Prettier**

## Dev loop

```bash
# terminal 1 - mike (proxy :9988, inspect API :9989) and the rest of the stack
cd ..
./scripts/start-proxy.sh

# terminal 2 - only if you want the SPA outside systemd
cd frontend
pnpm install
pnpm run dev                 # vite dev server with hot reload on :9990
```

`scripts/start-proxy.sh` already runs the SPA as the `mike-stack-spa` systemd user unit
(`pnpm run start`, which is `vite dev`). Open <http://localhost:9990>. Vite proxies
`/api/assistant` to the prompt assistant (`PROMPT_ASSISTANT_BACKEND`, default
`http://127.0.0.1:9985`), and `/api/ws` and the rest of `/api` to the inspect bind
(`INSPECT_BACKEND`, default `http://127.0.0.1:9989`). The SPA never talks to the proxy
bind on `:9988`; that is the Claude API surface, used only by Claude Code:

```bash
# optional - exercise the proxy with a real Claude Code session
ANTHROPIC_BASE_URL=http://localhost:9988 claude
```

`MIKE_FRONTEND_PORT` and `MIKE_FRONTEND_HOST` change the vite bind (defaults `9990`,
`127.0.0.1`); to point vite at another inspect backend:

```bash
INSPECT_BACKEND=http://inspect-host.example.com:9989 pnpm run dev
```

## Scripts

| Script                            | What it does                                                                  |
| --------------------------------- | ----------------------------------------------------------------------------- |
| `pnpm run dev` / `pnpm run start` | Vite dev server with HMR (`:9990`) and the `/api` proxies                     |
| `pnpm run build`                  | Build the static SPA into `build/` (adapter-static)                           |
| `pnpm run preview`                | `vite preview` of `build/`                                                    |
| `pnpm run check`                  | `../scripts/sb-test`: type-check, unit and Storybook tests                    |
| `pnpm run test`                   | Unit tests only, through `../scripts/sb-test --unit`                          |
| `pnpm run storybook`              | Storybook dev server on `:6006` (the stack runs it as `mike-stack-storybook`) |
| `pnpm run storybook:build`        | Build static Storybook                                                        |
| `pnpm run storybook:test`         | Storybook a11y and interaction tests                                          |
| `pnpm run lint`                   | ESLint                                                                        |
| `pnpm run format`                 | Prettier write                                                                |

Run frontend tests through `scripts/sb-test` (for example
`scripts/sb-test --skip-typecheck <filter>`), not raw `vitest`.

**Component story policy**: every new reusable component under `src/lib/components/` MUST ship with a sibling `*.stories.svelte` file — the Storybook harness runs axe-core under vitest (`a11y: { test: 'error' }`) and fails `pnpm run check` on violations. `scripts/component-index <word>` lists the component that implements a pattern.

## shadcn-svelte primitives (local, not a dependency)

The primitives under `src/lib/components/ui/` are **generated locally** via `pnpm exec shadcn-svelte@latest add <name>` and committed to this repo as regular source. CI never contacts the shadcn registry; `package.json`'s `scripts` contain no `shadcn-svelte add` invocation. Any addition or modification of a primitive goes through a normal PR with review — same bar as any other component. Runtime deps that the primitives rely on (`bits-ui`, `clsx`, `tailwind-merge`, `tailwind-variants`, `@lucide/svelte`) are exact-pinned in `package.json` ; bump them intentionally, not as a side effect of `pnpm install`.

## Project layout

```
frontend/
├── src/
│   ├── app.css              # Tailwind entry, FUNK tokens
│   ├── app.html             # SvelteKit shell
│   ├── lib/
│   │   ├── types.ts         # Wire types (mirror of the Rust structs)
│   │   ├── api.ts, api/     # Typed REST client
│   │   ├── ws.svelte.ts     # Runes-based WebSocket store
│   │   ├── stores/          # Shared client state
│   │   └── components/      # ui/ primitives, __foundations__/ stories, page components
│   └── routes/              # One folder per page (sessions, fleet, settings, ...)
├── e2e/                     # Playwright journeys
├── static/
├── vite.config.ts           # SvelteKit 3 config (adapter-static + index.html fallback) and dev + preview proxy: /api/assistant, /api/ws, /api
└── README.md
```

## Build output

```bash
pnpm run build
```

produces a fully static site in `frontend/build/`:

- `index.html` - the SPA fallback (all routes resolve to this)
- `_app/` - hashed JS/CSS chunks
- `favicon.png`, etc.

`mike` embeds this folder at compile time (`crates/mike/src/spa.rs`); the development
stack ignores it and runs `vite dev` instead.

## Wire types

`src/lib/types.ts` is the source of truth for what the REST and WebSocket
endpoints return. A few DTOs (the session-flow page, request and session summaries and their parts) are generated
into `src/lib/api/generated/` by `cargo test -p mike --features ts-export export_bindings`
(`scripts/check-generated-types.sh` checks they are current); do not hand-edit those files.
When the Rust types in
`crates/mike/src/api/`, `crates/mike/src/store/models/` or `crates/mike/src/types.rs`
change, update this file to match.

Key gotchas derived from the Rust code:

- `RequestSummary.status` is a plain `string` (Rust `Display` impl), so an
  error looks like `"Error: <msg>"` — not a tagged object.
- `RequestDetail.status` is the raw `RequestStatus` enum
  (`crates/mike/src/store/models/request_status.rs`), which serializes as either
  `"Pending" | "Streaming" | "Complete"` or `{ "Error": "<msg>" }`.
- `RequestDetail.has_request_body` / `has_response_body` are booleans (not the
  raw bytes) via a custom serializer on the Rust side.
- `ProxyEvent` frames on the WebSocket are tagged with a top-level `"type"`
  field; `Lagged` is synthetic (sent by `api/ws.rs`) and carries `skipped`
  instead of `id`.
