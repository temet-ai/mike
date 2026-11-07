import { describe, expect, it } from 'vitest';
import type { AssistantProposal } from '$lib/api/assistant';
import type { FilterRuleDto } from '$lib/types';
import { buildPreviewRules, planApplyProposal, resolveProposalRuleText } from './proposalRules';

function buildProposal(overrides: Partial<AssistantProposal> = {}): AssistantProposal {
	return {
		id: 'p1',
		title: 'Rewords the section',
		edit_id: null,
		action: 'replace',
		match: '# Session-specific guidance',
		before: '# Session-specific guidance',
		after: '# Session guidance',
		reach: 'This prompt only',
		...overrides
	};
}

function buildRule(overrides: Partial<FilterRuleDto> = {}): FilterRuleDto {
	return {
		id: 'r1',
		kind: 'text_substring',
		value: '# Session-specific guidance',
		action: 'replace',
		text: '# Session guidance (old)',
		title: null,
		enabled: true,
		source: 'ui',
		updated_at: '2026-09-01T00:00:00Z',
		trigger_token: null,
		sort_order: 1,
		fires_24h: 0,
		fires_7d: 0,
		last_fired_at: null,
		...overrides
	};
}

describe('resolveProposalRuleText', () => {
	it('drop always saves null, regardless of rule_text', () => {
		expect(resolveProposalRuleText(buildProposal({ action: 'drop', after: '' }))).toBeNull();
		expect(
			resolveProposalRuleText(buildProposal({ action: 'drop', after: '', rule_text: 'ignored' }))
		).toBeNull();
	});

	it('prefers rule_text when the service sends it, for every action', () => {
		expect(
			resolveProposalRuleText(
				buildProposal({
					action: 'append',
					match: '# Session-specific guidance',
					after: '# Session-specific guidance PROPOSAL_TAIL',
					rule_text: 'PROPOSAL_TAIL'
				})
			)
		).toBe('PROPOSAL_TAIL');
		expect(
			resolveProposalRuleText(
				buildProposal({
					action: 'replace',
					after: '# Session guidance',
					rule_text: '# Session guidance'
				})
			)
		).toBe('# Session guidance');
	});

	it('append without rule_text strips a leading match from after (pre-rule_text fallback)', () => {
		const text = resolveProposalRuleText(
			buildProposal({
				action: 'append',
				match: '# Session-specific guidance',
				after: '# Session-specific guidance PROPOSAL_TAIL'
			})
		);
		expect(text).toBe(' PROPOSAL_TAIL');
		expect(text).not.toContain('# Session-specific guidance');
	});

	it('append without rule_text and without a matching prefix falls back to after as-is', () => {
		const text = resolveProposalRuleText(
			buildProposal({ action: 'append', match: 'nomatch-here', after: 'brand new tail text' })
		);
		expect(text).toBe('brand new tail text');
	});

	it('replace without rule_text uses after as-is', () => {
		expect(
			resolveProposalRuleText(buildProposal({ action: 'replace', after: '# Session guidance' }))
		).toBe('# Session guidance');
	});
});

describe('buildPreviewRules', () => {
	it('appends a synthetic preview rule for a brand-new proposal (edit_id null)', () => {
		const enabled = [buildRule({ id: 'other' })];
		const proposal = buildProposal({ edit_id: null, id: 'p1' });
		const out = buildPreviewRules(enabled, proposal);
		expect(out).toHaveLength(2);
		// The synthetic preview rule's id comes from `proposal.edit_id` (null here → the
		// sentinel), never the proposal's OWN `id` — a brand-new proposal has no rule id yet.
		expect(out[1]?.id).toBe('__assistant_preview__');
		expect(out[1]?.text).toBe(resolveProposalRuleText(proposal));
	});

	it('replaces the matching enabled rule in place when edit_id names one already enabled', () => {
		const enabled = [buildRule({ id: 'r1' })];
		const proposal = buildProposal({ edit_id: 'r1', after: '# New text' });
		const out = buildPreviewRules(enabled, proposal);
		expect(out).toHaveLength(1);
		expect(out[0]?.id).toBe('r1');
		expect(out[0]?.text).toBe('# New text');
	});
});

describe('planApplyProposal', () => {
	it('creates a new rule when edit_id is null', () => {
		const plan = planApplyProposal(buildProposal({ edit_id: null }), null);
		expect(plan.kind).toBe('create');
		if (plan.kind === 'create') {
			expect(plan.body.title).toBe('Rewords the section');
			expect(plan.body.text).toBe(resolveProposalRuleText(buildProposal()));
		}
	});

	it('creates a new rule when edit_id names a rule that no longer exists', () => {
		const plan = planApplyProposal(buildProposal({ edit_id: 'gone' }), null);
		expect(plan.kind).toBe('create');
	});

	it('refuses when the existing rule is not a plain text_substring edit', () => {
		const rule = buildRule({ id: 'r1', kind: 'xml_tag', title: 'Drops the chrome section' });
		const plan = planApplyProposal(buildProposal({ edit_id: 'r1' }), rule);
		expect(plan.kind).toBe('refuse');
		if (plan.kind === 'refuse') {
			expect(plan.reason).toContain('Drops the chrome section');
		}
	});

	it('updates an existing rule WITHOUT touching its saved title', () => {
		const rule = buildRule({ id: 'r1', title: 'Operator-picked title' });
		const proposal = buildProposal({
			edit_id: 'r1',
			title: "Model's own title",
			after: '# New text'
		});
		const plan = planApplyProposal(proposal, rule);
		expect(plan.kind).toBe('update');
		if (plan.kind === 'update') {
			expect(plan.id).toBe('r1');
			expect(plan.patch.title).toBeUndefined();
			expect(plan.patch.action).toBe('replace');
			expect(plan.patch.value).toBe(proposal.match);
			expect(plan.patch.text).toBe('# New text');
		}
	});

	it('sets title from the proposal when the existing rule has no saved title', () => {
		const rule = buildRule({ id: 'r1', title: null });
		const proposal = buildProposal({ edit_id: 'r1', title: 'Derived title' });
		const plan = planApplyProposal(proposal, rule);
		expect(plan.kind).toBe('update');
		if (plan.kind === 'update') {
			expect(plan.patch.title).toBe('Derived title');
		}
	});

	it('overwrites a saved title only when useSuggestedTitle is explicitly set', () => {
		const rule = buildRule({ id: 'r1', title: 'Operator-picked title' });
		const proposal = buildProposal({ edit_id: 'r1', title: "Model's suggested title" });
		const plan = planApplyProposal(proposal, rule, { useSuggestedTitle: true });
		expect(plan.kind).toBe('update');
		if (plan.kind === 'update') {
			expect(plan.patch.title).toBe("Model's suggested title");
		}
	});

	it('saves null text for a drop proposal', () => {
		const proposal = buildProposal({ edit_id: null, action: 'drop', after: '' });
		const plan = planApplyProposal(proposal, null);
		expect(plan.kind).toBe('create');
		if (plan.kind === 'create') {
			expect(plan.body.text).toBeNull();
		}
	});

	it('creates a project-scoped rule when the proposal carries a project_path', () => {
		const proposal = buildProposal({ edit_id: null, project_path: '/mnt/data/projects/other' });
		const plan = planApplyProposal(proposal, null);
		expect(plan.kind).toBe('create');
		if (plan.kind === 'create') {
			expect(plan.body.project_path).toBe('/mnt/data/projects/other');
		}
	});

	it('omits project_path from a create body when the proposal is global', () => {
		const proposal = buildProposal({ edit_id: null });
		const plan = planApplyProposal(proposal, null);
		expect(plan.kind).toBe('create');
		if (plan.kind === 'create') {
			expect(plan.body.project_path).toBeUndefined();
		}
	});

	it('carries project_path onto an update patch when the proposal scopes an existing rule', () => {
		const rule = buildRule({ id: 'r1' });
		const proposal = buildProposal({ edit_id: 'r1', project_path: '/mnt/data/projects/other' });
		const plan = planApplyProposal(proposal, rule);
		expect(plan.kind).toBe('update');
		if (plan.kind === 'update') {
			expect(plan.patch.project_path).toBe('/mnt/data/projects/other');
		}
	});
});
