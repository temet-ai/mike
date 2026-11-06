# mods

Claude Code mods (plugins of function hooks) that work with mike. They live here so they have history, review and a gate. Each folder is one mod, loaded straight from this checkout.

| Mod | What it does |
|---|---|
| `mike` | A sessions pane (`/seats`), unread mike mail on the status line, and `/recall` to search past turns. In a session that also loads the tether mod (a `tether` folder in `CLAUDE_CODE_PLUGIN_DIRS`), tether serves all of that, the lessons, the changes feed and the loaded-mods note, and this mod does nothing. |
| `fleet-comms` | The fleet inside Claude Code, in a fleet seat only: a session mike started from a fleet definition (it carries `MIKE_RUN_ID`). Any other session loads the mod and gets nothing from it: no command, no timer, no request to mike, no band, no status line. In a seat, `/fleet-comms` opens a pane with the seat's own fleet's status as mike's fleet panel draws it (goal, summary, criteria, ACCEPTANCE OPEN, NEEDS YOU, NOTHING IS MOVING, tasks, seats with context bars, latest verdicts, blocked) and then its messages; a seat with no fleet thread sees its mailbox there instead. `/fleet-comms` (and its `Fleet` chip) is hidden from the typeahead and `/help` while there is nothing for it, no live fleet and no unread mail, and shows again when either appears; typed in full it still runs. While the fleet is starting or running, the band above the prompt carries the latest message and NEEDS YOU, and the status line says NEEDS YOU or NOTHING IS MOVING; a fleet that is done, failed or not yet started raises nothing, and only the pane shows what it left behind. Hide puts the band's messages away; NEEDS YOU stays until it is dismissed. Only a need that arrives after the session starts is toasted. A stopped fleet arms no goal. In a seat whose fleet definition gives it a goal (`goals`, `tasks.<id>.goal`), it runs the built-in `/goal` with that text after the seat's first turn, so the seat keeps working until a model judges the condition met. |
| `mike-mail` | Sends a reply to a session that wrote to you through mike back through mike's mailbox, instead of natively. |
| `wiretap` | Records what a mod can see of a session (prompts, system prompt sections, model steps, tool calls, transcript rows) to a JSON file, to compare with mike's capture. Not linked by default. |
| `mods` | `/mods` opens a pane with a row per mod the session loaded: `name`, `/command` (click one to run it), what it `produces` (panes, bands, status line) and what it does on its own (`auto`), with its description under it. Type in the filter box to narrow it. Needs nothing from mike. `/mods text` prints the same as text. It also draws one row of filled chips on the hint line under the prompt, after Claude Code's mode pill and before its own hint text, one chip per mod that asks for one in its `mod.json` (`offers.buttons`, a `label` and the `/command` it runs): `⏵⏵ auto mode on · Mods Fleet Sessions Changes Hide (shift+tab to cycle)`. A chip shows only while its command is offered: a mod that hides its command when it does not apply (a `command.describe` hook answering `isHidden`, as `fleet-comms` does with no fleet and no mail) takes its chip off the row, and the chip returns when the mod invalidates `command.describe` and offers it again. The mods mod learns this from its own `command.describe` hook, which sees each command's answer as the engine lists it; `$.command.list()` carries no hidden flag and a mod cannot call `$.command.describe`. The row is a surface module of the mod's own (`hooks/row.tsx`, a `Client` element): it draws the chips as coloured text, lights the one under the pointer, inverts it while the button is down, and posts the press to the hooks, which run the command. On the Desktop app the same module draws as styled spans. The chips come first so they never move when the text after them changes length. A status line a mod with a chip pins (`$.ui.status`) is drawn on one row directly under the chips, named by the chip (`Changes: nothing of yours uncommitted · Sessions: 3 changes by others`), and that mod's own `⚠` line is cleared; a mod without a chip keeps its `⚠` line. Both rows are cut to the transcript column's width with an ellipsis. `Hide` gives the line back to Claude Code and the `⚠` lines return at each mod's next refresh; `/mods buttons` or the button in the Mods pane brings the chips back. If the module ever fails to load, the hooks draw plain bracketed buttons in its place and log why. |
| `thread` | Work threads held in mike instead of `worklog.md` files: `/thread:list`, `/thread:get`, `/thread:new`, `/thread:checkpoint` and `/thread:depend` are skills over mike's `work_thread_*` MCP tools, and the first prompt of each session (and the first after a compaction) carries the project's open threads (`active`, `implementing`, `review`) as one line each, at most 2KB, with a count of any left out. In a session that also loads the tether mod (a `tether` folder in `CLAUDE_CODE_PLUGIN_DIRS`), the list is left to tether. Typing `#` and part of a slug in the prompt box offers the project's threads with their status and title. Once the session reads or writes a thread through the `work_thread_*` tools, the status line names it and how long ago this session checkpointed it (`thread release-mike · checkpointed 12m ago`, or `not checkpointed this session`). `/thread-board` opens a `Threads` pane of the project's open threads grouped by status, each with its summary and next step, a button to open it or checkpoint it, and a switch to show parked and done threads; and a `work_thread_get` or `work_thread_checkpoint` result is drawn in the transcript as a compact card (title, status, summary, next step, blockers) in the ctrl+o transcript. Family `mike`. |

## How a mod describes itself

`/mods` reads two files in each mod's `.claude-plugin/` folder: `plugin.json` gives the name, version and description, and `mod.json` gives the card's one-line tagline and what the mod adds. A mod without `mod.json` still shows, with its `plugin.json` description. A new mod should ship both.

```json
{
  "tagline": "One line, in everyday words, about what the mod does for you",
  "family": "mike",
  "offers": {
    "commands": ["/name"],
    "panes": ["Pane title"],
    "bands": ["what shows above the prompt"],
    "status": ["what shows on the status line"],
    "buttons": [{ "label": "Chip", "command": "/name" }],
    "acts": ["what it does to the session without being asked"]
  }
}
```

Every `offers` key is optional, and so is `family`: mods that name the same family are listed together under it.

`/mods` groups what it lists by where the mod loads:

- **This project**: a mod folder in the project's own `.claude/skills/<name>/`. Claude Code loads it only in that project, so commit it with the project.
- **A family, then Global**: a folder named in `CLAUDE_CODE_PLUGIN_DIRS` (which Claude Code reads only from the process environment or `~/.claude/settings.json`, never a project's settings) or in `~/.claude/skills/<name>/`. These load in every project.
- Plugins installed from a marketplace are named on one line at the end.

The self-description lives in its own file because `claude plugin validate` warns about unknown keys in `plugin.json`, and reads a `hooks` key there as a hooks declaration.

## Install

```
scripts/install-mods
```

This links `~/.claude/mods/<name>` to `mods/<name>` here for every mod except `wiretap`, so a `git pull` keeps the installed mods current. It prints the `CLAUDE_CODE_PLUGIN_DIRS` value and writes it to `env.CLAUDE_CODE_PLUGIN_DIRS` in `~/.claude/settings.json`, changing no other key: entries outside `~/.claude/mods` are kept, a `settings.json` that is not valid JSON is left alone (exit 4), and one that already lists these mods is not rewritten.

- `--with-wiretap` links `wiretap` as well.
- A directory already at a target is never changed or deleted. Without `--backup` it is left as it is, with a message, and the script exits 3. With `--backup` it is moved to `<name>.bak-<timestamp>` and then linked.
- It is idempotent: a link already pointing here is left alone, a link pointing elsewhere is replaced.
- `--dry-run` says what it would do.
- The target root is `$HOME/.claude/mods`, so `HOME=$(mktemp -d) scripts/install-mods` tries it without touching your own. `scripts/test-install-mods` does that, settings file included.

An installed mike build carries these mods under `share/mike/mods` and installs them with `mike claude-code mods` (`--with-wiretap`, `--remove`); `scripts/install-mods` is for a checkout.

A saved mod reloads in every live session, so installing is a deploy: do it when no work is mid-answer.

To try one mod in a single session without installing: `claude --plugin-dir mods/<name>`.

## Checks

`make lint` runs `claude plugin validate` on every `mods/*/`. `make test` runs `claude plugin test` on every `mods/*/` that has `*.test.ts` files, and `scripts/test-install-mods`. Both find the folders by glob, so a new mod is covered by adding its folder. Both report NOT RUN, without failing, when `claude` is not on the path.
