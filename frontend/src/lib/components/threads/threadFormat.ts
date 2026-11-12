import type { BadgeVariant } from '$lib/components/shared/Badge.variants';
import type { ProjectSummaryDto } from '$lib/types';

/** The project to show: the one asked for, else a pinned one, else the first. */
export function pickProject(projects: ProjectSummaryDto[], asked: string | null): string | null {
	if (asked && projects.some((p) => p.project_uuid === asked)) return asked;
	return (projects.find((p) => p.pinned) ?? projects[0])?.project_uuid ?? null;
}

export function statusVariant(status: string | null): BadgeVariant {
	switch (status) {
		case 'done':
			return 'tag-green';
		case 'review':
			return 'tag-blue';
		case 'implementing':
			return 'chip-active';
		case 'parked':
			return 'chip-muted';
		default:
			return 'tag-default';
	}
}

export function statusLabel(status: string | null): string {
	if (!status) return 'No status';
	return status.charAt(0).toUpperCase() + status.slice(1);
}

/** The criteria text is one criterion per line; list markers are display only. */
export function criteriaLines(text: string | null): string[] {
	if (!text) return [];
	return text
		.split('\n')
		.map((line) => line.replace(/^\s*(?:[-*]|\d+[.)])\s+/, '').trim())
		.filter((line) => line.length > 0);
}

export function splitLines(text: string): string[] {
	return text
		.split('\n')
		.map((line) => line.trim())
		.filter((line) => line.length > 0);
}
