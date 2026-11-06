import { describe, it, expect } from 'vitest';
import { parseTeammateMessages, teammateMessagePreview } from './teammateMessage';

describe('parseTeammateMessages', () => {
	it('extracts teammate_id and inner text from a single envelope', () => {
		const userPrompt =
			'Another Claude session sent a message:\n' +
			'<teammate-message teammate_id="fix-timeline-dup-key" color="yellow">\n' +
			'{"type":"idle_notification","from":"fix-timeline-dup-key"}\n' +
			'</teammate-message>\n\n' +
			'This came from another Claude session — not typed by your user…';

		const out = parseTeammateMessages(userPrompt);
		expect(out).toEqual([
			{
				teammateId: 'fix-timeline-dup-key',
				summary: null,
				text: '{"type":"idle_notification","from":"fix-timeline-dup-key"}'
			}
		]);
	});

	it('extracts the summary attribute when present', () => {
		const userPrompt =
			'Another Claude session sent a message:\n' +
			'<teammate-message teammate_id="fix-tools-contradiction" color="green" summary="Tool-count/list mismatch fixed and verified">\n' +
			'Root cause: two independently-refreshed data sources.\n' +
			'</teammate-message>\n\n' +
			'This came from another Claude session…';

		const out = parseTeammateMessages(userPrompt);
		expect(out).toHaveLength(1);
		expect(out[0]).toMatchObject({
			teammateId: 'fix-tools-contradiction',
			summary: 'Tool-count/list mismatch fixed and verified'
		});
		expect(out[0]?.text).toContain('Root cause:');
	});

	it('extracts multiple envelopes in wire order when a turn bundles several peer messages', () => {
		const userPrompt =
			'<teammate-message teammate_id="alice" summary="First fix done">first body</teammate-message>\n' +
			'boilerplate warning text…\n' +
			'<teammate-message teammate_id="bob">second body</teammate-message>\n' +
			'more boilerplate…';

		const out = parseTeammateMessages(userPrompt);
		expect(out).toHaveLength(2);
		expect(out[0]).toMatchObject({
			teammateId: 'alice',
			summary: 'First fix done',
			text: 'first body'
		});
		expect(out[1]).toMatchObject({ teammateId: 'bob', summary: null, text: 'second body' });
	});

	it('returns an empty array when no envelope is present', () => {
		expect(parseTeammateMessages('just a normal operator prompt')).toEqual([]);
	});
});

describe('teammateMessagePreview', () => {
	it('prefers the summary attribute over the first line of text', () => {
		const preview = teammateMessagePreview([
			{
				teammateId: 'fix-tools-contradiction',
				summary: 'Tool-count/list mismatch fixed and verified',
				text: 'Root cause: …'
			}
		]);
		expect(preview).toBe('fix-tools-contradiction: Tool-count/list mismatch fixed and verified');
	});

	it('falls back to the first line of text when no summary attribute is present', () => {
		const preview = teammateMessagePreview([
			{
				teammateId: 'fix-prompt-dup',
				summary: null,
				text: 'Done. Files changed:\n- TurnCard.svelte'
			}
		]);
		expect(preview).toBe('fix-prompt-dup: Done. Files changed:');
	});

	it('appends a "(+N more)" suffix when the turn bundles more than one envelope', () => {
		const preview = teammateMessagePreview([
			{ teammateId: 'alice', summary: 'First fix done', text: 'first body' },
			{ teammateId: 'bob', summary: null, text: 'second body' },
			{ teammateId: 'carol', summary: null, text: 'third body' }
		]);
		expect(preview).toBe('alice: First fix done (+2 more)');
	});

	it('returns an empty string for an empty array', () => {
		expect(teammateMessagePreview([])).toBe('');
	});
});
