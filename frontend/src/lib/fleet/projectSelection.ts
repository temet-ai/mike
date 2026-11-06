import type { FleetProjectSummary } from './types';

/**
 * An explicit choice wins, even if an old or empty project is absent from the list.
 * With no choice, the project with the most recent activity is chosen; the path
 * decides between projects last seen at the same time, so the result does not
 * depend on the order of the list.
 */
export function chooseFleetProject(
	selected: string,
	projects: readonly FleetProjectSummary[]
): string {
	if (selected) return selected;
	let chosen: FleetProjectSummary | null = null;
	for (const project of projects) {
		if (!project.project_path) continue;
		if (chosen === null || isBetterDefault(project, chosen)) chosen = project;
	}
	return chosen?.project_path ?? '';
}

function lastSeenMs(project: FleetProjectSummary): number {
	const ms = Date.parse(project.last_seen);
	return Number.isNaN(ms) ? Number.NEGATIVE_INFINITY : ms;
}

function isBetterDefault(candidate: FleetProjectSummary, current: FleetProjectSummary): boolean {
	const candidateSeen = lastSeenMs(candidate);
	const currentSeen = lastSeenMs(current);
	if (candidateSeen !== currentSeen) return candidateSeen > currentSeen;
	return candidate.project_path < current.project_path;
}

/** Each response must still own the current request before updating the view. */
export class LatestFleetRequest {
	private generation = 0;

	begin(): () => boolean {
		const generation = ++this.generation;
		return () => generation === this.generation;
	}

	invalidate(): void {
		this.generation += 1;
	}
}
