import type { PageLoad } from './$types';
import type { Paged } from '$lib/api/memoryDecisions';
import { getLessonRelations, getLessonOutcomes } from '$lib/api/memoryFeedback';
import type { LessonRelation, LessonOutcomeSummary } from '$lib/api/memoryFeedback';
import { getFleetProjects } from '$lib/fleet/api';
import type { FleetProjectSummary } from '$lib/fleet/types';

const RELATIONS_PAGE_SIZE = 50;

export interface MemoryPageData {
	relations: Paged<LessonRelation> | null;
	relationsLoadError: string | null;
	lessonOutcomes: LessonOutcomeSummary[] | null;
	lessonOutcomesLoadError: string | null;
	projects: FleetProjectSummary[] | null;
	projectsLoadError: string | null;
}

function errorMessage(reason: unknown): string {
	return reason instanceof Error ? reason.message : String(reason);
}

export const load: PageLoad = async (): Promise<MemoryPageData> => {
	const [relationsResult, outcomesResult, projectsResult] = await Promise.allSettled([
		getLessonRelations({ limit: RELATIONS_PAGE_SIZE }),
		getLessonOutcomes(),
		getFleetProjects()
	]);

	return {
		relations: relationsResult.status === 'fulfilled' ? relationsResult.value : null,
		relationsLoadError:
			relationsResult.status === 'rejected' ? errorMessage(relationsResult.reason) : null,
		lessonOutcomes: outcomesResult.status === 'fulfilled' ? outcomesResult.value : null,
		lessonOutcomesLoadError:
			outcomesResult.status === 'rejected' ? errorMessage(outcomesResult.reason) : null,
		projects: projectsResult.status === 'fulfilled' ? projectsResult.value : null,
		projectsLoadError:
			projectsResult.status === 'rejected' ? errorMessage(projectsResult.reason) : null
	};
};
