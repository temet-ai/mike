## Select the installation deliberately

The selected TOML file is the boot configuration for `mike`, `mike serve`, `mike claude` and the terminal control centre. Use `--config` with the same file when working from another directory. Merely entering a project with its own `.env` does not select or override the installed configuration.

```sh
mike --config "/absolute/private directory/config.toml" print-config
```

Values apply in this order: built-in defaults, selected TOML, process `MIKE_*` environment variables, command-line flags. An exported variable in the launching shell can therefore override a saved value. Inspect the effective boot values rather than assuming the file alone won.

## Keep configuration private

Use a private configuration directory. On Unix, existing config directories must be owned by the current user and inaccessible to other users (mode `0700`). The writer creates missing managed directories with `0700` and files with `0600`; it refuses unsafe or symlinked destinations instead of changing permissions on an arbitrary existing directory.

Default installed paths are listed in [Installed setup](/help/docs/installed-setup). An explicit config selects separate per-installation managed data, state and logs under that config's parent. Relative data and log paths are resolved against the selected config directory, so changing the current working directory does not relocate them.

## Import a legacy dotenv file once

The importer is an explicit migration command, not a shell. Use a private selected destination:

```sh
mike --config "/absolute/private directory/config.toml" import-dotenv "/absolute/path/legacy.env"
```

The source dotenv file is left unchanged. A read-only selected config is not overwritten. When replacing a selected config, the importer preserves a backup beside it before writing the new file. Treat both the config and its backup as private, since either may contain credentials.

Supported input is literal `KEY=value` or `export KEY=value`, optionally single- or double-quoted, with the supported escaped quote/backslash forms, whitespace and comments. Quoted dollar signs and backticks remain literal. Values are never evaluated as shell commands; unquoted shell substitutions and executable syntax are rejected.

Unknown or unrelated variable names fail by default. If the legacy file mixes unrelated application variables, append `--ignore-unknown` to skip unsupported names. That option does not permit executable syntax or make shell expansion run.

## Separate restart settings from live settings

Changes to listener addresses, upstream routing, persistence, data paths and log paths require a restart of the selected server. Save them, stop the intended installation after active work finishes, then start it with the same selected config.

Browser runtime settings and feature toggles are database-backed and separate from boot TOML. [Settings](/settings) shows when each setting applies. `mike print-config` labels the live overlay separately; if the database is unavailable, that section says so. `mike print-config --shell` remains boot-only and omits credentials.

Saving an OpenRouter provider key in Setup reloads the selected config without mutating the current process environment, then reconciles and verifies the selected owned server. This can start or restart that server to apply the effective configuration, including other saved boot changes. Let active sessions finish first; a restart loses ephemeral trial captures. Environment or CLI overrides that replace the requested key are refused before saving. If the key was saved but the server could not be verified, setup remains incomplete: inspect the selected server, retry, or restore the private config backup. Successful application verifies the server, not authentication with the provider. Never copy secrets from config files into a public support report.
