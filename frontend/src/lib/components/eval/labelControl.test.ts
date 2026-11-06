import { describe, expect, it } from 'vitest';
import type { EvalLabelDto } from '$lib/api/eval';
import {
	axisWord,
	canWithdraw,
	changedEntries,
	currentGold,
	newestFirst,
	notesForSubject,
	planSave,
	splitWord,
	tierWord,
	wordFor
} from './labelControl';

function label(o: Partial<EvalLabelDto>): EvalLabelDto {
	return {
		id: 1,
		subject_kind: 'turn',
		subject_id: 7,
		operator_turn_id: 7,
		session_uuid: 's',
		axis: 'reaction',
		label: 'accepted',
		action: 'assert',
		tier: 'gold',
		method: 'operator_click',
		split: 'dev',
		tags: [],
		severity: null,
		confidence: null,
		evidence: null,
		note: null,
		source: 'spa',
		provenance: {},
		verdict_snapshot: null,
		supersedes_id: null,
		created_at: '2026-09-24T00:00:00Z',
		is_current: true,
		...o
	};
}

describe('labelControl', () => {
	it('spells labels out in words', () => {
		expect(wordFor('no_response')).toBe('No response');
		expect(wordFor('cut_off_neutral')).toBe('Cut off, neutral');
	});

	it('finds the current gold label per axis', () => {
		const labels = [
			label({ id: 1 }),
			label({ id: 2, label: 'corrected' }),
			label({ id: 3, tier: 'seed' })
		];
		expect(currentGold(labels, 'reaction')?.id).toBe(2);
	});

	it('only posts axes that differ from the current gold', () => {
		const labels = [label({ id: 2, label: 'accepted' })];
		expect(changedEntries({ reaction: 'accepted', delivery: 'partial' }, labels)).toEqual([
			{ axis: 'delivery', label: 'partial' }
		]);
	});

	it('orders newest first and allows withdrawing only current gold or relayed rows', () => {
		expect(newestFirst([label({ id: 1 }), label({ id: 5 })]).map((l) => l.id)).toEqual([5, 1]);
		expect(canWithdraw(label({ tier: 'seed' }))).toBe(false);
		expect(canWithdraw(label({ tier: 'relayed' }))).toBe(true);
		expect(canWithdraw(label({ is_current: false }))).toBe(false);
	});

	it('spells outcomes, axes, tiers and splits in everyday words', () => {
		expect(wordFor('unknown')).toBe('Unknown');
		expect(wordFor('success')).toBe('Success');
		expect(wordFor('some_new_label')).toBe('Some new label');
		expect(axisWord('session_end')).toBe('How it ended');
		expect(axisWord('reaction')).toBe('How you reacted');
		expect(tierWord('gold')).toBe('Yours');
		expect(tierWord('auto_shadow')).toBe('Automatic');
		expect(splitWord('holdout')).toBe('Measurement only');
		expect(splitWord('dev')).toBe('');
	});

	describe('planSave', () => {
		const labels = [label({ id: 2, label: 'accepted' })];

		it('sends the changed label', () => {
			expect(planSave({ reaction: 'corrected', delivery: null }, labels, '', true)).toEqual({
				kind: 'labels',
				entries: [{ axis: 'reaction', label: 'corrected' }]
			});
		});

		it('re-sends the selected labels when only a note is typed', () => {
			expect(planSave({ reaction: 'accepted', delivery: null }, labels, 'why', true)).toEqual({
				kind: 'labels',
				entries: [{ axis: 'reaction', label: 'accepted' }]
			});
		});

		it('has nothing to send when the label is unchanged and there is no note', () => {
			expect(planSave({ reaction: 'accepted', delivery: null }, labels, '  ', true)).toEqual({
				kind: 'nothing',
				reason: 'unchanged'
			});
		});

		it('saves a note to the notebook when no label is selected', () => {
			expect(planSave({ reaction: null, delivery: null }, [], '  first\nsecond  ', true)).toEqual({
				kind: 'note',
				title: 'first',
				body: 'first\nsecond'
			});
		});

		it('takes the title from the first non-empty line and keeps a long line whole', () => {
			const long = 'x'.repeat(200);
			expect(planSave({ reaction: null }, [], `\n   \n  ${long}  \nrest`, true)).toEqual({
				kind: 'note',
				title: long,
				body: `${long}  \nrest`
			});
		});

		it('needs a label for a note when there is no session to file it under', () => {
			expect(planSave({ reaction: null }, [], 'why', false)).toEqual({
				kind: 'nothing',
				reason: 'note-needs-label'
			});
		});

		it('asks for a label or a note when nothing is selected or typed', () => {
			expect(planSave({ reaction: null, delivery: null }, [], '', true)).toEqual({
				kind: 'nothing',
				reason: 'choose'
			});
		});
	});

	describe('notesForSubject', () => {
		const note = (id: string, data: unknown, created_at: string) => ({ id, data, created_at });
		const notes = [
			note('a', { turn_id: 7 }, '2026-09-29T10:00:00Z'),
			note('b', { turn_id: 8 }, '2026-09-29T11:00:00Z'),
			note('c', null, '2026-09-29T12:00:00Z'),
			note('d', { turn_id: 7 }, '2026-09-29T13:00:00Z')
		];

		it('keeps the notes for one turn, newest first', () => {
			expect(notesForSubject(notes, 7).map((n) => n.id)).toEqual(['d', 'a']);
		});

		it('keeps only the notes with no turn for a session', () => {
			expect(notesForSubject(notes, null).map((n) => n.id)).toEqual(['c']);
		});
	});
});
