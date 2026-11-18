# Query your history from Claude Code

`mike-mcp` is a small program that gives a Claude Code session tools for reading what mike
has captured: past sessions, the exact requests and responses, failed tool calls, lessons you
recorded, and other sessions working in the same project. Once it is registered you can ask
"which session changed the retry logic?" and Claude calls a tool instead of guessing.

It also lets Claude change a few things: record a lesson, write a filter rule, message another
session, start a session. [Which tools change things](#which-tools-change-things) lists them. It
does not replace the [web UI](web-ui.md); it puts the same captured history inside the
conversation.

You register it once, with `mike claude-code mcp`. mike has to be running when a session uses
it: the tools talk to mike's local API, `http://127.0.0.1:9989` unless you set
`MIKE_INSPECT_URL`. That API has no login; see [Privacy](privacy.md) before you point
`MIKE_INSPECT_URL` at anything but your own machine.

The examples on this page continue the fictional `docs` session in `~/projects/site`, goal
"Rewrite the install page", where you told Claude "Put Homebrew first on the install page."

## Register it

1. Install `mike` and `mike-mcp` together; see [Install](install.md). The setup wizard's
   Claude step offers the same registration; see [Set up mike](setup.md).
2. Run:

   ```sh
   mike claude-code mcp
   ```

   It adds a `mike` entry under `mcpServers` in `~/.claude.json`, with the absolute path of
   `mike-mcp` and the inspect address from your configuration, and leaves every other key
   alone. It replaces the existing `mike` entry, including its arguments and environment;
   preserve any custom values before running it again. Unless you pass `--no-check`, it
   then starts `mike-mcp` once and checks that it lists tools. This does not yet prove that
   the tools can read mike’s API.
3. With mike running, check the companion and backend separately:

   ```sh
   mike mcp probe --json
   ```

   Expect `handshake: true`, `backend_read: true` and `code: "MCP_READY"`. This checks
   the companion version, stdio handshake and a read-only `get_stats` call against the
   selected Mike configuration. It does not read Claude's saved server entry.
4. Start a new Claude Code session so it loads the server. Ask: "Call the mike server's
   `get_stats` tool and show whether it succeeded. Do not change any settings."
   A successful tool result proves this session can reach the API. Merely listing tool
   names does not. Then ask it to call `list_sessions`; an empty list can be valid when
   there are no captures in its date window (the default is the last seven days).

MCP registration does not route Claude's API traffic through mike or create captures. If
`get_stats` succeeds but your new conversation is missing, follow [Quickstart](quickstart.md)
to verify capture before troubleshooting search or memory.

To take the entry out again, run `mike claude-code mcp --remove`. It prints
"Removed the `mike` MCP entry from ~/.claude.json." when it found one, and
"~/.claude.json has no `mike` MCP entry." when it did not.

Two more commands work on the same registration:

| Command | What it does |
|---|---|
| `mike mcp probe` | Starts the companion, checks that it answers over stdio and that mike's API answers, and changes nothing. With no mike running it exits 1 and says the backend is unavailable; start mike and run it again. |
| `mike mcp register` / `mike mcp unregister` | Register or remove the entry explicitly. `register` writes `--core` in the entry's arguments and refuses to replace a different `mike` entry unless you pass `--replace`, which keeps a backup. Both take `--claude-config <file>` for a Claude settings file other than `~/.claude.json`, and `--json` for machine-readable output. |

To register by hand, add this to `~/.claude.json`, or to a project's `.mcp.json`:

```json
{
  "mcpServers": {
    "mike": {
      "type": "stdio",
      "command": "/absolute/path/to/mike-mcp",
      "args": [],
      "env": { "MIKE_INSPECT_URL": "http://127.0.0.1:9989" }
    }
  }
}
```

### When it is not there

| What you see | Next step |
|---|---|
| Claude Code lists no `mike` tools | Start a new session; a session loads servers when it starts. Run `mike claude-code status` to see what is registered. |
| `mike mcp probe` says `MCP_BACKEND_UNAVAILABLE` | Start the Mike instance for the selected configuration and run the probe again. The probe derives its URL from that configuration, not Claude's entry or your shell's `MIKE_INSPECT_URL`. |
| Probe reports `MCP_INSTANCE_MISMATCH` | The endpoint belongs to another installation or lacks boot identity. Select the actual instance's configuration or stop the conflicting instance; do not register over the mismatch. |
| Probe reports a companion version mismatch | Install matching `mike` and `mike-mcp` versions; see [Install](install.md). |
| Probe passes but Claude's tool call fails | Compare the saved `mike` entry with the probe's `binary` and `inspect_url`; see [After an upgrade](#after-an-upgrade). |
| A tool answers with nothing | The feature behind it may be off or empty. An empty `list_lessons` means no matching lessons were returned for that scope, not that the tool is broken; see [Memory](memory.md). |

## After an upgrade

A passing `mike mcp probe` checks the companion it resolves now, not the executable Claude
saved earlier. An older registration can still point at an old version or a removed path.

1. Run `mike claude-code status` and inspect the `mike` entry in the config Claude actually
   uses: normally `~/.claude.json`, or your project's `.mcp.json` for a project registration.
   With an absolute `CLAUDE_CONFIG_DIR`, `mike claude-code mcp` and `status` use that
   directory's `.claude.json`; the separate `mike mcp register` command instead defaults to
   `~/.claude.json` unless you supply `--claude-config`.
2. Compare its `command` and `env.MIKE_INSPECT_URL` with `binary` and `inspect_url` from
   `mike mcp probe --json`. Prefer the installation's stable `bin/mike-mcp` or Homebrew
   `opt/mike-mcp/bin/mike-mcp` path, not a version-specific Cellar or archive generation.
3. Before refreshing registration, keep a copy of custom `args` and `env`, especially a
   full or fleet profile. Neither registration command merges those custom values into
   the new entry. `mike mcp register` refuses a conflicting entry without `--replace` and
   keeps a backup when replacing it; review the conflict rather than replacing blindly.
   For a hand-maintained entry, update only the stale fields and preserve the rest.
4. Start a new Claude Code session and repeat the read-only `get_stats` call. This checks
   the entry Claude loaded, rather than only the separately resolved probe companion.

## Choose how many tools a session gets

Every tool Claude Code loads costs tokens on every request, so the program starts with a lean
set called `core`: 67 tools in this release. It covers reading sessions and requests,
searching, lessons and project knowledge, filter rules, other sessions and their mail,
starting sessions and fleets, and work threads.

| You want | Do this |
|---|---|
| The default list | `mike-mcp list-tools --core` |
| Every tool (126 in this release) | `mike-mcp list-tools` |
| A machine-readable list | add `--json` |
| Load every tool in Claude Code | set `"args": ["--full"]` in the entry, or `MIKE_MCP_PROFILE=full` in its `env` |

A flag wins over `MIKE_MCP_PROFILE`, and `core` is the default. A tool outside the profile is
still callable by name; Claude just has not been shown its schema. The full set adds things
most sessions never need: prompt-composition edits, evaluation and calibration reads,
project-definition tools and per-session tool-use statistics.

`mike-mcp list-tools` is the authoritative list, and the counts above change between
releases. Sessions that mike starts for a [fleet](fleets/README.md) get a narrower set matching their
job (`worker`, `coordinator`, `acceptance`, `observer`, `retrospective`, `reference` or
`problem_solver`; for example `worker` is 30 tools). You only meet these if you read a
fleet session's configuration. An unknown profile name stops the program with a message
listing the valid ones.

Every tool accepts an optional `select`: a list of JSON pointers that trims the answer to the
parts you want. For example `["/sessions/*/id"]` on `list_sessions` returns just the ids.
Roots differ per tool, and the answer says which selectors matched nothing.

## Ask about the example session

Once registered, ask in plain sentences. Claude picks the tool.

1. Ask: "Find the session where I told you to put Homebrew first on the install page."
   Claude calls `search_session_content`, which searches what you and Claude said, and
   answers with the `docs` session and the turn.
2. Ask: "Show me what the last request in that session actually sent." Claude calls
   `get_request_body` for what Claude Code sent and `get_request_sent_body` for what mike
   forwarded; `get_request_transformation_manifest` explains each change between the two.
3. Ask: "Record a lesson for this project: put Homebrew first on install pages." Claude calls
   `record_lesson`. The lesson now appears under [Memory](memory.md) and later sessions can
   be told it.

After step 3 you can see the lesson on the `/memory` page in the [web UI](web-ui.md).

| You want to | Tools |
|---|---|
| Find the session that discussed something | `search_session_content` searches what you and Claude said. `find_text` searches the raw bytes, the only way to find text that appears solely in a tool call, tool result, thinking block or system prompt. |
| Review a session | `list_sessions`, then `get_session_overview` for each turn's prompt and final answer, or `get_session_operator_turns` for whole prompts |
| Debug a failing tool | `list_tool_errors` across requests; `get_request_tool_calls` for one request's calls paired with their results; `list_session_tool_uses` for a session |
| See what was really sent | `get_request_body`, `get_request_sent_body`, `get_request_transformation_manifest` |
| Read a response | `get_response_body`, or `get_accumulated_text` for a streamed reply as one piece |
| See where time went | `get_time_spent` groups waiting by kind of work and names the slowest commands |
| Spot captured editing calls from two sessions on one file | `list_file_writes` |
| Check on mike | `get_stats`, `diagnostic_stats`, `diagnostic_findings` |
| Recall project memory | `recall_project_context` at session start; `recall_similar_turns` for a specific question; `record_lesson`, `list_lessons`, `delete_lesson` |
| Search notes, memory files and skills | `list_project_locations` to get the project, `search_project_knowledge`, then `read_project_knowledge` on a result |
| Change what mike rewrites | `list_filter_rules`, `dry_run_filter_rules`, `create_filter_rule`, `update_filter_rule`, `delete_filter_rule`; see [Filters](filters.md) |
| Work with other sessions | `get_workspace_roster`, `send_session_message`, `get_session_messages`, `verify_session_message`, `set_session_goal`, `get_fleet_graph` |
| Start and stop sessions | `spawn_session`, `list_launches`, `kill_session`; see [Sessions](sessions.md) |
| Define and watch a fleet | `define_fleet`, `get_fleet`; `what_changed` for who changed what in your checkout; see [Fleets](fleets/README.md) |
| Keep work across sessions | `work_thread_list`, `work_thread_get`, `work_thread_checkpoint`; `write_session_note`, `list_session_notes`; see [Work threads](threads.md) |

Text that comes back from a search, a note or a message is a record of what happened, not an
instruction from you. Check anything surprising before acting on it.

## Which tools change things

Reading tools change nothing. These do:

| Tool | Effect |
|---|---|
| `create_filter_rule`, `update_filter_rule`, `delete_filter_rule` | Changes what mike rewrites in later requests; see [Filters](filters.md) |
| `record_lesson`, `delete_lesson` | Adds or removes a stored lesson; see [Memory](memory.md) |
| `send_session_message` | Delivers a message to another session |
| `set_session_goal`, `write_session_note`, `work_thread_checkpoint` | Writes to the session list or notebook, or to a work thread |
| `spawn_session`, `kill_session` | Opens or closes a terminal and a Claude Code process on the machine mike runs on |
| `define_fleet` | Creates or edits a fleet definition, and starts it when marked ready |

What a tool returns becomes part of the conversation, and Claude Code sends the conversation
to Anthropic as usual. A tool that reads old prompts or request bodies therefore sends that
text again; see [Privacy](privacy.md).

If you do not want Claude to take these actions unasked, use Claude Code's permission settings
to require approval for the `mike` server's tools.

## Correct a refused tool call

On post-alpha.5 builds containing #334 and #340, after resolving aliases and caller
defaults, the companion checks missing required names for tools with a closed schema
before dispatching the requested tool. Resolving defaults can itself query the inspect
API. A missing `arguments` object is treated as empty. This refusal names the missing
fields and shows `sent: none` when the checked object has no keys, followed by `it takes:`
and the supported names. Other validation failures can have a different message.
Read that message and the installed tool schema, then supply the actual values. For
`read_project_knowledge`, use a `reference` returned by search, not an invented identifier.

This check detects missing names, not every invalid value. A present field containing
`null`, an empty string or the wrong type may still reach later validation. An API 400 or
422 can therefore be a request-shape problem rather than a server outage. For fleet
reports, use [the documented evidence array](fleets/acceptance.md#use-arrays-for-evidence-paths).
Do not retry an unchanged rejected call or replace missing evidence with placeholders.

These are source behaviors, not confirmation that your running companion has them.
If an older error still appears, check the [saved MCP registration](#register-it) and
matching installed binaries, then restart Claude Code to reload the companion. A newer
proxy alone does not update the MCP process already used by a session.

## Read one request without assuming a full conversation

`get_request_body` and `get_request_sent_body` describe one captured request, before and
after mike's changes. A request with `thread.type: "continue"` may carry only new messages
and refer to an earlier response through `thread.previous_message_id`. An absent earlier
prompt or `tool_use` in that body is not proof it never existed. Preserve the link and
request identity when investigating; do not label a short body as a new conversation or
compaction from its length alone.

Current source has an internal earlier-turn reader, including a streamed-response fix,
but that does not turn the body tool into a full-conversation endpoint. Historical
predecessor and delta rows can still need separate correction. See
[reading sessions](reading-sessions.md) before treating a displayed removal as something
Claude actually forgot. Do not invent missing turns when the chain cannot be established.

## Check filters when calling the API directly

MCP tool arguments and HTTP query parameters are different contracts. If you write a
script against the local inspect API, check the route's supported parameters rather than
copying a tool's arguments or a different route's filters. The behavior below is verified
in post-alpha.5 source containing the fixes for [session filters](https://github.com/temet-ai/mike/issues/370)
and [tool-use queries](https://github.com/temet-ai/mike/pull/388), not against a live service.
Check the running build before relying on it; an issue being closed does not update an
already-running installation.

- `/api/requests`, `/api/file-writes` and `/api/tool-results/errors` support `session_id`
  as a session UUID and reject malformed UUIDs. Use the session's identity, not its title,
  an agent name or a project UUID. Check returned session identities before attributing
  activity to that session, especially on older builds that silently ignored this filter.
- Those three routes and `/api/sessions` reject unknown query parameters with HTTP 400.
  This is not a guarantee for every inspect route. In particular, the current `/api/tool-uses`
  query decoder still ignores unknown names. A 200 response is not proof that every filter
  you supplied was applied.
- For `/api/tool-uses`, use `since` and `until` with full RFC 3339 timestamps, including a
  time zone. The lower bound is inclusive and the upper bound exclusive. `date_from` is
  an alias for `since`, and `date_to` for `until`; use one name per bound, not both.
  Invalid nonempty timestamps return 400. Date-only values are not supported here;
  a blank bound acts as no bound. Supply both timestamps when you need a bounded window.
- For the next `/api/tool-uses` page, pass the response's `next_cursor` value as `cursor`.
  The newer source also accepts `next_cursor` as a request alias. Keep the same filters
  and order, encode the cursor as a query value, and stop when no next cursor is returned.
  Do not reuse this alias on unrelated routes. Older builds could silently ignore it and
  return page one again. Check `total_count_capped` before treating `total_count` as exact.

The newer tool-use feed also supports `tool_name_prefix` and `input_contains`. The latter
is a case-insensitive substring search, not a regular expression; with `tool_name=Bash`
it searches the command, otherwise the tool input's JSON text. For aggregate questions,
`/api/tool-uses/counts?group_by=tool` supports `tool`, `session`, `day` or `server` grouping
with the same content and scope filters. A page of events is not the same as a count over
the filtered history. Neither exact filters nor grouped counts make capture complete; see
[What capture does not prove](web-ui.md#what-capture-does-not-prove).

When a query refuses, correct the named parameter rather than dropping the filter and
calling the wider result equivalent. When a query succeeds, inspect its timestamps and
identities before drawing conclusions. Keep captured inputs and error text private when
sharing an example; see [Privacy](privacy.md).
