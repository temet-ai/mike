export type SpaDeltaFallbackReason =
	| 'predecessor-not-loaded'
	| 'compaction'
	| 'lineage-broken'
	| 'chrome-strip-failed'
	| 'parse-error';

function isRecord(v: unknown): v is Record<string, unknown> {
	return v !== null && typeof v === 'object' && !Array.isArray(v);
}

export function messageArraysShareFirstMessageHash(prev: unknown, curr: unknown): boolean {
	if (!isRecord(prev) || !isRecord(curr)) return false;
	const prevMessages = Array.isArray(prev.messages) ? prev.messages : [];
	const currMessages = Array.isArray(curr.messages) ? curr.messages : [];
	if (prevMessages.length === 0 || currMessages.length === 0) return false;
	const p = prevMessages[0];
	const c = currMessages[0];
	if (!isRecord(p) || !isRecord(c)) return false;
	if (p.role !== c.role) return false;
	const pContent = Array.isArray(p.content) ? p.content : null;
	const cContent = Array.isArray(c.content) ? c.content : null;
	if (pContent === null || cContent === null) {
		return typeof p.content === 'string' && p.content === c.content;
	}
	if (pContent.length !== cContent.length) return false;
	if (pContent.length === 0) return true;
	const pb = pContent[0];
	const cb = cContent[0];
	if (!isRecord(pb) || !isRecord(cb)) return false;
	if (pb.type !== cb.type) return false;
	if (typeof pb.text === 'string' && typeof cb.text === 'string') {
		return pb.text === cb.text;
	}
	return true;
}

export interface DeltaFallbackInput {
	shape: 'tail-append' | 'compaction' | 'first-turn' | 'lineage-broken';
	currLen: number;
	prev: unknown;
	curr: unknown;
}

export function classifyShapeBeacon(input: DeltaFallbackInput): SpaDeltaFallbackReason | null {
	if (input.shape === 'compaction') return 'compaction';
	if (input.shape === 'lineage-broken') {
		if (input.currLen > 0 && messageArraysShareFirstMessageHash(input.prev, input.curr)) {
			return 'chrome-strip-failed';
		}
		return 'lineage-broken';
	}
	return null;
}
