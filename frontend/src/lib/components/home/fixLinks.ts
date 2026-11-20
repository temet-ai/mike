import type { ResolvedPathname } from '$app/types';
import { resolveAppPath } from '$lib/resolveHref';

export type FixSegment =
	| { kind: 'text'; text: string }
	| { kind: 'link'; text: string; href: string; external: true }
	| { kind: 'link'; text: string; href: ResolvedPathname; external: false };

const LINK = /https?:\/\/[^\s]+|(?<=^|\s)\/[a-z][\w\-/]*/g;
const TRAILING = /[.,;:)]+$/;

function push(segments: FixSegment[], text: string) {
	if (text) segments.push({ kind: 'text', text });
}

export function fixSegments(fix: string): FixSegment[] {
	const segments: FixSegment[] = [];
	let cursor = 0;
	for (const match of fix.matchAll(LINK)) {
		const raw = match[0];
		const start = match.index ?? 0;
		const href = raw.replace(TRAILING, '');
		push(segments, fix.slice(cursor, start));
		segments.push(
			href.startsWith('http')
				? { kind: 'link', text: href, href, external: true }
				: { kind: 'link', text: href, href: resolveAppPath(href), external: false }
		);
		cursor = start + href.length;
	}
	push(segments, fix.slice(cursor));
	return segments;
}
