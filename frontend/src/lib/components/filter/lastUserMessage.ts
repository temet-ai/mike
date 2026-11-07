/**
 * Extracts the text of the LAST user message from a raw Anthropic Messages-API `messages`
 * array — mirroring `crates/mike/src/extract/request/fields.rs`'s
 * `extract_last_user_message`/`extract_text_from_content` exactly, since the dry-run
 * endpoint's `filtered_messages` field is the raw post-filter `messages` JSON value (an
 * `Option<serde_json::Value>` server-side), not a typed block list. Used for the "Session
 * message" prompt-pane section (spec 4.2/G9): a filter rule can land in the last user message
 * (an `inject_reminder`, or a rule anchored on text that also appears there), and that section
 * needs the SAME extraction the server used to build `RequestDetail.last_user_message` so the
 * two sides of the diff describe the same thing.
 *
 * `messages` is `unknown` on purpose: the type declared on `FilterDryRunResponse.filtered_messages`
 * is a placeholder (`FilterDryRunBlock[] | null`) that doesn't match this field's real shape, and
 * the value is untyped JSON straight from the wire either way.
 */
export function extractLastUserMessageText(messages: unknown): string {
	if (!Array.isArray(messages)) return '';
	for (let i = messages.length - 1; i >= 0; i--) {
		const msg = messages[i];
		if (isRecord(msg) && msg.role === 'user') {
			return extractTextFromContent(msg.content);
		}
	}
	return '';
}

/**
 * A dry-run response with no `messages` key filtered at all (`filtered_messages: null`)
 * almost always means no rule touched the message history — treat that as "unchanged" rather
 * than as "the message became empty", which `extractLastUserMessageText(null)` would otherwise
 * say. Only a genuinely present, filtered `messages` array is read for its real last-user text.
 */
export function resolveDryRunSessionMessage(filteredMessages: unknown, fallback: string): string {
	if (filteredMessages === null || filteredMessages === undefined) return fallback;
	return extractLastUserMessageText(filteredMessages);
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null;
}

function extractTextFromContent(content: unknown): string {
	if (typeof content === 'string') return content;
	if (Array.isArray(content)) {
		return content
			.filter(
				(block): block is { text: string } =>
					isRecord(block) && block.type === 'text' && typeof block.text === 'string'
			)
			.map((block) => block.text)
			.join('');
	}
	return '';
}
