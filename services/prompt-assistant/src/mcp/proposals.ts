import type { ProposalAction, SystemBlock } from '../types.js';

export function concatSystemBlocks(blocks: SystemBlock[] | undefined): string {
	return (blocks ?? []).map((b) => b.text).join('\n');
}

/**
 * The `after` text of a proposal, computed from the action + the model's
 * proposed replacement text — never from a `before` the model supplies,
 * because `propose_edit`'s schema has no such field.
 */
export function computeAfter(action: ProposalAction, match: string, text: string): string {
	switch (action) {
		case 'replace':
			return text;
		case 'append':
			return `${match}${text}`;
		case 'drop':
			return '';
	}
}

/**
 * The `before` text of a proposal: the literal `match` substring, but only
 * when it is verifiably present in the request's own captured text. Returns
 * '' when it is not found, rather than trusting the model's claim that it is
 * there — the caller should still show the proposal, just with an empty
 * before.
 */
export function computeBefore(haystack: string, match: string): string {
	return haystack.includes(match) ? match : '';
}
