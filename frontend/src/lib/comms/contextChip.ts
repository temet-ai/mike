import { resolve } from '$app/paths';
import type { ContextItem } from './types';

export type ContextChipMode = 'internal-link' | 'external-link' | 'action' | 'label';

function httpUrlOrNull(value: string): string | null {
	try {
		const url = new URL(value);
		return url.protocol === 'http:' || url.protocol === 'https:' ? value : null;
	} catch {
		return null;
	}
}

export function contextChipMode(itemOrKind: ContextItem | ContextItem['kind']): ContextChipMode {
	const item = typeof itemOrKind === 'string' ? undefined : itemOrKind;
	const kind = item ? item.kind : itemOrKind;
	switch (kind) {
		case 'request':
		case 'session':
			return 'internal-link';
		case 'url':
			if (item && httpUrlOrNull(item.value) === null) return 'label';
			return 'external-link';
		case 'thread':
		case 'message':
			return 'action';
		default:
			return 'label';
	}
}

export function contextChipHref(item: ContextItem, sessionId: string): string | null {
	if (item.kind === 'request') {
		return resolve(
			`/sessions/${encodeURIComponent(sessionId)}/requests/${encodeURIComponent(item.value)}` as `/sessions/${string}/requests/${string}`
		);
	}
	if (item.kind === 'session') {
		return resolve(
			`/sessions/${encodeURIComponent(item.value)}/comms` as `/sessions/${string}/comms`
		);
	}
	if (item.kind === 'url') {
		return httpUrlOrNull(item.value);
	}
	return null;
}

export function contextChipLabel(item: ContextItem): string {
	return item.label ?? item.value;
}
