# Run mike from the command line

Everything you can do in mike's web UI starts from a command: `mike` itself, and `mike-mcp`, the companion Claude Code uses to search what mike captured. This page lists the commands you will use to install, start, check and connect mike, grouped by what you are trying to do. It leaves the analysis and repair tools to a short list at the end, and the settings they read to [Settings](settings.md).

Run `mike --help` for the commands in your build, and `mike <command> --help` for one command's flags. The examples here continue the guide's fictional `~/projects/site` folder, where a session called `docs` is rewriting the install page.

Every `mike` command accepts `--config <FILE>`, which selects the configuration file. Pass it on every command when you keep more than one installation, or when you run commands from another folder. [Settings](settings.md#boot-settings-and-environment-variables) explains the order mike reads settings in.

## Start mike and open a session

1. Run `mike service install` once. mike starts at login from then on. If you would rather run it by hand, run `mike serve` in a terminal and leave it open.
2. Run `mike claude` from `~/projects/site`.
3. You should see mike print the new session's identifier, then Claude Code opens. The session appears at <http://localhost:9990/sessions>.

| Command | What it does |
|---|---|
| `mike` | In a terminal, opens the [terminal app](terminal.md). When its output is not a terminal, for example under a service manager, it runs `mike serve` instead. |
| `mike serve` | Runs mike in the foreground: the capture on port 9988, the local API on 9989 and the web UI on 9990. It applies any database changes on startup. Press Ctrl-C to stop it. |
| `mike claude [claude arguments]` | Starts mike if it is not running, then starts Claude Code wired to it. Everything after `claude` goes to Claude Code. |
| `mike --version`, `-V` | Prints the version and build. Needs no configuration and no database. |
| `mike --build-info` | Prints the build identity as JSON. Use it alone. |

Closing a browser tab does not stop mike. Let running Claude Code sessions finish before you stop it, because stopping can cut an answer short.

### Keep mike running in the background

`mike service` runs mike without a terminal, starts it at login and starts it again after a crash. It manages two services: mike itself, and the [forwarding sidecar](#the-forwarding-sidecar).

| Command | What it does |
|---|---|
| `mike service install [--dry-run]` | Writes the two service definitions, starts both, and checks that mike answers. On Linux they are systemd user units named `mike.service` and `mike-forward.service`. On macOS they are launchd agents named `ai.temet.mike.proxy` and `ai.temet.mike.forward`. Running it again changes nothing unless a definition would differ. |
| `mike service status` | Shows whether each service is installed, loaded and running. Exits 1 when either is down. |
| `mike service restart [--dry-run]` | Restarts both. Run it after you upgrade mike. |
| `mike service uninstall [--dry-run]` | Stops both and removes the definitions, only when they carry the comment `Managed by mike service install`. A file you wrote yourself at the same path is left alone. Logs are kept. |

`--dry-run` prints what would be written and run, and changes nothing. Install refuses to run while a development stack is running. On Linux, user services stop when you log out unless you run `sudo loginctl enable-linger $USER` once; mike never runs `sudo` for you.

## Set up and check mike

[Setup](setup.md) walks through these in order. Each also works alone.

| Command | What it does |
|---|---|
| `mike setup [step]` | Opens the guided setup in a terminal, and refuses to run anywhere else. Name a step to open it there: `welcome`, `machine`, `database`, `service`, `claude`, `features`, `models`, `embeddings`, `terminal` or `ready`. An unknown name prints this list. |
| `mike setup --mode <trial\|docker\|existing> --accept-trial-loss` | Makes the database choice without the screens. `--accept-trial-loss` is required with every `--mode`, because switching can restart mike and drop trial captures. |
| `mike doctor` | Checks the installation and changes nothing: configuration, database, ports, the sidecar, Claude Code, the MCP companion and more. Each problem names the setup step that fixes it. Exits 0 when nothing failed and 1 otherwise. |
| `mike print-config` | Prints the settings mike would start with, then the settings you changed in the web UI. Credentials are not printed. |
| `mike migrate-check` | Compares the database's change history with the one built into this version, and applies nothing. It fails when the database has changes this version does not know. |
| `mike features <watch-only\|recommended\|everything>` | Sets every feature switch at once, reads them back and reports any that did not take. [Settings](settings.md#the-three-starting-points) says what each choice turns on. When mike is not running, the choice is saved and applied the next time it starts. |

Flags worth knowing:

| Flag | On | Effect |
|---|---|---|
| `--json` | `mike doctor`, `mike setup` | Prints machine-readable output instead of text. |
| `--support-bundle` | `mike doctor` | Prints a redacted report to the terminal for you to read before sharing. |
| `--database-url-file <FILE>` | `mike setup --mode existing` | A file only you can read, holding the database address. mike does not accept passwords as command-line arguments. |
| `--port <PORT>` | `mike setup --mode docker` | Port for the database on your machine. Default 6666. |
| `--image <IMAGE>` | `mike setup --mode docker` | A different database image, pinned by digest. It must run PostgreSQL 19. |
| `--shell` | `mike print-config` | Prints the settings as shell exports. Credentials and database addresses are left out. |
| `--paths` | `mike print-config` | Prints the configuration, data, state and log folders as JSON, without contacting any service. |

### The database

| Command | What it does |
|---|---|
| `mike database provision [--port <PORT>] [--image <IMAGE>]` | Starts a PostgreSQL database in Docker for this installation, on your machine only, and saves its address in your configuration. Needs Docker with Compose. The password is generated and kept in a file only you can read. Running it again reuses the same database. |
| `mike database check` | Checks that the configured database meets mike's requirements, without changing it. |
| `mike database stop` | Stops this installation's Docker database and keeps its data. |

Only `mike serve` changes the database layout. The other commands refuse to run when the database's history does not match the program.

`mike db` is an earlier way to run a database in Docker: `mike db up [--port <PORT>]`, `mike db status`, `mike db check [URL] [--save]`, `mike db down` and `mike db native [--yes]`. New installs should use `mike setup` or `mike database provision`. Do not run both against the same port.

## Connect Claude Code

Setup runs these for you.

| Command | What it does |
|---|---|
| `mike claude-code status` | Shows the MCP registration, the route Claude Code takes to mike, the shell function, the mods and the folder-trust choice. |
| `mike claude-code mcp [--no-check] [--remove]` | Adds a `mike` entry to `~/.claude.json` pointing at `mike-mcp`, then starts the companion once and reports how many tools it lists. `--no-check` skips that test. `--remove` takes the entry out when it points at `mike-mcp`. |
| `mike claude-code route <sidecar\|plain>` | Chooses how Claude Code reaches mike. `plain` points Claude Code straight at mike and needs no certificate. `sidecar` goes through the [forwarding sidecar](#the-forwarding-sidecar). |
| `mike claude-code shell [--name <NAME>] [--shell <zsh\|bash\|fish>] [--remove]` | Writes a shell function, `cc` by default, that starts Claude Code through mike. It is written between `# >>> mike >>>` and `# <<< mike <<<` markers so it can be replaced or removed. A name already used by a command on your path is refused. |
| `mike claude-code mods [--with-wiretap] [--remove]` | Adds mike's Claude Code mods to `CLAUDE_CODE_PLUGIN_DIRS` in `~/.claude/settings.json`. The `wiretap` mod records every prompt and tool call, so it is added only with `--with-wiretap`. See [Mods](mods.md). |
| `mike claude-code trust <on\|off>` | With `on`, `mike claude` records the folder-trust answer for the folder it starts in, so Claude Code does not ask. Off by default. |
| `mike mcp probe [--binary <PATH>] [--json]` | Starts `mike-mcp` and checks that it connects. Changes nothing. |
| `mike mcp register [--binary <PATH>] [--claude-config <FILE>] [--replace] [--json]` | Registers `mike-mcp` in Claude Code after checking it. `--replace` overwrites a conflicting `mike` entry and keeps a backup. |
| `mike mcp unregister [--binary <PATH>] [--claude-config <FILE>] [--json]` | Removes the entry this installation registered, and only when it has not been edited. |

[MCP](mcp.md) covers what the companion offers.

## Start sessions and run fleets

[Sessions](sessions.md) and [Fleets](fleets/README.md) cover these; this is the list.

| Command | What it does |
|---|---|
| `mike spawn open <folder> [flags] [claude arguments]` | Starts a Claude Code session in a folder, through mike. Flags include `--tab`, `--split <l\|r\|u\|d>`, `--panel`, `--goal <text>`, `--role <role>`, `--instructions <text>`, `--session-name <name>`, `--prompt-file <path>`, `--trust` or `--no-trust`, `--no-proxy-env`, `--no-remote-control` and `--json`. Exits 69 when no terminal backend is chosen. |
| `mike spawn list [--run <ID>] [--json]` | Lists the sessions mike started. |
| `mike spawn kill --session <UUID> [--json]` | Stops one of them. |
| `mike spawn doctor [--json]` | Opens and closes a scratch workspace to check that the terminal you chose works. |
| `mike spawn warm`, `fork`, `templates`, `reap` | Keep a named, already-loaded session and fork new sessions from it; `reap [--yes]` cleans up finished ones. Used by fleets. |
| `mike fleet list` | Lists defined fleets, newest first. |
| `mike fleet status <FLEET> [--watch <SECONDS>] [--json]` | Shows where a fleet stands: tasks, seats, verdicts and what is blocked. |
| `mike fleet start <FLEET>` | Starts a fleet that is ready and waiting for your go-ahead. |
| `mike fleet stop <FLEET>` | Asks a fleet to shut down and reports what is still settling. |
| `mike fleet summary` | One line for a status bar. |
| `mike fleet record <FLEET> [--against <FLEET>] [--recompute] [--json]` | Prints a finished run's record: time to done, minutes lost to faults, tokens per accepted task and manual interventions. |
| `mike fleet focus <FLEET> [SEAT]` | Brings a seat's pane to the front. |
| `mike thread import [--repo <PATH>] [--registry <FILE>] [--dry-run]` | Imports work threads kept as files. See [Threads](threads.md). |

`mike fleet commit`, `check`, `gate` and `where` are for the sessions inside a fleet: they commit a seat's files, share one project check, run the full gate and find where a name is defined. You do not need them to run a fleet.

## Move settings in and out of files

| Command | What it does |
|---|---|
| `mike config path [--state]` | Prints the configuration file path. With `--state`, prints the state folder instead. |
| `mike config import-env [PATH]` | Copies the `MIKE_*` lines of a `.env` file (default `./.env`) into your configuration file. Keys already in the file are kept, and running it again changes nothing. |
| `mike import-dotenv <SOURCE> [--ignore-unknown]` | Imports a dotenv file into the selected configuration, once. The source file is left unchanged and a backup of the old configuration is kept. Names mike does not know are an error unless you pass `--ignore-unknown`. Shell syntax in a value is rejected. |

mike does not read a `.env` file from the folder you are in. Import it with one of these commands.

## The forwarding sidecar

The sidecar lets Claude Code's Remote Control traffic be captured, and keeps Claude Code working while mike restarts. It listens on port 9987 and signs traffic with a certificate authority it generates for itself. See [Privacy](privacy.md) before you use it.

| Command | What it does |
|---|---|
| `mike forward` or `mike forward serve` | Runs the sidecar in the foreground. `mike service install` runs it for you. |
| `mike forward export-ca` | Prints the sidecar's certificate, never its key. |
| `mike forward ca-path` | Prints where `mike claude` and `mike spawn` look for the certificate. |
| `mike forward print-config` | Prints the sidecar's own settings. |

## Remove mike

`mike uninstall` removes the MCP entry, the shell function, the mods it added, the route choice and the background services. It keeps the database, its data and your configuration file. See [Back up and recover](backup-and-recovery.md) for what to do next.

`mike uninstall --purge` also stops a database started by `mike db up` and then asks, one question each with the default no, whether to delete that database's Docker volume and your configuration file. It does not remove a database made by `mike database provision` or by setup; remove that one yourself as the page above describes.

## `mike-mcp`

| Command | What it does |
|---|---|
| `mike-mcp` or `mike-mcp serve` | Runs the companion over standard input and output. Claude Code starts it for you once it is registered. |
| `mike-mcp list-tools [--json] [--<profile>]` | Lists the tools in a profile. |
| `mike-mcp --version`, `--help`, `--build-info` | Version, usage and build identity. |

Two environment variables control it:

| Variable | Effect |
|---|---|
| `MIKE_INSPECT_URL` | Where to find mike's local API. Default `http://127.0.0.1:9989`. Set it if you changed the port. |
| `MIKE_MCP_PROFILE` | Which tools to offer. The default, `core`, is the short list for looking into sessions. `full` adds the rest. Also accepted: `worker`, `coordinator`, `acceptance`, `observer`, `retrospective`, `reference` and `problem_solver`; pass `--<profile>` or set the variable. |

Install `mike` and `mike-mcp` from the same release, and restart Claude Code after you upgrade `mike-mcp`.

## Repair and analysis commands

You will rarely run these. Most print what they would do with `--dry-run` or change nothing until you pass `--execute`; read `mike <command> --help` and use `--dry-run` first. Run them when a release note, [troubleshooting](troubleshooting.md) or `mike doctor` tells you to.

| Commands | When you would need one |
|---|---|
| `backfill-session-prompts`, `backfill-predecessors`, `backfill-flow-admission`, `backfill-tool-definitions`, `backfill-request-envelopes`, `backfill-request-system-prompts`, `backfill-lesson-injections`, `origin-backfill` | After an upgrade adds something mike shows about older captures, to fill it in for sessions captured earlier. Several take `--from`, `--to` and `--batch-size` to work in pieces. |
| `reclassify-system-prompts`, `repair-session-prompts`, `restamp-sidecar-origins`, `demote-duplicate-first-turns` | To correct how older captures were labelled. |
| `compact-request-bodies`, `reclaim-space --table <TABLE>`, `sweep-orphaned-messages`, `forget-retracted-migrations` | To free disk space or tidy the database. `compact-request-bodies` leaves sessions idle for under 14 days alone by default (`--min-idle-days`) and needs `--execute`. |
| `detect-lesson-relations [--project <PROJECT>]` | To compare stored lessons on demand. It sends lesson text to a model; see [Privacy](privacy.md). |
| `eval`, `semantic`, `guard`, `steer-questions`, `directives`, `verifier-replay`, `dossier-backfill`, `dossier-parity`, `dossier-render`, `export-analysis-corpus --out <FOLDER>`, `discover` | Measurement and research tools for tuning the judge and steerers. `verifier-replay` and the `--execute` forms can call a model and spend money: `verifier-replay` stops starting new items at 15 USD unless you pass `--max-usd`. Ordinary use never needs them. |

## What it looks like when something is off

| You see | Why | Do this |
|---|---|---|
| A command says the database history does not match | The database was changed by a newer or older mike | Run `mike migrate-check`, then start the matching `mike serve` once. |
| `mike setup` says it needs a terminal | It was run without one | Run it in a terminal, or use `mike doctor` to see what is missing. |
| `mike setup` says a name "is not a setup step" | A misspelt step | Use one from the list above. |
| `mike service status` exits 1 | A service is down | `mike service install` again, or `mike doctor`. |
| `mike spawn` exits 69 | No terminal to open sessions in is chosen | Set the `launch_backend` setting (or `MIKE_LAUNCH_BACKEND`) to your terminal program, as [Settings](settings.md#boot-settings-and-environment-variables) describes, then run `mike spawn doctor`. |
| `mike forward ca-path` says no certificate exists | The sidecar has never run | Run `mike forward export-ca`, which creates one. |

[Troubleshooting](troubleshooting.md) has the rest.
