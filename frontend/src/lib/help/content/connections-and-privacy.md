## Four ways to use mike

- **Wrapped Claude Code:** `mike claude` launches the installed CLI using the selected configuration and starts the stack if necessary. Additional Claude Code arguments follow `claude`.
- **Standalone proxy:** point an Anthropic-compatible client's base URL at the running proxy address shown in [Getting started](/help/guides/getting-started). Do not add `/v1` to Claude Code's base URL. The client's normal Anthropic authentication is still required.
- **MCP:** run the included `mike-mcp` stdio binary from your MCP client. Set its `MIKE_INSPECT_URL` environment value to this installation's inspect address. `mike-mcp list-tools` is the authoritative catalogue; `--full` selects the larger profile. The bridge includes tools that change state, so it is not a read-only interface.
- **Browser:** the installed binary serves its bundled UI and the API on the browser listener. API and WebSocket requests from the UI use the same origin. The separate Vite process is for source development only.

## Forwarding is not proof of capture

Capture admission requires a valid UUID in the `x-claude-code-session-id` request header. Supported Claude Code traffic supplies it. For another client, supply a stable UUID per conversation in that header. A request without that header, with an invalid UUID, or with an unreadable header is forwarded without capture. A successful upstream answer alone does not prove that a session was recorded.

Use a real prompt and confirm the response in [Sessions](/sessions). A version check, health request or ordinary non-message request does not create a captured conversation.

## Keep listeners local

mike's inspect API has no authentication. Its captured content, settings and launch actions are accessible to local processes, including processes running as another local user. Use it only on a workstation you alone use. Host and Origin checks reduce browser-based attacks; they are not a login system or protection from local processes.

Do not expose the listeners through a LAN address, public interface, reverse tunnel, or unaudited forwarding service. A displayed loopback URL is for the computer running mike, not a remote browser's computer. Copying it to another machine will not connect to this installation.

## Know what can change or leave the machine

mike is not an invariant byte-for-byte forwarding tunnel. Enabled filtering, compositions, recall, lessons, reminders, mailbox delivery, steering and proxy tools can change outbound requests. The weasel intercept can replace an answer and continue upstream generation. The original incoming request is captured separately from those changes, subject to the fidelity information shown on the inspected record.

Open [Settings](/settings) to review active features. Optional provider-backed judges, classifiers, verifiers and remote embeddings can send captured text to configured external services. Stored choices survive upgrades; a new default does not silently replace them. Leave optional providers unconfigured unless you intend to send that data.
