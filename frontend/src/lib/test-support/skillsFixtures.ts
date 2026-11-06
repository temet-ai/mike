import type {
	SkillDraft,
	SkillDraftResponse,
	SkillFolder,
	SkillFoldersResponse,
	SkillLesson,
	SkillProjectsResponse
} from '$lib/api/skills';

export const PROJECT_UUID = '6f1c2b8e-0d1a-4c55-9a0e-3b7f2e1d4c90';
export const OTHER_UUID = '0a9e4b21-77c3-4f10-8d2e-5c6b7a8f9e01';

export const SKILL_PROJECTS: SkillProjectsResponse = {
	waiting: 1,
	projects: [
		{
			project_uuid: OTHER_UUID,
			project_path: '/Users/sam/code/billing-service',
			display_name: 'billing-service',
			mode: 'auto',
			mode_set: true,
			folders: 1,
			shipped: 1,
			skill_cap: 12,
			waiting: 0
		},
		{
			project_uuid: PROJECT_UUID,
			project_path: '/Users/sam/code/storefront',
			display_name: 'storefront',
			mode: 'ask',
			mode_set: false,
			folders: 3,
			shipped: 0,
			skill_cap: 12,
			waiting: 1
		}
	]
};

const BASE_FOLDER: SkillFolder = {
	folder: 'checkout-flow',
	draft_id: 41,
	status: 'accepted',
	review: null,
	revision: 1,
	lesson_watermark: 812,
	lesson_count: 4,
	description:
		'Change the storefront checkout without breaking payment retries. Use when editing the cart, payment step or order confirmation, or when a checkout test fails after a pricing change.',
	drafted_at: '2026-10-14T09:12:00Z',
	evaluated_at: '2026-10-14T10:02:00Z',
	edited_at: null,
	trigger_score: 0.95,
	patched_trigger_score: null,
	reviewer: {
		decision: 'approve',
		reason: 'The steps match the lessons and no existing skill covers the retry test.',
		overlap: null,
		belongs_in: null
	},
	verdict_reasons: [],
	approved_draft_id: null,
	first_written: null,
	live: false,
	installs: [],
	retired_by_user_at: null,
	live_use: null,
	state: 'waiting_on_you',
	needs_you: true
};

export const FOLDER_WAITING: SkillFolder = BASE_FOLDER;

export const FOLDER_FAILED: SkillFolder = {
	...BASE_FOLDER,
	folder: 'release-notes',
	draft_id: 42,
	status: 'rejected',
	lesson_count: 2,
	description:
		'Write release notes for storefront from merged pull requests. Use when preparing a release or when asked what changed since the last tag.',
	trigger_score: 0.9,
	reviewer: {
		decision: 'wrong_place',
		reason: 'A one-line constraint on CHANGELOG.md is a rule for that path, not a procedure.',
		overlap: '.claude/rules/changelog.md',
		belongs_in: 'rule'
	},
	verdict_reasons: [
		'the reviewer found the content belongs elsewhere (rule: .claude/rules/changelog.md): A one-line constraint on CHANGELOG.md is a rule for that path, not a procedure.'
	],
	state: 'failed_review',
	needs_you: false
};

export const FOLDER_SHIPPED: SkillFolder = {
	...BASE_FOLDER,
	folder: 'invoice-rounding',
	draft_id: 17,
	review: 'approved',
	approved_draft_id: 17,
	first_written: '2026-10-02',
	live: true,
	installs: [
		{
			machine_name: 'sams-laptop',
			path: '/Users/sam/code/billing-service/.claude/skills/invoice-rounding/SKILL.md',
			draft_id: 17,
			first_written: '2026-10-02',
			retired_at: null,
			retired_reason: null,
			reported_at: '2026-10-14T01:30:00Z'
		}
	],
	live_use: {
		first_shipped_on: '2026-10-02',
		window_start: '2026-10-02T00:00:00Z',
		window_end: '2026-10-14T01:30:00Z',
		loads: 9,
		load_sessions: 6,
		loaded_turns: 9,
		judged_turns: 8,
		corrected_turns: 1,
		base_judged_turns: 120,
		base_corrected_turns: 24,
		retired_at: null,
		retired_reason: null,
		measured_at: '2026-10-14T01:30:00Z'
	},
	state: 'shipped',
	needs_you: false
};

export const FOLDER_GENERATED: SkillFolder = {
	...BASE_FOLDER,
	folder: 'deploy-and-restart',
	draft_id: null,
	status: null,
	revision: null,
	lesson_watermark: null,
	lesson_count: null,
	description: null,
	trigger_score: null,
	reviewer: null,
	verdict_reasons: null,
	state: 'generated_only',
	needs_you: false
};

export const FOLDER_RETIRED: SkillFolder = {
	...FOLDER_SHIPPED,
	folder: 'tax-lookup',
	draft_id: 23,
	approved_draft_id: 23,
	live: false,
	installs: [
		{
			machine_name: 'sams-laptop',
			path: '/Users/sam/code/billing-service/.claude/skills/tax-lookup/SKILL.md',
			draft_id: 23,
			first_written: '2026-09-20',
			retired_at: '2026-10-13',
			retired_reason: 'corrected_above_base',
			reported_at: '2026-10-13T01:30:00Z'
		}
	],
	live_use: {
		first_shipped_on: '2026-09-20',
		window_start: '2026-09-20T00:00:00Z',
		window_end: '2026-10-13T01:30:00Z',
		loads: 14,
		load_sessions: 9,
		loaded_turns: 13,
		judged_turns: 12,
		corrected_turns: 5,
		base_judged_turns: 140,
		base_corrected_turns: 25,
		retired_at: '2026-10-13T01:30:00Z',
		retired_reason: 'corrected_above_base',
		measured_at: '2026-10-13T01:30:00Z'
	},
	state: 'retired_corrected'
};

/** A shipped folder whose newest draft the reviewer says an existing skill already covers. */
export const FOLDER_COVERED: SkillFolder = {
	...FOLDER_SHIPPED,
	folder: 'refund-rounding',
	draft_id: 44,
	approved_draft_id: 44,
	live_use: null,
	reviewer: {
		decision: 'wrong_place',
		reason: 'The rounding steps repeat what invoice-rounding already tells Claude to do.',
		overlap: 'invoice-rounding',
		belongs_in: 'existing_skill'
	},
	installs: [
		{
			machine_name: 'sams-laptop',
			path: '/Users/sam/code/billing-service/.claude/skills/refund-rounding/SKILL.md',
			draft_id: 44,
			first_written: '2026-10-05',
			retired_at: null,
			retired_reason: null,
			reported_at: '2026-10-14T01:30:00Z'
		},
		{
			machine_name: 'sams-desktop',
			path: '/Users/sam/code/billing-service/.claude/skills/refund-rounding/SKILL.md',
			draft_id: 31,
			first_written: '2026-10-05',
			retired_at: null,
			retired_reason: null,
			reported_at: '2026-10-14T01:30:00Z'
		},
		{
			machine_name: 'ci-runner',
			path: '/srv/billing-service/.claude/skills/refund-rounding/SKILL.md',
			draft_id: null,
			first_written: '2026-10-05',
			retired_at: null,
			retired_reason: null,
			reported_at: '2026-10-14T01:30:00Z'
		}
	]
};

export const PROJECT_SKILLS: SkillFoldersResponse = {
	project: PROJECT_UUID,
	mode: 'ask',
	mode_set: false,
	skills: [FOLDER_WAITING, FOLDER_FAILED, FOLDER_GENERATED]
};

export const SKILL_LESSONS: SkillLesson[] = [
	{
		id: 801,
		kind: 'correction',
		stage: 'long_term',
		text: 'Run the payment retry test after any change to the cart total; a rounding change once double-charged retries.',
		topic: 'checkout-flow',
		archived_at: null,
		user_prompt: 'you changed the total again and the retry test is red, run it before saying done'
	},
	{
		id: 812,
		kind: 'convention',
		stage: 'daily',
		text: 'Order confirmation copy lives in locales/en/checkout.json, never inline in the component.',
		topic: 'checkout-flow',
		archived_at: null,
		user_prompt: null
	}
];

export const SKILL_DRAFT: SkillDraft = {
	id: 41,
	project_id: 9,
	project_uuid: PROJECT_UUID,
	project_path: '/Users/sam/code/storefront',
	folder: 'checkout-flow',
	agent_name: null,
	status: 'accepted',
	review: null,
	reviewed_at: null,
	edited_at: null,
	revision: 1,
	lesson_watermark: 812,
	lesson_ids: [801, 812],
	model: 'claude-sonnet-5-5',
	description: BASE_FOLDER.description ?? '',
	skill_md:
		'---\nname: checkout-flow\ndescription: "Change the storefront checkout without breaking payment retries."\n---\n\nRun `pnpm test checkout/retry` after any change to the cart total, because a rounding change once double-charged retries.\n\nRead [references/lessons.md](references/lessons.md) for the lessons behind this skill.\n',
	authored_skill_md: null,
	reference_files: [
		{
			name: 'copy-locations',
			when_to_read: 'you change text a customer sees during checkout.',
			content: '# Where checkout copy lives\n\nlocales/en/checkout.json holds every string.\n'
		}
	],
	trigger_queries: [],
	eval: {
		evaluator_version: 3,
		models: ['claude-sonnet-5-5'],
		calls: 3,
		verdict: {
			accepted: true,
			reasons: [],
			min_trigger_score: 0.8
		},
		trigger: {
			score: 0.95,
			correct: 19,
			total: 20,
			results: [
				{
					query: 'Round the cart total half up instead of banker’s rounding.',
					should_trigger: true,
					triggered: true,
					correct: true,
					loaded: ['checkout-flow']
				},
				{
					query: 'Update the README badges.',
					should_trigger: false,
					triggered: true,
					correct: false,
					loaded: ['checkout-flow']
				}
			]
		},
		review: {
			decision: 'approve',
			reason: 'The steps match the lessons and no existing skill covers the retry test.',
			overlap: null,
			belongs_in: null,
			original_description: null,
			original_skill_md: null
		},
		recheck: null
	},
	created_at: '2026-10-14T09:12:00Z',
	evaluated_at: '2026-10-14T10:02:00Z'
};

export const SKILL_DRAFT_RESPONSE: SkillDraftResponse = {
	draft: SKILL_DRAFT,
	lessons: SKILL_LESSONS,
	revisions: [
		{
			id: 41,
			revision: 1,
			lesson_watermark: 812,
			status: 'accepted',
			review: null,
			created_at: '2026-10-14T09:12:00Z'
		}
	],
	folder: FOLDER_WAITING,
	mode: 'ask'
};
