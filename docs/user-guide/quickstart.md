# Capture your first session

By the end of this walkthrough, you will have sent two prompts through mike and found both in the browser. You will also check whether mike changed a request before forwarding it. The sample is a small fictional website; use it before trying a project with private code.

## Before you start

You need [mike installed](install.md), Claude Code installed and signed in, and a terminal on the same computer as mike. Check that both commands are on your path:

```sh
mike --version
claude --version
```

Each should print a version. A version check does not prove Claude Code is signed in; you will find that out when you send the first prompt. The prompts use your normal Claude access and count towards its usage limits.

mike captures prompts, answers and tool output. Keep the web UI on its local address, and read [Privacy](privacy.md) before using real project data. You do not need an embedding server, a background model provider or a fleet for this walkthrough.

## Set up once

On a new installation, run:

```sh
mike setup
```

Choose **Keep captures in memory only** at the database step if you only want a first look; history is lost when mike stops. Choose a durable database if you want to keep what you capture. At the features step choose **Watch only**. Skip background models and search by meaning for now. [Set mike up](setup.md) walks through the choices and what to do when one fails.

You should finish on a **ready** screen. If you leave early, press `f`: setup keeps the answers so far.

**Already using mike?** Keep your configured database rather than switching to trial storage. You can use the sample below without rerunning setup. Existing filter rules and settings still apply, so your **Sent** request may differ from **Original**. This walkthrough does not ask you to reset them.

## Make a sample project

The rest of the guide calls the example folder `~/projects/site` and its session `docs`. The following block creates that folder and one Markdown file. It stops without changing an existing `site` folder. If that name is already in use, choose an unused folder name and use it throughout the walkthrough.

```sh
(
  set -eu
  mkdir -p "$HOME/projects"
  mkdir "$HOME/projects/site"
  cat > "$HOME/projects/site/install.md" <<'EOF'
# Install the Ankh-Morpork guide

This is a fictional website for a documentation exercise.

## Download an archive

Download the archive, unpack it and open the guide.

## Homebrew

Install the guide with Homebrew.
EOF
)
```

You should now have `~/projects/site/install.md` with an archive section followed by a Homebrew section. These are sample instructions, not commands to install a real product.

## Start a session

Go to the sample folder and start Claude Code through mike:

```sh
cd "$HOME/projects/site" && mike claude
```

`mike claude` starts mike in the background if it is not running, then starts Claude Code wired to it. Anything after `mike claude` goes to Claude Code, so `mike claude --model opus` works. Claude Code may ask whether you trust the folder, as it does for any new folder. Review that prompt for this sample folder.

Ask for the work:

> Rewrite install.md to make its instructions clearer. Keep both sections and do not invent installation commands. Only edit this file.

Wait for the answer, then send the correction:

> Put Homebrew first on the install page.

The wording of Claude's answers can vary. The important result is that it answered both prompts and the Homebrew section is now first. You do not need to commit or publish the sample.

mike records a request only when Claude Code sends its session identifier with it, which it does for ordinary prompts. A version check or a health request is not recorded.

## See it in your browser

1. Open <http://localhost:9990> on the same computer.
2. Choose **Sessions** in the left-hand menu and find the group for your sample folder. Open its most recently active session. The guide calls it `docs`, but the title on your machine may differ; use the folder and time to identify it.
3. Find the first prompt and its answer in the timeline. Search the timeline for `Homebrew` to find the correction, then open that turn and read its reply.
4. Choose **view request** beside a prompt. Compare **Original** with **Sent**, then open **Transformations**. On a fresh Watch-only installation, expect no feature-driven rewrite. If you already configured mike, this is where you see the changes it made.

You have completed the walkthrough when you can find both of your prompts and their replies, and open a captured request. A working Claude answer alone does not prove mike saved it.

New turns appear as they happen. [Read a session](reading-sessions.md) explains the timeline, request tabs and search limits.

## If you cannot reach that checkpoint

- **Claude Code cannot answer:** finish its sign-in or resolve the error it reports, then send the prompt again. No successful answer has been captured yet.
- **The browser cannot connect:** run `mike doctor` in another terminal and follow the setup step named by the failed check. Use your configured web UI port if you changed it.
- **Claude answered, but the session or reply is missing:** follow [No session appears](troubleshooting.md#no-session-appears). Check capture health as well as the route; a healthy upstream answer is not a capture receipt.
- **The session disappeared after restarting mike:** trial storage does not survive a restart. [History disappeared](troubleshooting.md#history-disappeared) explains the storage choices.

## What you have, and what you do not yet

You have inspected captured traffic from one session. If you chose trial storage, keep mike running until you are finished reading it; changing to a database does not migrate those in-memory captures.

On a fresh Watch-only installation, mike has not turned your correction into an automatic lesson or watched the session for drift. Those are optional features. Do not expect a new session to know the Homebrew correction until you have set up the relevant memory feature.

## Next steps

- [Get around the web UI](web-ui.md) to see what each page is for.
- [Keep the correction as a lesson](memory.md) so a later session starts with it.
- [Register the MCP companion](mcp.md) so Claude Code can search what mike captured.
- [Read what mike stores and sends](privacy.md) before turning anything on.
- [Find a page](pages.md) for every screen and command.
