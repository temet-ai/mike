import type { FleetProjectSummary } from './types';

/** An explicit choice wins, even if an old or empty project is absent from the list. */
export function chooseFleetProject(
	selected: string,
	projects: readonly FleetProjectSummary[]
): string {
	if (selected) return selected;
	return (
		projects
			.map((project) => project.project_path)
			.filter(Boolean)
			.sort()[0] ?? ''
	);
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
