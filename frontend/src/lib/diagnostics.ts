import type { DiagnosticCategory } from './types';

export type Severity = 'error' | 'warning' | 'muted';

const CATEGORY_SEVERITY: Record<DiagnosticCategory, Severity> = {
	justifying_shortcuts: 'warning',
	deflecting_blame: 'warning',
	avoiding_work: 'error',
	faking_understanding: 'muted',
	overselling: 'muted'
};

export function severityOf(category: DiagnosticCategory): Severity {
	return CATEGORY_SEVERITY[category];
}

const SEVERITY_BADGE_CLASS: Record<Severity, string> = {
	error: 'badge-error',
	warning: 'badge-warning',
	muted: 'badge-muted'
};

export interface CategoryMeta {
	label: string;
	badgeClass: string;
}

export const CATEGORY_META: Record<DiagnosticCategory, CategoryMeta> = {
	justifying_shortcuts: {
		label: 'Justifying shortcuts',
		badgeClass: SEVERITY_BADGE_CLASS[severityOf('justifying_shortcuts')]
	},
	deflecting_blame: {
		label: 'Deflecting blame',
		badgeClass: SEVERITY_BADGE_CLASS[severityOf('deflecting_blame')]
	},
	avoiding_work: {
		label: 'Avoiding work',
		badgeClass: SEVERITY_BADGE_CLASS[severityOf('avoiding_work')]
	},
	faking_understanding: {
		label: 'Faking understanding',
		badgeClass: SEVERITY_BADGE_CLASS[severityOf('faking_understanding')]
	},
	overselling: {
		label: 'Overselling',
		badgeClass: SEVERITY_BADGE_CLASS[severityOf('overselling')]
	}
};

export const ALL_CATEGORIES = Object.keys(CATEGORY_META) as DiagnosticCategory[];

export function checkLabel(name: string): string {
	const base = name.replace(/_analyzer$/, '').replace(/_/g, ' ');
	return base.charAt(0).toUpperCase() + base.slice(1);
}
