import { ApiError } from '$lib/api';

export type CodeSeverity = 'error' | 'warning' | 'hint';
export type CodeOutcome = 'open' | 'fixed' | 'fixed_by_other' | 'ignored' | 'superseded';
export type CodeDeliveredVia = 'request' | 'push';
export type CodeRuleSource = 'builtin' | 'project';

export interface CodeFinding {
	id: number;
	scan_id: number;
	change_event_id: number;
	session_id: string | null;
	checkout: string;
	path: string;
	rule_id: string;
	rule_source: CodeRuleSource | string;
	severity: CodeSeverity;
	message: string;
	line: number;
	col: number;
	matched: string;
	fix: string | null;
	found_at: string;
	delivered_request_id: number | null;
	delivered_via: CodeDeliveredVia | null;
	delivered_at: string | null;
	outcome: CodeOutcome;
	outcome_at: string | null;
}

export interface CodeScanSummary {
	scans: number;
	scanned: number;
	skipped: number;
	failed: number;
	findings_new: number;
	findings_fixed: number;
	median_us: number;
	max_us: number;
	last_scan_at: string | null;
}

export interface CodeRuleCount {
	rule_id: string;
	source: CodeRuleSource | string;
	found: number;
	delivered: number;
	pushed: number;
	fixed: number;
	fixed_after_told: number;
	fixed_untold: number;
	fixed_by_other: number;
	ignored: number;
	open: number;
	median_lag_s: number | null;
}

export interface CodeSessionCounts {
	files_scanned: number;
	found: number;
	told: number;
	fixed_after_told: number;
	fixed: number;
	fixed_by_other: number;
	ignored: number;
	open: number;
}

export interface CodeRuleInfo {
	id: string;
	language: string;
	severity: CodeSeverity;
	source: CodeRuleSource | string;
	replaces: string | null;
}

export interface CodeProjectRules {
	checkout: string;
	files: string[];
	rules: string[];
	version: string | null;
	error: string | null;
}

export interface CodeFindingsResponse {
	scanning: boolean;
	delivering: boolean;
	pushing: boolean;
	grammars: string[];
	rules_version: string | null;
	rule_ids: string[] | null;
	catalogue: CodeRuleInfo[] | null;
	project_rules: CodeProjectRules[];
	watched_checkouts: string[];
	session_counts: CodeSessionCounts | null;
	hours: number;
	summary: CodeScanSummary;
	rules: CodeRuleCount[];
	findings: CodeFinding[];
	findings_truncated: boolean;
}

export interface CodeFindingsParams {
	session?: string;
	hours?: number;
	limit?: number;
}

export async function getCodeFindings(
	params: CodeFindingsParams = {}
): Promise<CodeFindingsResponse> {
	const query = new URLSearchParams();
	if (params.session) query.set('session', params.session);
	if (params.hours !== undefined) query.set('hours', String(params.hours));
	if (params.limit !== undefined) query.set('limit', String(params.limit));
	const qs = query.toString();
	const path = `/api/code-findings${qs ? `?${qs}` : ''}`;
	const res = await fetch(path, { headers: { accept: 'application/json' } });
	if (!res.ok) {
		throw new ApiError(`GET ${path} failed: ${res.status} ${res.statusText}`, res.status, path);
	}
	return (await res.json()) as CodeFindingsResponse;
}
