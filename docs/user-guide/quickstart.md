# Capture your first session

This page takes you from an installed mike to one Claude Code session captured and open in your browser, using a small fictional website as the example. It assumes you have already [installed mike](install.md) and that Claude Code is installed and signed in (`claude --version` prints a version).

The example is fictional. The folder is `~/projects/site`, the session is called `docs`, and its goal is "Rewrite the install page". Nothing here comes from a real capture.

## Set up once

Run setup in a terminal:

```sh
mike setup
```

Choose **Keep captures in memory only** at the database step if you only want a first look; history is lost when mike stops. Choose a durable database if you want to keep what you capture. At the features step choose **Watch only**, which leaves everything that rewrites a request or sends text elsewhere off. [Set mike up](setup.md) walks through every step and what to do when one fails.

You should finish on a **ready** screen. If you leave early, press `f`: setup keeps the answers so far.

## Start a session

Go to the project folder and start Claude Code through mike:

```sh
cd ~/projects/site
mike claude
```

`mike claude` starts mike in the background if it is not running, then starts Claude Code wired to it. Anything after `mike claude` goes to Claude Code, so `mike claude --model opus` works. Claude Code may ask whether you trust the folder, as it does for any new folder.

Ask for the work:

> Rewrite the install page.

Then correct Claude partway through:

> Put Homebrew first on the install page.

mike records a request only when Claude Code sends its session identifier with it, which it does for ordinary prompts. A version check or a health request is not recorded.

## See it in your browser

1. Open <http://localhost:9990>.
2. Choose **Sessions** in the left-hand menu. The session you just started is at the top.
3. Open it and pick a turn. You see the request exactly as Claude Code sent it and the answer as it streamed back. Your correction, "Put Homebrew first on the install page.", is in the turn where you typed it.

New turns appear as they happen. [Read a session](reading-sessions.md) shows how to move around one.

If nothing appears, run `mike doctor`. It names the setup step that fixes each problem it finds. [Troubleshooting](troubleshooting.md) covers the usual causes.

## What you have, and what you do not yet

- Every request and answer of the session is stored. Nothing else has changed: your requests go to Anthropic unaltered.
- mike has not kept your correction as a lesson, watched the session for drift, or rewritten anything. Those are the optional features, and each has its own page.

## Next steps

- [Get around the web UI](web-ui.md) to see what each page is for.
- [Keep the correction as a lesson](memory.md) so a later session starts with it.
- [Register the MCP companion](mcp.md) so Claude Code can search what mike captured.
- [Read what mike stores and sends](privacy.md) before turning anything on.
- [Find a page](pages.md) for every screen and command.
