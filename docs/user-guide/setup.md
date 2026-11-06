# Set mike up

Setup is the first thing you run after [installing mike](install.md). It checks your machine, asks where mike should keep history, connects Claude Code to mike and lets you choose which optional features are on. It changes nothing without showing the exact command first, and it never runs `sudo`: a command that needs administrator rights is printed for you to run.

Run it in a terminal:

```sh
mike setup
```

Setup needs a terminal. Without one it stops with a message that points you to `mike doctor`. The same checks and the commands that fix them are also in the browser at <http://localhost:9990/settings/setup>, where you can change the settings a browser can and copy the command for the rest.

## The steps

Setup shows one screen per step. Each lists what it found, then the choices that fit this machine, with the recommended one first. `⏎` runs the highlighted choice, and `f` leaves setup and keeps the answers so far. Run `mike setup <step>` to reopen it at one step: `database`, `service`, `claude`, `features`, `models`, `embeddings` or `terminal`.

| Step | What it settles |
|---|---|
| Welcome and machine | Whether setup should list the tools it can use on this machine. It also repeats the warning that mike stores every prompt and answer in full, on this machine. |
| Database | Where history is kept. |
| Background service | Whether mike keeps running after you log out. |
| Claude Code | How Claude Code reaches mike, and what is registered in it. |
| Features | Which optional features are on. |
| Background models | Which model service mike's own workers use. |
| Search by meaning | Whether recall can match by meaning as well as by text. |
| Terminal | Which terminal tool `mike spawn` and fleets use. |
| Ready | A summary, and shortcuts to start Claude Code, open the web UI or take the tour. |

You can skip most of these on the first run. The database and Claude Code steps are the ones that decide whether anything is captured.

## Choose where history is kept

mike stores captured sessions in PostgreSQL 19 with the `vector` extension. The Database step offers what fits your machine:

| Choice | Pick it when | What happens |
|---|---|---|
| **Run PostgreSQL in Docker** | Docker is installed and you want history that lasts, with mike managing the database. | Starts a database in a container on your own machine only, keeps the data in a Docker volume and generates a password into mike's config file. |
| **Install PostgreSQL 19 and pgvector with Homebrew** | You are on a Mac with Homebrew. | Runs the Homebrew commands it shows, then creates the database. |
| **Install PostgreSQL with your package manager** | Neither of the above fits. | Prints the commands for this machine and runs none of them. Run them, then choose the next option with the URL they end with. |
| **Use a PostgreSQL I already run** | You have a server. | Checks the URL you type and saves it only if the version, the extensions and the migrations pass. |
| **Keep captures in memory only** | You want a first look. | Nothing is stored between runs, and the web UI shows only what happened since mike started. |

If a database is already set, the first choice is **Keep the configured database**, which re-checks it.

To make the database choice without the screens:

```sh
mike setup --mode docker --accept-trial-loss
```

`--mode` takes `trial`, `docker` or `existing`. With `existing`, pass `--database-url-file` with the path of a file only you can read; mike never takes a password as an argument. `--port` sets the database port (default 6666). `--accept-trial-loss` is required every time, because switching storage can restart mike and lose captures that were only in memory.

To manage the Docker database yourself, `mike database check` validates the prerequisites without changing anything, `mike database provision` creates the container, and `mike database stop` stops it and keeps the data.

## Keep mike running

The Background service step installs mike as a login service, so the proxy and its forwarding sidecar start at login and restart after a crash. On Linux it writes two systemd user units; on macOS, two launchd agents. It refuses if something already answers on mike's ports.

```sh
mike service install
```

`mike service status` reads the state of both, and `mike service restart` restarts them. `mike service install --dry-run` shows what it would write. If you skip the service, `mike claude` starts mike by itself when it is down, and nothing keeps running after you log out.

## Connect Claude Code

The Claude Code step makes four choices, each skippable:

1. **Register mike-mcp.** Lets a Claude Code session query what mike captured. `mike mcp register` does the same; `mike mcp probe` checks it and `mike mcp unregister` removes it. See [MCP](mcp.md).
2. **Route.** *Through the forwarding sidecar* (recommended) sends Claude Code through the sidecar, so mike also captures the traffic of Claude's Remote Control. It needs a certificate authority file that `mike forward` creates the first time it starts. *Straight to the proxy* needs no certificate and captures model traffic only. Change it later with `mike claude-code route sidecar` or `mike claude-code route plain`. The route applies to the shell function setup installs. `mike claude` goes through the sidecar only when the `launch_use_sidecar` setting is on, and it is off by default.
3. **Folder trust.** Whether `mike claude` marks the folder as trusted before it starts. Off means Claude Code asks about each new folder itself. `mike claude-code trust on` and `trust off` change it.
4. **Mods and a shell function.** The [mods](mods.md) add panes and buttons inside Claude Code. The shell function, `cc` by default, starts Claude Code through mike from any folder. Setup writes it into your shell's startup file and replaces it in place if you run the step again.

Whichever you choose, `mike claude` starts Claude Code wired to mike.

## Choose features

The Features step offers three presets, and `mike features <preset>` applies one from the command line:

| Preset | What it turns on |
|---|---|
| `watch-only` | Nothing that rewrites a request or sends captured text to another service. Every other switch keeps its default. |
| `recommended` | Watch only, plus recall, lessons and reminders. They add notes to what Claude Code sends to Anthropic; nothing leaves your machine for them. See [Memory](memory.md). |
| `everything` | Every switch on the settings page, including those that send captured text to OpenRouter and those that rewrite requests and responses. The ones that call a model do nothing until a provider is set. |

Choose `watch-only` to keep everything optional off. The step shows which switches a preset would turn off. If mike is not running, the choice is saved and applied the next time it starts. [Settings](settings.md) lists every switch with its default, and [Privacy](privacy.md) says what each one sends and where.

## Models and search by meaning

Background models are what mike's own workers use for judging and for finding lessons. Choose "Use" plus a provider's name "for every job" for a provider the step found on your machine, set up an OpenRouter key, or leave the models alone. Features that need a model stay idle until one is set. Skip this step unless you turned those features on; [the judge](judge.md) and [steering](steering.md) say which need one.

Search by meaning lets recall match a request to lessons by meaning, not only by words. The step can pull a small embedding model into Ollama on your machine, point at another embedding server, or leave it off, in which case recall uses text matching. Skip it on a first run.

## Check the result

```sh
mike doctor
```

`mike doctor` lists each check as `ok`, `warn` or `FAIL`, and each line that is not `ok` ends with the setup step that fixes it, for example `fix: mike setup claude`. It exits with an error when a required check fails. `mike doctor --json` and `mike doctor --support-bundle` print a JSON summary of the installation instead, with values left out; use the support bundle to preview what you would attach to a bug report. Neither changes anything.

## When a step fails

| What you see | What to do |
|---|---|
| `FAIL database url`: persistence is on but no database URL is set | Run `mike setup database`, or choose **Keep captures in memory only**. |
| `FAIL inspect port`: nothing answers | mike is not running. Run `mike claude`, or install the service with `mike setup service`. |
| `warn mike-mcp in claude`: not registered | Run `mike setup claude`, or `mike mcp register`. |
| `warn claude route`: no route is chosen | Run `mike setup claude` and choose one. |
| `warn terminal backend`: none chosen | `mike spawn` and fleets are unavailable until you run `mike setup terminal`. |
| Setup says it needs a terminal | Run it in a terminal window, or use `mike doctor` and the commands it prints. |
| The Docker choice is missing | Docker is not installed or not running. Pick another database choice. |

[Troubleshooting](troubleshooting.md) covers the rest, and [Capture your first session](quickstart.md) is the next step.
