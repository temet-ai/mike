import {
	CircleCheck,
	CircleDashed,
	CircleX,
	EyeOff,
	FileX,
	Info,
	TriangleAlert,
	UserCheck
} from '@lucide/svelte';
import type { Component } from 'svelte';
import type { CodeFinding, CodeOutcome, CodeRuleCount, CodeSeverity } from '$lib/api/codeFindings';
import { formatDuration } from '$lib/time';

type Glyph = Component<{
	size?: number | string;
	class?: string;
	'aria-hidden'?: 'true' | boolean;
}>;

export type MarkTone = 'success' | 'danger' | 'warning' | 'info' | 'strong' | 'quiet';

export interface MarkSpec {
	icon: Glyph;
	label: string;
	tone: MarkTone;
}

export const OUTCOME_MARKS: Record<CodeOutcome, MarkSpec> = {
	fixed: { icon: CircleCheck, label: 'fixed', tone: 'success' },
	fixed_by_other: { icon: UserCheck, label: 'fixed by other', tone: 'success' },
	ignored: { icon: EyeOff, label: 'ignored', tone: 'strong' },
	open: { icon: CircleDashed, label: 'open', tone: 'quiet' },
	superseded: { icon: FileX, label: 'file deleted', tone: 'quiet' }
};

export const SEVERITY_MARKS: Record<CodeSeverity, MarkSpec> = {
	error: { icon: CircleX, label: 'error', tone: 'danger' },
	warning: { icon: TriangleAlert, label: 'warning', tone: 'warning' },
	hint: { icon: Info, label: 'hint', tone: 'info' }
};

export function outcomeMark(outcome: string): MarkSpec {
	return Object.hasOwn(OUTCOME_MARKS, outcome)
		? OUTCOME_MARKS[outcome as CodeOutcome]
		: { icon: CircleDashed, label: outcome, tone: 'quiet' };
}

export function severityMark(severity: string): MarkSpec {
	return Object.hasOwn(SEVERITY_MARKS, severity)
		? SEVERITY_MARKS[severity as CodeSeverity]
		: { icon: Info, label: severity, tone: 'quiet' };
}

export const COUNT_TOOLTIPS = {
	files_scanned: 'Files this session edited that a rule could read.',
	found: 'Findings its edits added.',
	told: 'Findings sent to the session in a request or by push.',
	fixed_after_told: 'Told, then fixed by the session.',
	ignored: 'Told, and still there when the session committed.',
	open: 'Not fixed yet.',
	fixed_by_other: 'Findings a different session fixed.'
} as const;

export const COLUMN_TOOLTIPS = {
	found: 'Findings edits added.',
	told: 'Sent to the session in a request or by push.',
	pushed: 'Told by push rather than in a request.',
	fixed_after_told: 'Told, then fixed by the same session.',
	fixed_untold: 'Fixed before the session was told.',
	fixed_by_other: 'Fixed by a different session.',
	ignored: 'Told, and still there when the session committed.',
	open: 'Not settled yet.',
	fix_rate: 'Share of told findings the session then fixed.',
	told_after: 'Median time from finding to telling the session.',
	replaces: 'The lint, hook or habit this rule makes redundant.',
	project: 'A project rule file added this rule.'
} as const;

export interface FindingGroup {
	key: string;
	checkout: string;
	path: string;
	rule_id: string;
	severity: CodeSeverity;
	message: string;
	oldestFoundAt: number;
	minId: number;
	findings: CodeFinding[];
}

function foundAtMs(finding: CodeFinding): number {
	const ms = Date.parse(finding.found_at);
	return Number.isNaN(ms) ? 0 : ms;
}

export function groupFindings(findings: readonly CodeFinding[]): FindingGroup[] {
	const byKey = new Map<string, FindingGroup>();
	for (const finding of findings) {
		const key = `${finding.checkout}\n${finding.path}\n${finding.rule_id}`;
		const at = foundAtMs(finding);
		const group = byKey.get(key);
		if (group) {
			group.findings.push(finding);
			group.oldestFoundAt = Math.min(group.oldestFoundAt, at);
			group.minId = Math.min(group.minId, finding.id);
		} else {
			byKey.set(key, {
				key,
				checkout: finding.checkout,
				path: finding.path,
				rule_id: finding.rule_id,
				severity: finding.severity,
				message: finding.message,
				oldestFoundAt: at,
				minId: finding.id,
				findings: [finding]
			});
		}
	}
	const groups = [...byKey.values()];
	for (const group of groups) {
		group.findings.sort((a, b) => a.line - b.line || a.col - b.col || a.id - b.id);
	}
	return groups.sort((a, b) => b.oldestFoundAt - a.oldestFoundAt || a.minId - b.minId);
}

export function fixRate(fixedAfterTold: number, told: number): string {
	if (told <= 0) return '—';
	return `${Math.round((fixedAfterTold / told) * 100)}%`;
}

export function formatMicros(us: number): string {
	if (us < 1_000) return `${Math.round(us)} µs`;
	const ms = us / 1_000;
	return `${ms < 10 ? ms.toFixed(1) : Math.round(ms)} ms`;
}

export function formatLag(seconds: number | null): string {
	return seconds === null ? '—' : formatDuration(seconds * 1_000);
}

export function sortRules(rules: readonly CodeRuleCount[]): CodeRuleCount[] {
	return [...rules].sort((a, b) => b.found - a.found || a.rule_id.localeCompare(b.rule_id));
}

export interface RuleTotals {
	found: number;
	delivered: number;
	pushed: number;
	fixed_after_told: number;
	fixed_untold: number;
	fixed_by_other: number;
	ignored: number;
	open: number;
}

export function totalRules(rules: readonly CodeRuleCount[]): RuleTotals {
	const totals: RuleTotals = {
		found: 0,
		delivered: 0,
		pushed: 0,
		fixed_after_told: 0,
		fixed_untold: 0,
		fixed_by_other: 0,
		ignored: 0,
		open: 0
	};
	for (const rule of rules) {
		totals.found += rule.found;
		totals.delivered += rule.delivered;
		totals.pushed += rule.pushed;
		totals.fixed_after_told += rule.fixed_after_told;
		totals.fixed_untold += rule.fixed_untold;
		totals.fixed_by_other += rule.fixed_by_other;
		totals.ignored += rule.ignored;
		totals.open += rule.open;
	}
	return totals;
}

export function plural(count: number, one: string, many = `${one}s`): string {
	return count === 1 ? one : many;
}
