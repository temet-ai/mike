---
title: Using mike MCP from a Background Monitor / cron context
status: superseded
created: 2026-06-13
updated: 2026-11-20
summary: A shell process (Monitor, tail -f, cron) has no MCP transport, so it cannot call mike's MCP tools.
superseded_by: ../../AGENTS.md
---

# Using mike MCP from a Background Monitor / cron context

`Monitor`, `tail -f` and cron shell scripts run in a bare shell process. The
`mcp__mike__*` tools exist only inside an agent turn: they are JSON-RPC calls over
the MCP stdio transport that `mike-mcp` serves to Claude Code. A shell script has no
access to that transport, so a call to one from a `Monitor` command does nothing.
Questions about mike go to ask-kit's `ask` tool ([`AGENTS.md`](../../AGENTS.md)).
