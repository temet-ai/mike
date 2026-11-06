---
title: What mike stores and sends
status: active
created: 2026-11-05
updated: 2026-11-06
summary: Reference list of what mike sends off the machine, what it writes and where, and how to remove it.
---

# What mike stores and sends

mike has no telemetry and no update check. Change this file in the same commit as any code
that changes what is stored or sent. [`SECURITY.md`](SECURITY.md) covers the trust boundary
and where keys live; [`CONFIGURATION.md`](CONFIGURATION.md) lists every setting named here.

## What it sends, and where

| To | What | When |
|---|---|---|
| `upstream_url` (`https://api.anthropic.com` by default) | Each request Claude Code sends to the proxy bind, with the caller's own credential header, after mike's configured rewrites | Every proxied request |
| `openrouter.ai` | Captured conversation text, for the steerer, session judge, judge worker, `mike eval run` judge runs, turn guard and lesson-relation detection | Only when an OpenRouter key is set and the feature is on; all are off by default |
| The configured embedding URL | Text to embed, for semantic recall | Only when `embedding.enabled` is on. A non-loopback URL is refused unless `semantic.allow_remote_embedding` is on |
| The `claude` or `codex` CLI the prompt assistant starts | Prompts the operator writes there, under that CLI's own login | Only when the prompt assistant is used |
| Docker Hub, the Debian package mirror and `github.com` (the pgvector release tag) | The image pull and local build of the managed database | `mike db up`, `mike database provision` |
| Nothing else | | |

## What it writes, and where

| Place | What | Moved by |
|---|---|---|
| The PostgreSQL database (managed Docker volume, or the one you point mike at) | Every captured request, response, tool call, system prompt and operator turn, with sessions, filter rules, settings, lessons and notes. Headers carrying credentials are not stored. Sessions older than `default_ttl` (365 days) are deleted | `database_url`, the managed database's volume |
| `$XDG_CONFIG_HOME/mike/` (`~/.config/mike/`) | `config.toml` | `XDG_CONFIG_HOME`, `--config` |
| `$XDG_STATE_HOME/mike/` (`~/.local/state/mike/`) | Machine identity, the managed database's Compose files and owner-only password file, `service-token.<port>` (mode 0600) | `XDG_STATE_HOME` |
| `log_dir` / `data_dir` | The daemon's log and pid file | `MIKE_DATA_DIR`, `--data-dir` |
| The sidecar's CA folder (`~/.local/share/proxy-forward/`) | A generated CA certificate and key (key mode 0600), only when the `proxy-forward` sidecar is used | The sidecar's own setting |

## Removing it

Stop mike, then `mike db down` (or `mike database stop`). To remove all data, delete the managed database's
Docker volume and the config and state folders above. [`RECOVERY.md`](RECOVERY.md) covers
backup and restore of the same set.
