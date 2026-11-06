import { describe, expect, it } from 'vitest';
import type { DeltaRowDto } from '$lib/types';
import { assembleDeltaBody } from './bodyFetcher.svelte';
import {
	classifyShapeBeacon,
	messageArraysShareFirstMessageHash,
	type SpaDeltaFallbackReason
} from './deltaFallback';

function addedRow(ord: number, body: unknown, byte_size = 100): DeltaRowDto {
	return {
		change_kind: 'added',
		ord,
		message_id: ord + 1000,
		message_sha256: `sha-${ord}`,
		role: 'user',
		byte_size,
		body
	};
}

function removedRow(ord: number, body: unknown): DeltaRowDto {
	return {
		change_kind: 'removed',
		ord,
		message_id: ord + 2000,
		message_sha256: `rsha-${ord}`,
		role: 'user',
		byte_size: 50,
		body,
		removed_reason: 'unknown'
	};
}

function textMessage(role: 'user' | 'assistant', text: string): unknown {
	return { role, content: [{ type: 'text', text }] };
}

describe('assembleDeltaBody', () => {
	it('sorts added rows by ord ascending, not array order', () => {
		const deltas = [
			addedRow(83, { role: 'assistant', content: 'third' }),
			addedRow(81, { role: 'user', content: 'first' }),
			addedRow(82, { role: 'user', content: 'second' })
		];
		const out = assembleDeltaBody(deltas);
		expect(out).not.toBeNull();
		const parsed = JSON.parse(out!.body.text!);
		expect(parsed.messages.map((m: { content: string }) => m.content)).toEqual([
			'first',
			'second',
			'third'
		]);
	});

	it('derives commonLen from the first added ord and currLen from the last + 1', () => {
		const deltas = [addedRow(81, { role: 'user' }), addedRow(82, { role: 'assistant' })];
		const out = assembleDeltaBody(deltas);
		expect(out!.commonLen).toBe(81);
		expect(out!.currLen).toBe(83);
	});

	it('passes the full message body through verbatim (no truncation)', () => {
		const bigText = 'x'.repeat(50_000);
		const body = {
			role: 'user',
			content: [{ type: 'text', text: bigText }]
		};
		const out = assembleDeltaBody([addedRow(0, body, 50_000)]);
		const parsed = JSON.parse(out!.body.text!);
		expect(parsed.messages[0].content[0].text).toBe(bigText);
		expect(parsed.messages[0].content[0].text.length).toBe(50_000);
	});

	it('ignores removed rows when assembling the added tail', () => {
		const deltas = [
			addedRow(0, { role: 'user', content: 'kept' }),
			removedRow(0, { role: 'user', content: 'dropped' })
		];
		const out = assembleDeltaBody(deltas);
		const parsed = JSON.parse(out!.body.text!);
		expect(parsed.messages).toHaveLength(1);
		expect(parsed.messages[0].content).toBe('kept');
	});

	it('returns null for a pure-removal turn (no added rows)', () => {
		const deltas = [removedRow(0, { role: 'user', content: 'gone' })];
		expect(assembleDeltaBody(deltas)).toBeNull();
	});

	it('returns null for an empty delta set', () => {
		expect(assembleDeltaBody([])).toBeNull();
	});

	it('reports byte_size as the UTF-8 length of the assembled body', () => {
		const out = assembleDeltaBody([addedRow(0, { role: 'user', content: 'hi' })]);
		expect(out!.body.byte_size).toBe(new TextEncoder().encode(out!.body.text!).length);
		expect(out!.body.has_body).toBe(true);
	});
});

describe('messageArraysShareFirstMessageHash', () => {
	it('returns false for non-record inputs', () => {
		expect(messageArraysShareFirstMessageHash(null, {})).toBe(false);
		expect(messageArraysShareFirstMessageHash({}, null)).toBe(false);
		expect(messageArraysShareFirstMessageHash(42, 'string')).toBe(false);
	});

	it('returns false when either messages array is empty', () => {
		expect(messageArraysShareFirstMessageHash({ messages: [] }, { messages: [] })).toBe(false);
		expect(
			messageArraysShareFirstMessageHash({ messages: [textMessage('user', 'x')] }, { messages: [] })
		).toBe(false);
	});

	it('returns false when first-message roles differ', () => {
		expect(
			messageArraysShareFirstMessageHash(
				{ messages: [textMessage('user', 'x')] },
				{ messages: [textMessage('assistant', 'x')] }
			)
		).toBe(false);
	});

	it('returns true when first-message role + first-block text are byte-identical', () => {
		expect(
			messageArraysShareFirstMessageHash(
				{ messages: [textMessage('user', 'identical')] },
				{ messages: [textMessage('user', 'identical')] }
			)
		).toBe(true);
	});

	it('returns false when first-block text differs', () => {
		expect(
			messageArraysShareFirstMessageHash(
				{ messages: [textMessage('user', 'old')] },
				{ messages: [textMessage('user', 'new')] }
			)
		).toBe(false);
	});

	it('returns true when message[0] has the same role + content shape but unknown root keys differ', () => {
		expect(
			messageArraysShareFirstMessageHash(
				{
					messages: [
						{
							role: 'user',
							content: [{ type: 'text', text: 'identical first' }],
							unknown_chrome_key: 'a'
						}
					]
				},
				{
					messages: [
						{
							role: 'user',
							content: [{ type: 'text', text: 'identical first' }],
							unknown_chrome_key: 'b'
						}
					]
				}
			)
		).toBe(true);
	});

	it('returns false when content arrays have different lengths', () => {
		expect(
			messageArraysShareFirstMessageHash(
				{
					messages: [
						{
							role: 'user',
							content: [{ type: 'text', text: 'x' }]
						}
					]
				},
				{
					messages: [
						{
							role: 'user',
							content: [
								{ type: 'text', text: 'x' },
								{ type: 'text', text: 'y' }
							]
						}
					]
				}
			)
		).toBe(false);
	});

	it('handles string-content form (legacy Anthropic shape)', () => {
		expect(
			messageArraysShareFirstMessageHash(
				{ messages: [{ role: 'user', content: 'plain string' }] },
				{ messages: [{ role: 'user', content: 'plain string' }] }
			)
		).toBe(true);
		expect(
			messageArraysShareFirstMessageHash(
				{ messages: [{ role: 'user', content: 'one' }] },
				{ messages: [{ role: 'user', content: 'two' }] }
			)
		).toBe(false);
	});
});

describe('classifyShapeBeacon', () => {
	const emptyArgs = { currLen: 0, prev: null, curr: null };

	it('returns null for tail-append', () => {
		expect(classifyShapeBeacon({ ...emptyArgs, shape: 'tail-append' })).toBeNull();
	});

	it('returns null for first-turn (caller decides predecessor-not-loaded vs first)', () => {
		expect(classifyShapeBeacon({ ...emptyArgs, shape: 'first-turn' })).toBeNull();
	});

	it('returns "compaction" for shape="compaction"', () => {
		expect(classifyShapeBeacon({ ...emptyArgs, shape: 'compaction' })).toBe('compaction');
	});

	it('returns "lineage-broken" for shape="lineage-broken" with mismatched first messages', () => {
		const args = {
			shape: 'lineage-broken' as const,
			currLen: 1,
			prev: { messages: [textMessage('user', 'A')] },
			curr: { messages: [textMessage('user', 'B')] }
		};
		expect(classifyShapeBeacon(args)).toBe('lineage-broken');
	});

	it('returns "chrome-strip-failed" for lineage-broken with first-message-shape match', () => {
		const args = {
			shape: 'lineage-broken' as const,
			currLen: 1,
			prev: {
				messages: [
					{
						role: 'user',
						content: [{ type: 'text', text: 'identical' }],
						unknown_chrome_key: 'a'
					}
				]
			},
			curr: {
				messages: [
					{
						role: 'user',
						content: [{ type: 'text', text: 'identical' }],
						unknown_chrome_key: 'b'
					}
				]
			}
		};
		expect(classifyShapeBeacon(args)).toBe('chrome-strip-failed');
	});

	it('returns "lineage-broken" for lineage-broken with empty curr', () => {
		const args = {
			shape: 'lineage-broken' as const,
			currLen: 0,
			prev: { messages: [] },
			curr: { messages: [] }
		};
		expect(classifyShapeBeacon(args)).toBe('lineage-broken');
	});
});

describe('SpaDeltaFallbackReason — closed-set discipline', () => {
	it('the reason set is exactly five values (matches Rust W1.11 enum)', () => {
		const allReasons: SpaDeltaFallbackReason[] = [
			'predecessor-not-loaded',
			'compaction',
			'lineage-broken',
			'chrome-strip-failed',
			'parse-error'
		];
		expect(allReasons).toHaveLength(5);
	});
});
