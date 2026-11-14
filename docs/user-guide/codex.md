# Codex sessions

mike can also watch OpenAI Codex. It does not rewrite anything Codex sends: it records what happened so a Codex session shows up beside your Claude Code sessions. This page says what mike sees, how to connect Codex, how to check the connection and the one switch that keeps more.

Everything here stays on your machine. Codex talks to OpenAI as it always did; mike's hooks only tell mike about it.

## What mike sees

For an ordinary Codex session, mike keeps small facts and not the text of the conversation:

- That the session started, ended, stopped, ran a tool or ran a subagent.
- Which model answered and how many tokens it used.
- Messages sent to it from other sessions, so a Codex session can take part in [messages between sessions](reading-sessions.md).

mike does not keep your prompts, Codex's answers, tool arguments or tool output. The session appears in the roster of sessions with this much and no more.

## Connect Codex

Install mike's hooks once in your Codex folder. From a release install, the installer sits in the package's `share/mike/scripts` folder, under the prefix mike was installed into:

```sh
<prefix>/share/mike/scripts/codex-hooks/install --codex-home ~/.codex
```

In a mike source checkout, run `scripts/codex-hooks/install --codex-home ~/.codex` instead. It needs `python3`.

| Option | What it does |
|---|---|
| `--codex-home` | The Codex folder to change. Required. Use a throwaway folder to try it first. |
| `--print` | Shows the hooks and settings it would write, and writes nothing. |
| `--mike-url` | Where mike's inspect API listens. Default `http://127.0.0.1:9989`. |
| `--source` | A label for this machine on the sessions it reports. Default: the machine's name. |

The installer adds its hooks next to yours and keeps the rest of your Codex settings. It turns Codex's hooks feature on, adds a small exporter of usage figures (with prompt and answer logging off) unless you already configured one, and registers `mike-mcp` in Codex if it is installed and no server called `mike` is set. It never changes Codex's sandbox or approval settings.

Codex asks you to trust new hooks before it runs them. Open `/hooks` inside Codex and review and trust mike's. Until you do, mike sees nothing of that session.

## Check the hooks

```sh
mike doctor
```

Look for the `codex hooks` line. It is `ok` when Codex is not installed (nothing to hook) or when mike's hooks are in your Codex folder. It is `warn` when Codex is installed but the hooks are not, and the line ends with the install command above. The check looks only for the installed files, not for your trust review in Codex, so a session that still shows nothing is a reason to open `/hooks`.

If you installed the hooks while mike was stopped, nothing is lost for good: later hook calls report again, but facts from the gap are not replayed.

## Keep the full bodies for diagnosis

A switch, `codex_diagnostic_capture`, is off by default. Turn it on in **Settings**, **Workers** at <http://localhost:9990/settings/workers> when you need to see exactly what Codex sent and received, for example to report a problem. With it on, mike keeps the complete request and response bodies of Codex sessions that go through mike's address, within the size limits the settings page shows, and drops them after 24 hours. Nothing leaves the machine. Bodies contain your code and prompts, so turn it off again when you are done. [Settings](settings.md) lists it with the other switches.

This applies only to Codex whose model calls are sent through mike's proxy. A Codex that talks to OpenAI directly is seen only through the hooks.

## When it does not work

| What you see | What to do |
|---|---|
| `warn codex hooks` in `mike doctor` | Run the install command the line prints, then trust the hooks in Codex with `/hooks`. |
| Hooks installed, the session is missing | Review and trust the hooks with `/hooks` in Codex, and check that mike is running with `mike doctor`. |
| The installer is not in `share/mike/scripts` | Your install has no scripts folder. Install from a release archive, or use a source checkout. |

[Privacy](privacy.md) says what mike stores, and [Troubleshooting](troubleshooting.md) covers the rest.
