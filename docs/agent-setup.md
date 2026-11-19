# Setting up mike (for the agent reading this)

The person you work for pasted a line like "Read this file, then set up mike for me". This file is
everything you need to install mike and set it up so it captures their Claude Code sessions, on Linux
or macOS. Work through it in order, tell the person briefly what you are doing, and ask them only the
questions marked **Ask** below.

mike is a local proxy. It sits between Claude Code and Anthropic, keeps a full copy of every request
and answer in a PostgreSQL database on this machine, and shows them in a web page and to Claude Code
through an MCP server (`mike-mcp`). Installing mike puts two programs on the machine. Nothing is
captured until setup is done and Claude Code is started through mike.

One thing you cannot do from here: `mike setup` on its own is a full-screen terminal program, and you
cannot drive it. This file uses the commands that do the same work without it. The result is the same
database, service and Claude Code connection. The steps for features, background models, search by
meaning, reranker and terminal backend stay at their defaults; the person can open them later with
`mike setup <step>`.

## 1. Warn the person, then check the machine

**Say this to the person before you install anything, in your own words:**

> mike stores every prompt you send to Claude Code and every answer, in full, in a database on this
> machine. That includes any file, command output or secret Claude Code read in a session. Credentials
> in headers are not stored, but nothing in the message bodies is filtered. mike has no login: anything
> on this machine, or any other user of it, can read the captures. A fresh install sends nothing to
> anyone except the request Claude Code was already sending to Anthropic. Do you want to go ahead?

Go on only if they say yes. The full list of what is stored and what leaves the machine is in
[docs/user-guide/privacy.md](user-guide/privacy.md).

Then check:

- The platform must be Linux on x86_64 or ARM64 with glibc 2.35 or newer, or an Apple Silicon Mac. There is no
  Windows, Intel Mac or 32-bit ARM build. `uname -sm` shows the platform; `getconf LONG_BIT` must print
  `64`. If the platform is not supported, stop and tell the person.
- `claude --version` must print a version, and Claude Code must be signed in.
- For the recommended database choice, `docker info` must succeed.
- **Already installed?** `command -v mike`. If it prints a path, run `mike --version` and
  `mike doctor` (neither changes anything) and skip to step 3 for whatever is missing.

## 2. Install mike

mike comes as two programs, `mike` and `mike-mcp`, installed together from the same release. mike is a
pre-release (`0.1.0-alpha.5` at the time of writing); say so to the person.

**Homebrew** (macOS, and Linux where Homebrew is installed), the recommended route:

```sh
brew install temet-ai/mike/mike temet-ai/mike/mike-mcp
```

Use the full names. The short names make Homebrew refuse the formula as coming from an untrusted tap.

**mise**, if the person uses it and not Homebrew:

```sh
mise use -g github:temet-ai/mike@0.1.0-alpha.5 \
  'ubi:temet-ai/mike[matching=mike-mcp,exe=mike-mcp]@0.1.0-alpha.5'
```

Check the [releases page](https://github.com/temet-ai/mike/releases) for a newer version first. Keep
the quotes around the second argument.

**Archive**, if neither is available: follow "Install from an archive" in
[docs/user-guide/install.md](user-guide/install.md). It needs Python 3.11 or newer and unpacks into
`~/.local/bin`. Ask before using it.

Check the install:

```sh
mike --version
mike-mcp --version
```

If a command is not found, the install folder is not on `PATH` (for an archive install, add
`~/.local/bin`). Use the full path and tell the person to fix their `PATH`.

## 3. Choose where history is kept

**Ask**: "Where should mike keep your history?" Offer the choices that fit this machine, recommended first:

1. **A PostgreSQL that mike runs in Docker** (recommended, if `docker info` worked). History lasts, mike
   manages the container, and it is reachable only from this machine. The first run downloads and
   builds a database image; tell the person it is working while that runs.
2. **A PostgreSQL the person already runs.** It must be PostgreSQL 19 with the `vector` extension.
   Ask for the connection URL, write it to a file only they can read, and never put the URL or its
   password on a command line:

   ```sh
   umask 077 && printf '%s\n' '<url the person gave you>' > ~/.config/mike-database-url
   ```

3. **In memory only.** Nothing is kept when mike stops. Good for a first look, not for history. Offer
   it when there is no Docker and no PostgreSQL, and say what is lost.

Then run the one that was chosen. `--accept-trial-loss` is required every time, because switching
storage can restart mike and lose anything captured only in memory; tell the person that.

```sh
mike setup --mode docker --accept-trial-loss
mike setup --mode existing --database-url-file ~/.config/mike-database-url --accept-trial-loss
mike setup --mode trial --accept-trial-loss
```

`--port <PORT>` changes the Docker database's port (default 6666). If setup fails, show the person
its message; do not retry with other options. Do not pass `--mode` together with a step name.

For the Docker choice you can look first, without changing anything: `mike database check`.

## 4. Choose whether mike runs in the background

**Ask**: "Should mike keep running in the background and start when you log in?" Recommend yes. Say
what it does: on Linux it writes two systemd user units, on macOS two launchd agents (mike and its
forwarding sidecar). If they say no, nothing is lost: `mike claude` starts mike by itself when it is
not running, but nothing keeps running after they close it.

If yes:

```sh
mike service install --dry-run
mike service install
mike service status
```

The first command shows what would be written. `mike service install` refuses if a service file of the
same name was not written by mike, or if something already answers on mike's ports; tell the person
what it says. On Linux, user services stop when the person logs out unless they run
`sudo loginctl enable-linger $USER` once. Do not run `sudo` yourself; tell them.

## 5. Register Claude Code and choose the route

mike captures a session only when Claude Code is started through it. It does this by starting Claude
Code with `ANTHROPIC_BASE_URL` pointing at mike's proxy (`http://127.0.0.1:9988`), or, with the sidecar
route, through the forwarding sidecar on `127.0.0.1:9987`. It does not edit Claude Code's own
settings to do this. A Claude Code session started any other way, including the one you are running
in, is not captured.

Do these three, in this order:

**Register the MCP server**, so Claude Code sessions can search what mike captured. It adds a `mike`
entry to `~/.claude.json` and leaves everything else alone:

```sh
mike claude-code mcp
```

**Choose the route.** Use `sidecar` if the person installed the background service in step 4, because
the service runs the sidecar and the sidecar also captures Claude's Remote Control traffic. Use `plain`
if they did not: it goes straight to the proxy, needs no certificate, and captures model traffic only.
If `sidecar` is chosen but the sidecar is not running, `mike claude` says so and falls back to the
proxy, so a wrong choice loses nothing but Remote Control traffic.

```sh
mike claude-code route sidecar
mike claude-code route plain
```

**Ask** (optional): "Do you want a short shell command, `cc`, that starts Claude Code through mike
from any folder?" It writes a function into their shell startup file between `# >>> mike >>>` and
`# <<< mike <<<` markers. If yes:

```sh
mike claude-code shell
```

It refuses a name that is already a command on the path; `--name <NAME>` picks another. If they say
no, they start Claude Code with `mike claude` instead.

Leave the Claude Code mods, folder trust and features alone unless the person asks. `mike claude-code
mods --with-wiretap` records every prompt and tool call and must never be run without their say-so.

## 6. Check that it works

```sh
mike doctor
mike claude-code status
mike mcp probe
```

`mike doctor` lists each check as `ok`, `warn` or `FAIL`, and each line that is not `ok` ends with a
`fix:` line naming the setup step. It exits with an error if a required check fails. Required for
capture: `database url` (no `FAIL`), `proxy port`, `inspect port` and `web UI port` all `ok`, and
`claude` and `mike-mcp` found. `warn` lines for `terminal backend`, `running capabilities` or the
reranker do not stop capture; mention them in one line, do not fix them. `mike mcp probe` changes
nothing and needs mike to be running; with no mike running it says the backend is unavailable.

If a port check fails because mike is not running, start it: `mike service restart` if you installed
the service; otherwise the `mike claude` run below starts it.

Then show that a session is captured. Ask the person to open a **new** terminal in a project folder and
run `mike claude` (or `cc`), type one short prompt, and open <http://localhost:9990>. Under
**Sessions** their session is at the top, with the request and the answer. Do not run the prompt for
them: it would be a real request on their account and would be stored too. New Claude Code sessions
also load the MCP server on start, so in that session `list_sessions` and other `mike` tools appear.

## 7. Tell the person what happened

Say, in a few lines, what you did and what is left to them:

- Which version of mike is installed and how (Homebrew, mise or archive).
- Where history is kept (Docker, their own PostgreSQL, or memory only, which is lost when mike stops).
- Whether mike runs in the background, and the route chosen.
- That only sessions started with `mike claude` (or `cc`) are captured; plain `claude` is not.
- That the web page is <http://localhost:9990>, and that it has no login, so mike belongs on a computer
  only they use.
- Any `warn` or `FAIL` line `mike doctor` still shows, and the `fix:` command it names.
- That the rest of setup (features, background models, search by meaning, reranker, terminal) is
  skipped and is a command away: `mike setup` in a terminal, or `mike setup features`, and so on. Say
  that `mike features watch-only` keeps everything optional off.

## If something fails

- `mike doctor` names the setup step for each problem. The pages in
  [docs/user-guide/troubleshooting.md](user-guide/troubleshooting.md) and
  [docs/user-guide/setup.md](user-guide/setup.md) cover the rest.
- If mike needs a step you cannot do (for example a command that needs administrator rights, which mike
  prints instead of running), give the person the command and say what it is for.
- If the person would rather click through setup themselves, tell them to run `mike setup` in a
  terminal. It asks the same questions, one screen each, with the recommended choice first.

## Undo it

Tell the person this, and run it only when they ask:

```sh
mike uninstall
```

It removes the MCP entry, shell function, mods, route choice and background services that setup added.
It keeps the database, its data and the configuration file, so their history is not deleted. If the
MCP server was registered with `mike mcp register`, also run `mike mcp unregister`. Then remove the
programs the way they were installed:

```sh
brew uninstall mike mike-mcp
```

(for mise, `mise uninstall github:temet-ai/mike@<version> ubi:temet-ai/mike@<version>`). To delete the
captures too, follow "Delete what mike stored" in [docs/user-guide/privacy.md](user-guide/privacy.md);
`mike uninstall --purge` does not remove a database that setup created.
