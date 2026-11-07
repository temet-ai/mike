import { describe, expect, it } from 'vitest';
import type { ReviewDecision, ReviewerVerdict, ReviewPlace } from '$lib/api/skills';
import { installTextLabel, lessonsAsBullets, placementAdvice } from './skillEvidenceText';

function verdict(
	decision: ReviewDecision,
	belongs_in: ReviewPlace | null,
	overlap: string | null
): ReviewerVerdict {
	return { decision, reason: 'because', overlap, belongs_in };
}

describe('placementAdvice', () => {
	it('says an existing skill covers the lessons, naming it', () => {
		const a = placementAdvice(verdict('wrong_place', 'existing_skill', 'invoice-rounding'));
		expect(a?.sentence).toBe(
			'The reviewer advised against a new skill, because the skill invoice-rounding already covers these lessons.'
		);
		expect(a?.followUp).toBe(
			'mike does not yet move these lessons into invoice-rounding by itself.'
		);
	});

	it('says an existing skill covers the lessons when no name is known', () => {
		const a = placementAdvice(verdict('wrong_place', 'existing_skill', null));
		expect(a?.sentence).toBe(
			'The reviewer advised against a new skill, because an existing skill already covers these lessons.'
		);
		expect(a?.followUp).toBe('mike does not yet move these lessons into that skill by itself.');
	});

	it('advises a rule and says mike does not write rules', () => {
		const a = placementAdvice(verdict('wrong_place', 'rule', null));
		expect(a?.sentence).toBe(
			'The reviewer advised that these lessons work better as a rule in the project than as a skill.'
		);
		expect(a?.followUp).toBe(
			"mike does not write rules itself. Copy the lessons, then paste them into the project's CLAUDE.md or a file in .claude/rules."
		);
	});

	it('adds the overlap after a place that is not an existing skill', () => {
		expect(
			placementAdvice(verdict('wrong_place', 'rule', '.claude/rules/changelog.md'))?.sentence
		).toBe(
			'The reviewer advised that these lessons work better as a rule in the project than as a skill. It overlaps with .claude/rules/changelog.md.'
		);
	});

	it('advises an agent persona', () => {
		const a = placementAdvice(verdict('wrong_place', 'agent_persona', null));
		expect(a?.sentence).toBe(
			"The reviewer advised that these lessons belong in an agent's instructions, not in a skill."
		);
		expect(a?.followUp).toBeNull();
	});

	it('advises a memory file', () => {
		const a = placementAdvice(verdict('wrong_place', 'memory_file', null));
		expect(a?.sentence).toBe(
			'The reviewer advised that these lessons belong in a memory file, not in a skill.'
		);
		expect(a?.followUp).toBeNull();
	});

	it('advises keeping the lessons nowhere', () => {
		const a = placementAdvice(verdict('wrong_place', 'nowhere', null));
		expect(a?.sentence).toBe(
			'The reviewer advised that these lessons are not worth keeping anywhere, so no skill should be written.'
		);
		expect(a?.followUp).toBeNull();
	});

	it('says somewhere other than a skill when no place is given', () => {
		const a = placementAdvice(verdict('wrong_place', null, null));
		expect(a?.sentence).toBe(
			'The reviewer advised that these lessons belong somewhere other than a skill.'
		);
		expect(a?.followUp).toBeNull();
	});

	it('says nothing is worth saving, naming the skill that covers it', () => {
		const a = placementAdvice(verdict('nothing_to_save', null, 'invoice-rounding'));
		expect(a?.sentence).toBe(
			'The reviewer found nothing worth saving, because invoice-rounding already covers these lessons.'
		);
		expect(a?.followUp).toBeNull();
	});

	it('says nothing is worth saving without a name', () => {
		expect(placementAdvice(verdict('nothing_to_save', null, null))?.sentence).toBe(
			'The reviewer found nothing worth saving in these lessons.'
		);
	});

	it('gives no advice for an approval, a rewrite or an unreadable answer', () => {
		expect(placementAdvice(verdict('approve', null, null))).toBeNull();
		expect(placementAdvice(verdict('patch', null, null))).toBeNull();
		expect(placementAdvice(verdict('unusable', null, null))).toBeNull();
	});
});

describe('installTextLabel', () => {
	it('is quiet when the install holds the draft the row shows', () => {
		expect(installTextLabel({ draft_id: 17 }, 17, false)).toEqual({
			text: 'holds this draft',
			quiet: true
		});
		expect(installTextLabel({ draft_id: 17 }, 17, true)).toEqual({
			text: 'held this draft',
			quiet: true
		});
	});

	it('names the generated list when the install has no draft', () => {
		expect(installTextLabel({ draft_id: null }, 17, false)).toEqual({
			text: "holds mike's generated list of lessons",
			quiet: false
		});
		expect(installTextLabel({ draft_id: null }, null, true)).toEqual({
			text: "held mike's generated list of lessons",
			quiet: false
		});
	});

	it('names an earlier revision for any other draft', () => {
		expect(installTextLabel({ draft_id: 12 }, 17, false)).toEqual({
			text: 'holds an earlier revision',
			quiet: false
		});
		expect(installTextLabel({ draft_id: 12 }, null, true)).toEqual({
			text: 'held an earlier revision',
			quiet: false
		});
	});
});

describe('lessonsAsBullets', () => {
	it('writes one bullet per lesson that is not archived, keeping a multi-line lesson in one bullet', () => {
		const out = lessonsAsBullets([
			{ text: 'First.', archived_at: null },
			{ text: 'Gone.', archived_at: '2026-10-01' },
			{ text: 'Line one\nline two', archived_at: null }
		]);
		expect(out).toEqual({ count: 2, text: '- First.\n- Line one\n  line two' });
	});

	it('counts none when every lesson is archived', () => {
		expect(lessonsAsBullets([{ text: 'Gone.', archived_at: '2026-10-01' }]).count).toBe(0);
	});
});
