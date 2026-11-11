# Find a page

Use this directory to find the guide for the screen or command in front of you. The web UI lives at <http://localhost:9990>; each row gives the menu entry, the route to type after that address and the guide page that explains it.

New to mike? Start with [Capture your first session](quickstart.md), then [get around the web UI](web-ui.md).

## The web UI

The menu on the left groups the pages as Overview, Levers, Insight and System. Pages reached from a link rather than the menu have no menu entry.

### Overview

| Page | Route | Guide |
|---|---|---|
| Home | `/` | [Get around the web UI](web-ui.md) |
| Projects | `/projects`, `/projects/<project>`, `/projects/<project>/graph` | [Projects](projects.md) |
| Sessions | `/sessions` | [Read a session](reading-sessions.md) |
| A session, its flow graph and its messages | `/sessions/<session>`, `/sessions/<session>/graph`, `/sessions/<session>/comms` | [Read a session](reading-sessions.md) |
| A request | `/sessions/<session>/requests/<request>` | [Read a session](reading-sessions.md) |
| Comms | `/comms` | [Read a session](reading-sessions.md) |
| What the judge did | `/sessions/improvement`, and a lesson at `/sessions/improvement/lessons/<lesson>` | [The judge](judge.md) |
| Guard nudges | `/sessions/improvement/guard` | [The judge](judge.md) |
| Judge | `/sessions/labelling` | [The judge](judge.md) |
| Judge accuracy | `/sessions/labelling/accuracy`, a run at `/sessions/labelling/accuracy/<run>` and its review | [The judge](judge.md) |

### Levers

| Page | Route | Guide |
|---|---|---|
| Filters | `/filter/workbench` | [Filter rules](filters.md) |
| Steering Monitor | `/steering`, and a steerer at `/steering/<steerer>` | [Steering](steering.md) |
| Steerers | `/steerers` (opens the Steering Monitor's fleet tab) | [Steering](steering.md) |

### Insight

| Page | Route | Guide |
|---|---|---|
| Flagged responses | `/analysis` | [Flagged responses](flagged-responses.md) |
| Prompts | `/prompts`, and its main, subagent, volatile-lines and new views | [Prompts, agents and tools](prompts-and-agents.md) |
| Agents | `/agents`, and an agent at `/agents/<agent>` | [Prompts, agents and tools](prompts-and-agents.md) |
| Fleet | `/fleet` | [Fleets](fleets/README.md), [Projects](projects.md) |
| Compression review | `/compression-review` | [Compression](compression.md) |
| Tools | `/tools` (tool errors are on the same page) | [Prompts, agents and tools](prompts-and-agents.md) |
| Agent dispatches | `/diagnostics/agent-dispatches` | [Prompts, agents and tools](prompts-and-agents.md) |

### System

| Page | Route | Guide |
|---|---|---|
| Memory | `/memory` | [Memory](memory.md) |
| Skills | `/skills`, and a skill at `/skills/<skill>` | [Memory](memory.md) |
| Proposals | `/proposals` | [Memory](memory.md) |
| Memory diagnostics | `/diagnostics/memory` | [Troubleshooting](troubleshooting.md) |
| Settings | `/settings` | [Settings](settings.md) |
| Setup | `/settings/setup` | [Set mike up](setup.md) |
| Steering settings | `/settings/steering` | [Steering](steering.md) |
| Workers | `/settings/workers` | [Settings](settings.md) |
| Assistants | `/settings/assistants` | [Settings](settings.md) |
| Embedding | `/settings/embedding` | [Settings](settings.md) |
| Reranker | `/settings/reranker` | [Settings](settings.md) |
| Flagging settings | `/settings/flagging` | [Flagged responses](flagged-responses.md) |
| Integrations | `/settings/integrations` | [Settings](settings.md) |
| Configuration history | `/settings/history` | [Settings](settings.md) |
| Help | `/help`, its guides, glossary and documents | [Get around the web UI](web-ui.md) |

Some older addresses still work and send you on: `/judges` and `/judge-disagreements` go to the judge pages, `/settings/workers/evals` goes to judge accuracy, and `/diagnostics/tool-errors` goes to Tools.

## Commands

Run `mike --help` for the list and `mike <command> --help` for one command. [Command line](command-line.md) groups every command by what you do with it.

| What you do | Commands | Guide |
|---|---|---|
| Set mike up and check it | `mike setup`, `mike doctor`, `mike features`, `mike database`, `mike db`, `mike service` | [Set mike up](setup.md) |
| Start mike and Claude Code | `mike` (in a terminal), `mike serve`, `mike claude` | [Capture your first session](quickstart.md), [The terminal app](terminal.md) |
| Connect Claude Code | `mike mcp`, `mike claude-code` | [MCP](mcp.md), [Mods](mods.md), [Set mike up](setup.md) |
| Start and manage sessions | `mike spawn` | [Start and manage sessions](sessions.md) |
| Run a fleet | `mike fleet` | [Fleets](fleets/README.md) |
| Keep work across sessions | `mike thread` | [Work threads](threads.md) |
| Look at configuration | `mike config`, `mike print-config`, `mike import-dotenv` | [Settings](settings.md) |
| Remove mike | `mike uninstall` | [Backup and recovery](backup-and-recovery.md) |
| Everything else: the forwarding sidecar, code discovery, tuning and evaluation, migration checks and the backfill and repair commands | `mike forward`, `mike discover`, `mike eval`, `mike semantic`, `mike guard`, `mike steer-questions`, `mike directives`, `mike migrate-check`, the `backfill-` commands and the rest | [Command line](command-line.md) |

The separate `mike-mcp` program is the MCP companion; see [MCP](mcp.md).

## Settings and reference

[Settings](settings.md) covers the settings sections, the config file and the environment variables. [Privacy](privacy.md) says what mike stores and sends. If a page does not behave as expected, use [Troubleshooting](troubleshooting.md).
