export interface TeammateMessage {
	teammateId: string;
	summary: string | null;
	text: string;
}

const ENVELOPE_RE = /<teammate-message\b([^>]*)>([\s\S]*?)<\/teammate-message>/g;
const ATTR_RE = /([\w-]+)\s*=\s*"([^"]*)"/g;

function parseAttrs(raw: string): Record<string, string> {
	const attrs: Record<string, string> = {};
	for (const m of raw.matchAll(ATTR_RE)) {
		const key = m[1];
		const value = m[2];
		if (key === undefined || value === undefined) continue;
		attrs[key] = value;
	}
	return attrs;
}

export function parseTeammateMessages(userPrompt: string): TeammateMessage[] {
	const out: TeammateMessage[] = [];
	for (const m of userPrompt.matchAll(ENVELOPE_RE)) {
		const attrs = parseAttrs(m[1] ?? '');
		out.push({
			teammateId: attrs['teammate_id'] ?? 'unknown',
			summary: attrs['summary'] ?? null,
			text: (m[2] ?? '').trim()
		});
	}
	return out;
}

function firstLineOf(text: string): string {
	const nl = text.indexOf('\n');
	return (nl === -1 ? text : text.slice(0, nl)).trim();
}

export function teammateMessagePreview(messages: TeammateMessage[]): string {
	const first = messages[0];
	if (!first) return '';
	const rest = messages.slice(1);
	const body = first.summary ?? firstLineOf(first.text);
	const suffix = rest.length > 0 ? ` (+${rest.length} more)` : '';
	return `${first.teammateId}: ${body.length > 0 ? body : '(empty message)'}${suffix}`;
}
