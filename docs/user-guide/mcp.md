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
   alone. Unless you pass `--no-check`, it then starts `mike-mcp` once and checks it answers.
3. Start a new Claude Code session so it loads the server.
4. In that session, ask: "List the tools you have from the mike server." You should see tools
   such as `list_sessions` and `search_session_content`.

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
| `mike mcp probe` says the backend is unavailable | mike is not running at the address in the entry. Start it, or set `MIKE_INSPECT_URL` to where it listens. |
| A tool answers with nothing | The feature behind it may be off or empty. An empty `list_lessons` means no lessons are stored, not that the tool is broken; see [Memory](memory.md). |

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
releases. Sessions that mike starts for a [fleet](fleets.md) get a narrower set matching their
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
| Spot two sessions editing one file | `list_file_writes` |
| Check on mike | `get_stats`, `diagnostic_stats`, `diagnostic_findings` |
| Recall project memory | `recall_project_context` at session start; `recall_similar_turns` for a specific question; `record_lesson`, `list_lessons`, `delete_lesson` |
| Search notes, memory files and skills | `list_project_locations` to get the project, `search_project_knowledge`, then `read_project_knowledge` on a result |
| Change what mike rewrites | `list_filter_rules`, `dry_run_filter_rules`, `create_filter_rule`, `update_filter_rule`, `delete_filter_rule`; see [Filters](filters.md) |
| Work with other sessions | `get_workspace_roster`, `send_session_message`, `get_session_messages`, `verify_session_message`, `set_session_goal`, `get_fleet_graph` |
| Start and stop sessions | `spawn_session`, `list_launches`, `kill_session`; see [Sessions](sessions.md) |
| Define and watch a fleet | `define_fleet`, `get_fleet`; `what_changed` for who changed what in your checkout; see [Fleets](fleets.md) |
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
