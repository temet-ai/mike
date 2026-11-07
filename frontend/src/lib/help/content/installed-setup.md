## Install, set up and capture

This help is bundled into mike. Reading it needs no source checkout, Rust, Node, or repository access. mike ships for Linux on x86_64 or ARM64 (glibc 2.35 or newer) and Apple Silicon macOS. Do not assume another platform works.

Claude Code must already be installed and authenticated for a real capture; `claude --version` checks installation but does not send a request.

## Install

Install `mike` and `mike-mcp` together from the same release. Both are needed: `mike` runs the proxy and the control centre, and `mike-mcp` lets Claude Code query captured traffic.

With Homebrew (Linux and Apple Silicon macOS):

```sh
brew install temet-ai/mike/mike temet-ai/mike/mike-mcp
```

With mise:

```sh
mise use -g github:temet-ai/mike@<version> \
  'ubi:temet-ai/mike[matching=mike-mcp,exe=mike-mcp]@<version>'
```

Use the version number of the release you want, for example `0.1.0-alpha.3`. Both routes put `mike` and `mike-mcp` on your PATH. Check the install with `mike --version` and `mike-mcp --version`.

Without a package manager, keep the matching archive pair from one release candidate together on your executable PATH. Retain the candidate manifest and both archive/checksum pairs. Check the supplied checksums before using explicitly trusted files; checksums establish integrity, not authenticity. The optional [maintenance kit](/help/docs/recovery-and-maintenance) can install the pair without a source checkout.

## Set up

Run `mike` in a terminal to open the control centre, then choose Setup. The same setup can run from the command line:

```sh
mike setup
```

Setup offers three modes. `trial` keeps captures in memory. Trial mode is explicitly ephemeral: restarting it loses captured history. `docker` provisions a local PostgreSQL database in Docker. `existing` uses a PostgreSQL database you already run. Choose a durable database if history must survive restarts. The browser alone does not prove persistence is configured.

To provision the managed database directly, with Docker and Compose installed:

```sh
mike database provision --port 6666
mike database check
```

`mike database check` validates the prerequisites without running migrations. The provisioning command saves a private per-user config and a random private credential.

Installed boot configuration comes from the selected TOML file, not from any project's `.env` file. Precedence is defaults, selected TOML, `MIKE_*` process environment, then command-line flags. To select a private installation explicitly, use the same file on every command:

```sh
mike --config "/absolute/private directory/config.toml" serve
```

`mike print-config` shows the selected boot configuration and a labelled live database overlay when available. `mike print-config --shell` shows boot values only. Boot-file changes require a restart; settings in the browser apply according to the timing shown on each setting.

## Start and stop

Start the proxy in the foreground:

```sh
mike serve
```

`mike serve` applies the database migrations on startup and listens for the proxy, the inspect API and the web UI. Open <http://localhost:9990> for the web UI. The control centre's Start choice does the same from inside `mike`.

Stop a foreground `mike serve` with Ctrl-C. Stop a managed Docker database without deleting its data with:

```sh
mike database stop
```

Stopping the proxy does not remove its data. `mike doctor` reports the state of the selected installation.

## Make the first capture

Open [Getting started](/help/guides/getting-started) for this running instance's proxy and MCP addresses. On the same computer, run `mike claude` from your project directory and send a prompt. The wrapper reads the selected installation, starts its stack when needed, and launches Claude Code. If you used a custom config file, pass that same `--config` option here too.

The wrapper can record the working directory as trusted when auto-trust is enabled. Review that setting before use; trust gives Claude Code permission to work in that directory. The optional forward sidecar has a separate certificate trust boundary. Direct localhost proxy use needs no extra CA certificate.

To query captures from Claude Code, register the companion with `mike mcp register` and check it with `mike mcp probe`. `mike mcp unregister` removes only that registration.

Open [Sessions](/sessions), select the new session, then inspect a request and response. Stop and restart a durable installation and confirm that the same session remains before relying on it for retained history.

## Upgrade

Upgrade through the channel you installed from, and restart `mike serve` afterwards so the new binaries and migrations take effect:

```sh
brew upgrade mike mike-mcp
```

```sh
mise upgrade github:temet-ai/mike ubi:temet-ai/mike
```

Before upgrading an installation that keeps history in PostgreSQL, take a coordinated backup first with the maintenance kit. Keep the previous binaries and the backup until the new version has started and a new capture has been seen. Reinstalling an older binary against newer data is not a rollback; restore the backup into a fresh instance instead. See [backup, restore and upgrade](/help/docs/recovery-and-maintenance).

## Uninstall

Remove the MCP registration, then stop the proxy, then remove the commands:

```sh
mike mcp unregister
mike uninstall
```

`mike uninstall` keeps the config, the data and the database. Then remove the binaries through the channel you installed from:

```sh
brew uninstall mike mike-mcp
```

```sh
mise uninstall github:temet-ai/mike@<version> ubi:temet-ai/mike@<version>
```

Removing the binaries does not remove captured history. The config, the data and state directories and any managed database remain until you delete them yourself. Keep a backup first if you may need the history again.

## Find files

On Linux, defaults are `$XDG_CONFIG_HOME/mike/config.toml`, `$XDG_DATA_HOME/mike` and `$XDG_STATE_HOME/mike`; unset XDG variables use `~/.config`, `~/.local/share` and `~/.local/state`. Logs are in the state directory's `logs` folder. For an explicitly selected config, defaults are isolated `data`, `state` and `logs` directories under a per-installation `.mike-<installation-id>` directory beside that config. Relative data and log overrides resolve against the selected config directory, not the current working directory.

Do not share config files, database connection strings, or complete logs publicly. Captured conversations may contain source code, tool results and personal data. Durable capture lives in the selected PostgreSQL database; copying only the config directory is not a history backup.
