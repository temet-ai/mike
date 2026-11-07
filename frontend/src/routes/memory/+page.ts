import type { PageLoad } from './$types';
import { getConsolidationRuns, getMemoryDecisions } from '$lib/api/memoryDecisions';
import type { ConsolidationRun, ConsolidationAction, Paged } from '$lib/api/memoryDecisions';
import { getLessonRelations, getLessonOutcomes } from '$lib/api/memoryFeedback';
import type { LessonRelation, LessonOutcomeSummary } from '$lib/api/memoryFeedback';
import { getSettings } from '$lib/api/settings';
import { getFleetProjects } from '$lib/fleet/api';
import type { FleetProjectSummary } from '$lib/fleet/types';

const RUNS_LOOKBACK = 30;
const DECISIONS_PAGE_SIZE = 50;
const RELATIONS_PAGE_SIZE = 50;

export interface MemoryPageData {
	runs: ConsolidationRun[] | null;
	runsLoadError: string | null;
	decisions: Paged<ConsolidationAction> | null;
	decisionsLoadError: string | null;
	relations: Paged<LessonRelation> | null;
	relationsLoadError: string | null;
	lessonOutcomes: LessonOutcomeSummary[] | null;
	lessonOutcomesLoadError: string | null;
	projects: FleetProjectSummary[] | null;
	projectsLoadError: string | null;
	nightlyHourUtc: number | null;
}

function errorMessage(reason: unknown): string {
	return reason instanceof Error ? reason.message : String(reason);
}

export const load: PageLoad = async (): Promise<MemoryPageData> => {
	const [
		runsResult,
		decisionsResult,
		relationsResult,
		outcomesResult,
		settingsResult,
		projectsResult
	] = await Promise.allSettled([
		getConsolidationRuns({ limit: RUNS_LOOKBACK }),
		getMemoryDecisions({ limit: DECISIONS_PAGE_SIZE }),
		getLessonRelations({ limit: RELATIONS_PAGE_SIZE }),
		getLessonOutcomes(),
		getSettings(),
		getFleetProjects()
	]);

	const runs = runsResult.status === 'fulfilled' ? runsResult.value.items : null;
	const runsLoadError = runsResult.status === 'rejected' ? errorMessage(runsResult.reason) : null;

	const decisions = decisionsResult.status === 'fulfilled' ? decisionsResult.value : null;
	const decisionsLoadError =
		decisionsResult.status === 'rejected' ? errorMessage(decisionsResult.reason) : null;

	const relations = relationsResult.status === 'fulfilled' ? relationsResult.value : null;
	const relationsLoadError =
		relationsResult.status === 'rejected' ? errorMessage(relationsResult.reason) : null;

	const lessonOutcomes = outcomesResult.status === 'fulfilled' ? outcomesResult.value : null;
	const lessonOutcomesLoadError =
		outcomesResult.status === 'rejected' ? errorMessage(outcomesResult.reason) : null;

	const projects = projectsResult.status === 'fulfilled' ? projectsResult.value : null;
	const projectsLoadError =
		projectsResult.status === 'rejected' ? errorMessage(projectsResult.reason) : null;

	let nightlyHourUtc: number | null = null;
	if (settingsResult.status === 'fulfilled') {
		const setting = settingsResult.value.settings.find(
			(s) => s.key === 'lesson_worker.nightly_consolidation_hour'
		);
		if (setting && typeof setting.value === 'number') nightlyHourUtc = setting.value;
	}

	return {
		runs,
		runsLoadError,
		decisions,
		decisionsLoadError,
		relations,
		relationsLoadError,
		lessonOutcomes,
		lessonOutcomesLoadError,
		projects,
		projectsLoadError,
		nightlyHourUtc
	};
};
