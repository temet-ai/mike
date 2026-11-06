import type { Proposal, ProposalProjectsResponse, ProposalsResponse } from '$lib/api/proposals';

export const PROPOSAL_PROJECT_UUID = '6f1c2b8e-0d1a-4c55-9a0e-3b7f2e1d4c90';
export const PROPOSAL_OTHER_UUID = '0a9e4b21-77c3-4f10-8d2e-5c6b7a8f9e01';

export const PROPOSAL_PROJECTS: ProposalProjectsResponse = {
	undecided: 2,
	projects: [
		{
			project_uuid: PROPOSAL_OTHER_UUID,
			project_path: '/Users/sam/code/billing-service',
			display_name: 'billing-service',
			proposals: 1,
			undecided: 0
		},
		{
			project_uuid: PROPOSAL_PROJECT_UUID,
			project_path: '/Users/sam/code/storefront',
			display_name: 'storefront',
			proposals: 3,
			undecided: 2
		}
	]
};

export const PROPOSAL_NEW_RULE: Proposal = {
	lesson_id: 1303,
	placement: 'rule',
	kind: 'correction',
	stage: 'long_term',
	text: 'When adding storefront-core to Cargo.toml, keep its HTTPS tag URL and git-tag pin; do not replace it with a path dependency.',
	topic: 'dependency pins',
	covered_by: null,
	reason: 'This is a standing constraint for edits to Cargo.toml, and no rule shown covers it.',
	agent_name: null,
	user_prompt: 'no, put the tag pin back, the release build cannot see your local path',
	created_at: '2026-10-12T09:12:00Z',
	placed_at: '2026-10-14T10:02:00Z',
	decision: null,
	decided_at: null
};

export const PROPOSAL_EXISTING_RULE: Proposal = {
	...PROPOSAL_NEW_RULE,
	lesson_id: 1316,
	text: 'Keep checkout copy in locales/en/checkout.json; never inline it in a component.',
	topic: 'checkout copy',
	covered_by: '.claude/rules/frontend-copy.md',
	reason: 'A standing constraint on checkout components that belongs beside the other copy rules.',
	user_prompt: null,
	decision: 'accepted',
	decided_at: '2026-10-15T08:00:00Z'
};

export const PROPOSAL_AGENT: Proposal = {
	...PROPOSAL_NEW_RULE,
	lesson_id: 1320,
	placement: 'agent',
	kind: 'preference',
	text: 'The release reviewer reads the changelog before the diff and refuses a release without one.',
	topic: null,
	reason: 'This says how the release reviewer decides, which belongs in its definition.',
	agent_name: 'release-reviewer',
	user_prompt: null
};

export const PROJECT_PROPOSALS: ProposalsResponse = {
	project: PROPOSAL_PROJECT_UUID,
	proposals: [PROPOSAL_NEW_RULE, PROPOSAL_EXISTING_RULE, PROPOSAL_AGENT]
};
