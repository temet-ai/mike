import type {
	CodeFinding,
	CodeFindingsResponse,
	CodeProjectRules,
	CodeRuleCount,
	CodeRuleInfo,
	CodeSessionCounts
} from '$lib/api/codeFindings';

export const CODE_FINDINGS_SESSION_ID = '9d9b690e-cb59-4607-81f0-c96bdb3ff9b2';
export const CODE_FINDINGS_CHECKOUT = '/home/dev/projects/temet-proxy';

export function buildCodeFinding(overrides: Partial<CodeFinding> = {}): CodeFinding {
	return {
		id: 1,
		scan_id: 10,
		change_event_id: 100,
		session_id: CODE_FINDINGS_SESSION_ID,
		checkout: CODE_FINDINGS_CHECKOUT,
		path: 'scripts/deploy-proxy.sh',
		rule_id: 'sh-fixed-tmp-path',
		rule_source: 'builtin',
		severity: 'warning',
		message: 'A fixed /tmp path collides between runs. Use mktemp.',
		line: 12,
		col: 5,
		matched: '/tmp/deploy.lock',
		fix: 'mktemp -t deploy.XXXXXX',
		found_at: '2026-11-14T08:00:00Z',
		delivered_request_id: 4821,
		delivered_via: 'request',
		delivered_at: '2026-11-14T08:00:20Z',
		outcome: 'fixed',
		outcome_at: '2026-11-14T08:05:00Z',
		...overrides
	};
}

export const CODE_FINDINGS: CodeFinding[] = [
	buildCodeFinding({ id: 1 }),
	buildCodeFinding({
		id: 2,
		line: 31,
		col: 9,
		matched: '/tmp/deploy.log',
		fix: null,
		outcome: 'ignored',
		outcome_at: '2026-11-14T08:30:00Z',
		found_at: '2026-11-14T08:02:00Z'
	}),
	buildCodeFinding({
		id: 3,
		path: 'crates/mike-fleet/src/change_record/code_scan/mod.rs',
		rule_id: 'rust-no-comments',
		severity: 'error',
		message: 'Comments in Rust sources are rejected. Delete the comment or rename the item.',
		line: 88,
		col: 1,
		matched:
			'/// Scans the checkout for changed files\n/// and records each finding\n/// against the session that edited it.',
		fix: null,
		delivered_request_id: 4830,
		delivered_via: 'push',
		delivered_at: '2026-11-14T09:10:30Z',
		outcome: 'open',
		outcome_at: null,
		found_at: '2026-11-14T09:10:00Z'
	}),
	buildCodeFinding({
		id: 4,
		path: 'crates/mike-fleet/src/change_record/code_scan/mod.rs',
		rule_id: 'rust-temp-dir',
		severity: 'hint',
		message: 'Hand-rolled temporary directories leak. Use the shared test helper.',
		line: 140,
		col: 13,
		matched: 'std::env::temp_dir().join("scan")',
		fix: null,
		delivered_request_id: null,
		delivered_via: null,
		delivered_at: null,
		outcome: 'fixed_by_other',
		outcome_at: '2026-11-14T09:40:00Z',
		found_at: '2026-11-14T09:12:00Z'
	}),
	buildCodeFinding({
		id: 5,
		path: 'scripts/old-gate.sh',
		rule_id: 'sh-fixed-tmp-path',
		line: 4,
		col: 1,
		matched: '/tmp/gate',
		fix: null,
		delivered_request_id: null,
		delivered_via: null,
		delivered_at: null,
		outcome: 'superseded',
		outcome_at: '2026-11-14T09:50:00Z',
		found_at: '2026-11-14T09:00:00Z'
	})
];

export const CODE_SESSION_COUNTS: CodeSessionCounts = {
	files_scanned: 14,
	found: 5,
	told: 3,
	fixed_after_told: 1,
	fixed: 1,
	fixed_by_other: 1,
	ignored: 1,
	open: 1
};

export const CODE_RULE_COUNTS: CodeRuleCount[] = [
	{
		rule_id: 'sh-fixed-tmp-path',
		source: 'builtin',
		found: 41,
		delivered: 30,
		pushed: 12,
		fixed: 25,
		fixed_after_told: 21,
		fixed_untold: 4,
		fixed_by_other: 3,
		ignored: 6,
		open: 7,
		median_lag_s: 18.4
	},
	{
		rule_id: 'rust-no-comments',
		source: 'builtin',
		found: 120,
		delivered: 96,
		pushed: 40,
		fixed: 70,
		fixed_after_told: 64,
		fixed_untold: 6,
		fixed_by_other: 10,
		ignored: 22,
		open: 18,
		median_lag_s: 95
	},
	{
		rule_id: 'rust-temp-dir',
		source: 'builtin',
		found: 7,
		delivered: 0,
		pushed: 0,
		fixed: 0,
		fixed_after_told: 0,
		fixed_untold: 0,
		fixed_by_other: 0,
		ignored: 0,
		open: 7,
		median_lag_s: null
	},
	{
		rule_id: 'project-no-println',
		source: 'project',
		found: 3,
		delivered: 3,
		pushed: 1,
		fixed: 2,
		fixed_after_told: 2,
		fixed_untold: 0,
		fixed_by_other: 0,
		ignored: 1,
		open: 0,
		median_lag_s: 3700
	}
];

export const CODE_CATALOGUE: CodeRuleInfo[] = [
	{
		id: 'rust-no-comments',
		language: 'rust',
		severity: 'error',
		source: 'builtin',
		replaces: 'The rust-no-comments.sh pre-push check'
	},
	{
		id: 'rust-temp-dir',
		language: 'rust',
		severity: 'hint',
		source: 'builtin',
		replaces: null
	},
	{
		id: 'sh-fixed-tmp-path',
		language: 'bash',
		severity: 'warning',
		source: 'builtin',
		replaces: 'Reviewer comments about fixed /tmp paths in deploy scripts'
	},
	{
		id: 'project-no-println',
		language: 'rust',
		severity: 'warning',
		source: 'project',
		replaces: null
	}
];

export const CODE_PROJECT_RULES: CodeProjectRules[] = [
	{
		checkout: CODE_FINDINGS_CHECKOUT,
		files: ['.mike/rules/no-println.yml'],
		rules: ['project-no-println'],
		version: '4f2a9c1d03be',
		error: null
	},
	{
		checkout: '/home/dev/projects/holdfast',
		files: ['.mike/rules/bad.yml'],
		rules: [],
		version: null,
		error: "rule file .mike/rules/bad.yml: unknown field 'langauge' at line 3, column 1"
	}
];

export function buildCodeFindingsResponse(
	overrides: Partial<CodeFindingsResponse> = {}
): CodeFindingsResponse {
	return {
		scanning: true,
		delivering: true,
		pushing: false,
		grammars: ['bash', 'python', 'rust', 'typescript'],
		rules_version: '9c3b21e07a54',
		rule_ids: CODE_CATALOGUE.map((rule) => rule.id),
		catalogue: CODE_CATALOGUE,
		project_rules: CODE_PROJECT_RULES,
		watched_checkouts: [CODE_FINDINGS_CHECKOUT, '/home/dev/projects/holdfast', '/home/dev/gg'],
		session_counts: CODE_SESSION_COUNTS,
		hours: 720,
		summary: {
			scans: 1840,
			scanned: 1710,
			skipped: 120,
			failed: 10,
			findings_new: 171,
			findings_fixed: 97,
			median_us: 640,
			max_us: 48_300,
			last_scan_at: '2026-11-14T09:58:00Z'
		},
		rules: CODE_RULE_COUNTS,
		findings: CODE_FINDINGS,
		findings_truncated: false,
		...overrides
	};
}
