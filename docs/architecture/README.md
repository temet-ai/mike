---
title: Architecture model
status: active
created: 2026-10-01
updated: 2026-10-01
summary: One model of mike written in LikeC4, the diagrams drawn from it, how to change it, and how to plan a change against it.
---

# Architecture model

This folder holds one description of mike, written as code in [LikeC4](https://likec4.dev/).
Every diagram is a view of that one description. That gives three things a folder of
hand-drawn pictures cannot:

- **Rename or move a part once.** Every diagram that shows it follows.
- **Plan against what exists.** A proposed change is a small file that adds to the real
  model, so the plan is drawn on the real system and cannot refer to a part that is not
  there.
- **Ask the model questions.** "What writes to the mailbox schema?", "what sends content
  off this machine?" and "what is on the path between a request and Anthropic?" are
  queries, for a person in the browser and for a session through the MCP server.

## Looking at it

| To | Run |
|---|---|
| Browse every diagram, with live reload while you edit | Open <http://localhost:9986/>. `./scripts/start-proxy.sh` and every deploy keep it running as the systemd user unit `mike-stack-arch` (log in `.deploy/arch.log`), and `./scripts/stop-proxy.sh` stops it. To run one by hand on another port: `scripts/arch serve --port N` |
| Get pictures a session can read | `scripts/arch png` writes one PNG per view to `docs/architecture/out/png/`; name views to render only those |
| Build the browsable site as static files | `scripts/arch site` |
| List the views | `scripts/arch views` |
| Check the model is valid and still true of the code | `scripts/arch check` |

`scripts/arch` is the only way LikeC4 is run here. It uses the version pinned in
`scripts/likec4/` and installs it on first use. Do not run `npx likec4` or
`pnpm dlx likec4` in this repository: the wrapper keeps model-provider keys away from
LikeC4 and keeps its listeners on this machine, and those commands do neither (see
[Keys and listeners](#keys-and-listeners)).

## The views

| View | Shows |
|---|---|
| `index` | mike among the people and services around it |
| `processes` | the seven things that run, and what connects them |
| `server` | every part of the server process |
| `requestPath` | what a request and its reply pass through |
| `pipeline` | the eighteen stages of `STAGE_ORDER`, with the injected-block replay before them and the composer after, in order, with their gates |
| `inspection` | how a captured request reaches the web UI and other sessions |
| `judging` · `lessons` · `steering` | the three parts of the loop that turns one session's outcome into the next session's instructions |
| `coordination` | the roster, the mailbox, launching, fleets and the change record |
| `data` · `captureWrites` | the database's schemas, and what one captured request writes |
| `leavesTheMachine` | every connection that carries content off this machine |
| `machine` | which supervisor runs each process, and on which port |
| `flowRequest` · `flowLiveUpdate` · `flowCut` · `flowMail` · `flowLaunch` · `flowImprovement` | step-by-step sequences |

## How it is laid out

| File | Holds |
|---|---|
| `specification.c4` | the vocabulary: kinds of element, kinds of relationship, tags |
| `model/context.c4` | people, outside services, and mike's processes |
| `model/server.c4` | the inside of the server process |
| `model/pipeline.c4` | the request pipeline's stages, in order |
| `model/workers.c4` | background work: lessons, judging, steering, coordination, housekeeping |
| `model/data.c4` | one element per Postgres schema |
| `deployment.c4` | where each process runs |
| `global.c4` | styles every view shares |
| `views/` | the diagrams |

Tags carry the facts a reader most often wants at a glance: `#hot-path`,
`#mutates-request`, `#mutates-response`, `#default-off`, `#default-on`,
`#sends-content-out` and `#planned`. A stage or worker also carries its gate and its
default as metadata, taken from the tables in [`REQUEST_FLOW.md`](../REQUEST_FLOW.md)
and [`STARTUP_FLOW.md`](../STARTUP_FLOW.md).

## Changing it

Edit the `.c4` files, then run `scripts/arch check`. Change the model in the same commit
as the code it describes.

`scripts/arch check` runs in `make lint`. It fails when:

- the model does not parse, or a view refers to something that is not in it;
- a file is not formatted (`scripts/arch fmt` fixes it);
- an element links to a path in this repository that no longer exists;
- the pipeline's stages differ from `STAGE_ORDER` in `crates/mike/src/pipeline/mod.rs`;
- the schemas differ from the ones the migrations create.

Everything else in the model is checked by reading it. The prose reference is still
[`ARCHITECTURE.md`](../ARCHITECTURE.md); where the two disagree, the code decides and
both are fixed.

## For a session: the MCP server

LikeC4 has an MCP server with twenty read-only tools over this model: search, read an
element, find the relationships or the paths between two elements, walk everything
upstream or downstream of one. It is not registered for every session, because each
session that loads it starts a 250 MB process; `docs/architecture/likec4.mcp.json` is
the one-server MCP config that starts it over stdio through `scripts/arch mcp`. Start a
session with it when the work is about architecture:

```
claude --mcp-config docs/architecture/likec4.mcp.json
```

Without it a session still has everything: the `.c4` files are text, and
`scripts/arch likec4 export json` writes the whole model as JSON.

## Keys and listeners

LikeC4's preview server switches on an AI chat, and installs a package for it, as soon
as it finds `OPENAI_API_KEY`, `OPENROUTER_API_KEY`, `ANTHROPIC_API_KEY`,
`GEMINI_API_KEY`, `OLLAMA_HOST` or `MINIMAX_API_KEY` in its environment. `scripts/arch`
removes those from LikeC4's environment before it starts, so the chat stays off and no
key is used.

Two of LikeC4's listeners open on every network interface when it is run directly, and
neither asks who is connecting. Its HTTP transport for MCP is one, so `scripts/arch mcp`
offers stdio only. The live-reload socket of its preview is the other, so
`scripts/arch serve` puts that socket on the page's own port, where it shares the page's
loopback listener. The preview is then one listener, on `127.0.0.1`.
