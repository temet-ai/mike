import type { IncomingMessage, ServerResponse } from 'node:http';

const MAX_BODY_BYTES = 1024 * 1024;

export async function readJsonBody(req: IncomingMessage): Promise<unknown> {
	const chunks: Buffer[] = [];
	let total = 0;
	for await (const chunk of req) {
		total += (chunk as Buffer).byteLength;
		if (total > MAX_BODY_BYTES) {
			throw new Error('request body too large');
		}
		chunks.push(chunk as Buffer);
	}
	if (chunks.length === 0) return {};
	const raw = Buffer.concat(chunks).toString('utf8');
	if (raw.trim() === '') return {};
	return JSON.parse(raw);
}

/**
 * A conversation id must be exactly a UUID — no path segments, no `..`, no
 * encoded slash. Checked BEFORE the id is used to build a filesystem path
 * (`store/conversations.ts` does `path.join(stateDir, 'conversations',
 * \`${id}.json\`)`), so a value like `..%2Fbudget-ledger` (which contains no
 * literal `/` and so still matches the route's `[^/]+` path segment, but
 * decodes to `../budget-ledger`) is rejected here rather than reaching
 * `path.join` (G15).
 */
const UUID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function isValidConversationId(id: string): boolean {
	return UUID_PATTERN.test(id);
}

export function sendJson(res: ServerResponse, status: number, body: unknown): void {
	const payload = JSON.stringify(body);
	res.writeHead(status, {
		'content-type': 'application/json',
		'content-length': Buffer.byteLength(payload)
	});
	res.end(payload);
}
