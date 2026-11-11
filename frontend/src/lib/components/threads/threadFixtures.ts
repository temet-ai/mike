import type {
	ThreadDetail,
	ThreadEntry,
	ThreadListResponse,
	ThreadListRow
} from '$lib/api/workThreads';
import type { ProjectSummaryDto } from '$lib/types';

export const THREAD_PROJECT_UUID = '3d6f1a52-8b0e-4c71-9a52-1e0f7c4b2d18';

export const THREAD_PROJECTS: ProjectSummaryDto[] = [
	{
		project_id: 2,
		project_uuid: '9a1b7c30-5d24-4e8f-b6a3-0c2d9e1f4a77',
		project_path: '/Users/sam/code/billing-service',
		first_seen: '2026-09-01T08:00:00Z',
		last_seen: '2026-11-08T17:20:00Z',
		pinned: false,
		custom_name: null
	},
	{
		project_id: 1,
		project_uuid: THREAD_PROJECT_UUID,
		project_path: '/Users/sam/code/storefront',
		first_seen: '2026-08-12T08:00:00Z',
		last_seen: '2026-11-11T09:30:00Z',
		pinned: true,
		custom_name: 'storefront'
	}
];

export const THREAD_ROWS: ThreadListRow[] = [
	{
		slug: 'checkout-rewrite',
		title: 'Checkout rewrite',
		status: 'implementing',
		tldr: 'Cart totals move to the pricing service; the old totals module is read-only until the cutover.',
		next_up: 'Port the discount rules, then switch the staging flag.',
		parse_error: null,
		updated_at: '2026-11-10T15:44:00Z'
	},
	{
		slug: 'search-latency',
		title: 'Search latency',
		status: 'review',
		tldr: 'The product search index is rebuilt nightly; p95 dropped from 900 ms to 210 ms.',
		next_up: 'Read the acceptance criteria against the staging numbers.',
		parse_error: null,
		updated_at: '2026-11-09T11:02:00Z'
	},
	{
		slug: 'legacy-import',
		title: 'Legacy import',
		status: 'parked',
		tldr: 'Parked until the vendor sends the new export format.',
		next_up: '',
		parse_error: 'worklog.md: heading "Where we are" appears twice',
		updated_at: '2026-10-02T09:12:00Z'
	}
];

export const THREAD_LIST: ThreadListResponse = { threads: THREAD_ROWS, total: THREAD_ROWS.length };

export const THREAD_ENTRIES: ThreadEntry[] = [
	{
		ord: 3,
		kind: 'decision',
		summary: 'Keep the old totals module read-only until cutover',
		body: 'Both modules writing totals produced drift in staging. The old module stays readable for reports and refuses writes from today.',
		session_uuid: '5b0c8e12-4f6a-4d3b-9c71-2a8e6d0f1b34',
		commit_sha: null,
		data: null,
		entry_date: '2026-11-10',
		created_at: '2026-11-10T15:44:00Z'
	},
	{
		ord: 2,
		kind: 'checkpoint',
		summary: 'Discount rules ported for three of five coupon types',
		body: 'Percentage, fixed and free-shipping coupons pass the pricing service tests.\nBundle and tiered coupons still need the cart line grouping.',
		session_uuid: '5b0c8e12-4f6a-4d3b-9c71-2a8e6d0f1b34',
		commit_sha: '4f9a2c1d7e0b',
		data: null,
		entry_date: '2026-11-09',
		created_at: '2026-11-09T18:20:00Z'
	},
	{
		ord: 1,
		kind: 'brief',
		summary: null,
		body: 'Move cart totals to the pricing service. Start with the discount rules; keep the checkout page working throughout.',
		session_uuid: null,
		commit_sha: null,
		data: null,
		entry_date: '2026-11-08',
		created_at: '2026-11-08T08:00:00Z'
	}
];

export const THREAD_DETAIL: ThreadDetail = {
	thread: {
		id: 11,
		slug: 'checkout-rewrite',
		title: 'Checkout rewrite',
		goal: 'Cart totals come from the pricing service and the old totals module is removed.',
		status: 'implementing',
		branch: 'feat/pricing-service-totals',
		worktree: '.claude/worktrees/checkout',
		source_path: null,
		parse_error: null,
		last_session_uuid: '5b0c8e12-4f6a-4d3b-9c71-2a8e6d0f1b34',
		created_at: '2026-11-08T08:00:00Z',
		updated_at: '2026-11-10T15:44:00Z'
	},
	state: {
		tldr: 'Cart totals move to the pricing service; the old totals module is read-only until the cutover.',
		where_we_are:
			'Three of five coupon types are ported. The staging flag is off.\nCheckout renders totals from the old module.',
		next_up: 'Port bundle and tiered coupons, then switch the staging flag.',
		blockers: 'Cart line grouping lives in the old module and has no tests.',
		open_questions: 'Does the mobile app read totals from the same endpoint?',
		decisions: 'Old totals module is read-only from 2026-11-10.',
		files: ['services/pricing/discounts.ts', 'checkout/totals.ts'],
		updated_at: '2026-11-10T15:44:00Z'
	},
	acceptance_criteria:
		'- Checkout totals match the pricing service for every coupon type in staging\n- The old totals module has no remaining importers',
	depends_on: ['pricing-service'],
	depended_on_by: ['search-latency'],
	history: THREAD_ENTRIES,
	history_total: THREAD_ENTRIES.length,
	history_next_before_ord: null,
	sessions: [
		{
			session_uuid: '5b0c8e12-4f6a-4d3b-9c71-2a8e6d0f1b34',
			contribution: 'Ported the first three coupon types.',
			first_seen_at: '2026-11-08T08:10:00Z',
			last_seen_at: '2026-11-10T15:44:00Z'
		},
		{
			session_uuid: 'c7d2f0a9-31e5-4b86-8f14-6a9b0e3d5c21',
			contribution: '',
			first_seen_at: '2026-11-09T12:00:00Z',
			last_seen_at: '2026-11-09T12:40:00Z'
		}
	]
};
