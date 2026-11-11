# Start and manage sessions

`mike spawn` opens a new Claude Code session in a terminal pane for any folder, already
connected to mike, with a name, a role and a goal that other sessions can read before the
first turn. It spares you the setup of starting `claude` by hand in each folder. You can
list the sessions mike started, close one, and have sessions send each other messages.
It does not run a group of sessions toward one goal (that is [Fleets](fleets/README.md)), and it
does not read a session's traffic (that is [Reading sessions](reading-sessions.md)).

Run it from any terminal: `mike spawn open <folder>`. The commands below need a terminal
program chosen first, and a running mike to record the launch.

## Choose a terminal first

`mike spawn` needs somewhere to open panes. A fresh install has none chosen, and every
spawn command then refuses and says so (exit code 69). Pick one in the setup wizard's
terminal step (`mike setup`, then see [Setup](setup.md)), or set `launch_backend` in the
config file, or `MIKE_LAUNCH_BACKEND` in the environment:

| Backend | What it is |
|---|---|
| `herdr` | The `herdr` terminal multiplexer. Reports whether each session is idle, working or blocked. |
| `herdr-core` | The Herdr desktop app. Also reports session state. |
| `tmux` | For machines without a display, such as an SSH session. |
| `terminal` | A plain window per session. Hyprland only, because it needs `xdg-terminal-exec` and `hyprctl`. |
| `ghostty` | A Ghostty window per session on macOS. Accepted as a value, but this build has no adapter for it yet: the setup wizard does not offer it, and `mike spawn` refuses as it does with `none`. |
| `none` | The default. Nothing opens. |

When you run `mike spawn` from inside a Herdr or tmux pane, mike opens the new session
beside you in the same kind of terminal, whatever `launch_backend` says. Set
`launch_backend_follow_caller = false` to always use the configured one.

Run `mike spawn doctor` to check the whole chain. It prints one line per check, `PASS` or
`FAIL`, with the fix under each failure: the backend, the forwarding sidecar, the CA
certificate, the `claude` program, the inspect API, the database, whether `mike` is
registered with Claude Code, and when the backend last passed a live test. On a fresh
install it starts like this:

```text
backend: none
FAIL backend chosen — launch_backend is `none`
     remedy: choose a terminal backend: run `mike` and complete the terminal step of setup, or set MIKE_LAUNCH_BACKEND
```

## Start the `docs` session

Continue the example: you want a session named `docs`, a writer, in the `~/projects/site`
folder, working on the install page.

1. Choose a terminal as above, and make sure mike is running (`mike doctor` says if it is not).
2. Write the first message in a file, `brief.md`, for example: "Rewrite the install page.
   Start from the current text."
3. Run:

   ```sh
   mike spawn open ~/projects/site --seat docs --role writer \
     --goal "Rewrite the install page" --prompt-file brief.md
   ```

4. Wait for the command to return. A pane opens beside you in the chosen terminal, Claude
   Code starts in `~/projects/site` and the brief is its first message.
5. Run `mike spawn list`. The `docs` session is on it with its folder and session id.

The folder comes first. Any other word goes to `claude` unchanged, in order, and `--`
passes the rest through untouched.

| Flag | What it does |
|---|---|
| `--session-name <name>` | The name other sessions see. Defaults to `--seat`. |
| `--seat <name>` | A name for this session within a run (see [Fleets](fleets/README.md)). |
| `--role <role>` | What the session is, for example `writer`. Messages can be addressed to a role. |
| `--instructions <text>` | The standing brief the session works under. |
| `--goal <text>` | What it is working on. |
| `--prompt-file <path>` | The first message, from a file, or from standard input with `-`. There is no inline prompt flag. |
| `--tab`, `--split <l\|r\|u\|d>`, `--panel` | Where the pane goes. Tab is the default. A backend that cannot split reports what it did instead. |
| `--trust` / `--no-trust` | Record the folder as trusted in `~/.claude.json` first, or refuse an untrusted folder. |
| `--no-remote-control` | Start without `--remote-control`. |
| `--no-channels` | Start without the message channel that lets other sessions wake this one. |
| `--no-proxy-env` | Start without routing through the forwarding sidecar. |
| `--worktree-ok` | Allow a git worktree outside the repository's `.claude/worktrees/` folder. |
| `--json` | Print one JSON object instead of text. |

A session started this way:

- goes through mike, so its traffic is captured like any other and appears in
  [Reading sessions](reading-sessions.md);
- starts with `--remote-control` and the message channel unless you turn them off;
- gets a session id mike chose before launch, so it is on the roster with its name, role,
  instructions and goal before its first turn. Write `{{session_id}}` in a brief and mike
  fills in the real id.

mike refuses a folder Claude Code does not yet trust, unless you pass `--trust` or run
`mike claude-code trust on` once. It refuses a linked git worktree that is not under
`.claude/worktrees/`, because worktrees belong inside their repository. It warns at 8
open panes and stops at 16 (`launch_warn_surfaces` and `launch_max_surfaces`).

The command waits until the pane is open, the brief is delivered and the Remote Control
channel is found, then returns. Exit codes: `0` done, `1` refused or failed, `2` the
backend cannot do that or the result could not be confirmed, `69` no backend chosen or
the backend is not answering, `127` a program is missing.

A brief over 800 characters is not typed into the pane. mike mails it to the session over
Remote Control and keeps it as the session's note of kind `brief`, so you can read what any
session was told later and from any machine. A file is written only when mike cannot record
the launch: the launch-brief switch is off, the session id is not known, or the database and
the proxy are both out of reach. The file goes under `~/.local/state/mike/briefs/` and the
session is told to read it.

The brief is the session's first message, so it goes to Anthropic like any other
prompt. See [Privacy](privacy.md).

## List and close sessions

```sh
mike spawn list
mike spawn kill --session <uuid>
```

`list` prints a header row, `surface`, `folder`, `session`, `agent_state` and `run/seat`,
then one line for each pane the backend hosts. The state shows only when the backend
reports one. Add `--run <id>` to see one fleet's seats, or `--json` for machine-readable
output. For delivery results, ask Claude to call `list_launches`.

`kill` closes the pane hosting that session and waits up to 20 seconds for the `claude`
process to exit (`launch_kill_wait_secs`). Closing a pane that is already gone does
nothing and exits 0. The process can take ten to twenty seconds to exit after the pane
closes, so a pane missing from `list` does not prove the process has finished.

Close sessions by this command, not by killing processes by name, which can hit other
sessions.

Three more verbs exist for reusing a warmed-up session as a template:

| Verb | What it does |
|---|---|
| `mike spawn warm <folder> --name <template>` | Starts a session like `open` and records it as a named template |
| `mike spawn fork <template or session> …` | Starts a new session that resumes the template's conversation, so it begins with that context loaded |
| `mike spawn templates [show <name>\|delete <name> --yes]` | Lists, shows or removes templates |

`mike spawn reap` lists stray `claude` processes that no pane owns; add `--yes` to signal
them. It never touches the process you run it from or its parents. Without `--yes` it
prints "dry run" and changes nothing.

## Name sessions and give them goals

A session's name, role, instructions and goal make it findable and let a steerer judge
whether it is on task ([Steering](steering.md)). `mike spawn open` sets them at launch. A
running session can set or change its own by calling `set_session_goal`, and anything it
sets itself wins over what the launch seeded. A field you leave out keeps its value.

Ask Claude to call `get_workspace_roster` to see the sessions on a project: name, role,
goal, folder, whether each is alive, and whether mail can reach it. By default it lists
sessions with a current lease, plus any that captured a request in the last 15 minutes;
ask for inactive ones too to see the project's whole history. A role is free text;
sessions with no role cannot be reached by role.

Claude can also start and close sessions itself with `spawn_session`, `list_launches`
and `kill_session` ([MCP](mcp.md)). `spawn_session` takes the same name, role,
instructions and goal.

## Send messages between sessions

Suppose a second session, `reviewer`, reads the install page and wants `docs` to put
Homebrew first. A session can message another one by session id or by role. Ask Claude to
send it, or it calls `send_session_message` with `to` set to a session id or to
`role:writer`. The recipient reads it as `Mail from reviewer (session <id>): <message>`.

How a message arrives, in the order mike prefers:

1. **Message channel.** If the recipient was started with the channel (the default for
   `mike spawn`), an idle session wakes and takes a turn on the message.
2. **Remote Control.** If the session has a Remote Control stream open, mike pushes the
   message into it. This does not change the request sent to Anthropic.
3. **Next request.** Otherwise mike adds the message to the next request the session
   makes. This one does change what is sent to Anthropic.

mike redelivers a message until the recipient acknowledges it. `get_session_messages`
shows a session's inbox and outbox with each message's status: `pending`, `claimed`,
`delivered`, `consumed` or `dead_lettered`. A session reads mail only on its own main
turns, so one that is busy inside a subagent shows as inactive. The web UI shows the same
messages at `/comms` ([Reading sessions](reading-sessions.md)).

A message is a request from another session, not from you. Before acting on an
unexpected message, especially a "done" report or an instruction about a third session,
a session can call `verify_session_message` to confirm that mike delivered it. Text that
only looks like mail inside a request body fails that check.

Messages stay on your machine. The only network hop is the ordinary one, when a message
that arrives in the next request goes to Anthropic with the rest of that request. See
[Privacy](privacy.md).

## When something does not work

| What you see | Why | What to do |
|---|---|---|
| `mike spawn: no terminal backend is configured`, exit 69 | A fresh install has none chosen. | Choose one in `mike setup`'s terminal step, or set `MIKE_LAUNCH_BACKEND`. |
| `mike spawn: ledger unavailable (persistence is enabled but MIKE_DATABASE_URL is not set …)` | mike has no database to record launches in, or the running mike did not answer. | Run `mike doctor`; start mike with `mike serve` or `mike claude`, or finish [Setup](setup.md). Launches are queued locally meanwhile. |
| `mike spawn list` prints only the header | The backend hosts no panes, or none were started by mike. | Start one with `mike spawn open`. |
| `mike spawn doctor` ends with `doctor: problems found` | At least one check failed. | Follow the `remedy:` line under each `FAIL`. |
| A session shows as inactive though its pane is open | It is busy inside a subagent and reads mail only on main turns. | Wait, or message it again later. |
