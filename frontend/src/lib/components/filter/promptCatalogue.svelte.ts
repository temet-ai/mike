import {
	ApiError,
	getPromptClassMain,
	getPromptClassSubagent,
	getPromptClassVersions,
	listPromptClasses,
	UNRESOLVED_PERSONA_SEGMENT,
	type GetPromptClassObservationsParams
} from '$lib/api';
import type {
	ObservedSystemBlock,
	PromptClassDetailDto,
	PromptClassDto,
	PromptClassVersionDto
} from '$lib/types';
import {
	classKey,
	classTitle,
	isMainLanePersonaRow,
	isNeverObserved,
	sortClasses,
	type PromptLane,
	type RowStatus
} from './promptCatalogue';

export type { RowStatus };

export interface CatalogueObservationRef {
	requestId: string;
	sessionId: string;
	createdAt: string;
}

export interface CatalogueModelRow {
	model: string | null;
	unscoped: boolean;
	windowedCount: number;
	latestObservation: CatalogueObservationRef | null;
	sampleBlocks: ObservedSystemBlock[] | null;
	observationsTotalCount: number | null;
	status: RowStatus;
	error: string | null;
}

export interface CatalogueClass {
	key: string;
	lane: PromptLane;
	persona: string | null;
	title: string;
	registered: boolean;
	registrationLabel: string | null;
	blockCount: number;
	observationCount: number;
	lastSeen: string | null;
	firstSeen: string | null;
	noRoute: boolean;
	models: CatalogueModelRow[];
	versionsStatus: RowStatus;
	versionsError: string | null;
	versions: PromptClassVersionDto[];
}

export interface PromptCatalogue {
	readonly classes: CatalogueClass[];
	readonly status: RowStatus;
	readonly error: string | null;
	readonly detailsSettled: boolean;
	load(): Promise<void>;
	retryModel(key: string, modelIndex: number): void;
	retryVersions(key: string): void;
}

export interface PromptCatalogueFetchers {
	loadClasses: typeof listPromptClasses;
	loadMainDetail: typeof getPromptClassMain;
	loadSubagentDetail: typeof getPromptClassSubagent;
	loadVersions: typeof getPromptClassVersions;
}

const DEFAULT_FETCHERS: PromptCatalogueFetchers = {
	loadClasses: listPromptClasses,
	loadMainDetail: getPromptClassMain,
	loadSubagentDetail: getPromptClassSubagent,
	loadVersions: getPromptClassVersions
};

function errorMessage(err: unknown): string {
	return err instanceof ApiError ? err.message : String(err);
}

function toObservationRef(
	observation: { request_id: string; session_id: string; created_at: string } | undefined
): CatalogueObservationRef | null {
	if (observation === undefined) return null;
	return {
		requestId: observation.request_id,
		sessionId: observation.session_id,
		createdAt: observation.created_at
	};
}

function buildClass(dto: PromptClassDto): CatalogueClass {
	const noRoute = isMainLanePersonaRow(dto);
	const base: CatalogueClass = {
		key: classKey(dto.lane, dto.subagent_persona),
		lane: dto.lane,
		persona: dto.subagent_persona,
		title: classTitle(dto.lane, dto.subagent_persona),
		registered: dto.registration.registered,
		registrationLabel: dto.registration.registered
			? (dto.registration.label ?? 'registered')
			: null,
		blockCount: dto.block_count,
		observationCount: dto.observation_count,
		lastSeen: dto.last_seen,
		firstSeen: dto.first_seen,
		noRoute,
		models: [],
		versionsStatus: noRoute ? 'ready' : 'loading',
		versionsError: null,
		versions: []
	};
	if (noRoute) return base;

	base.models =
		dto.model_breakdown.length > 0
			? dto.model_breakdown.map((m) => ({
					model: m.model,
					unscoped: false,
					windowedCount: m.count,
					latestObservation: null,
					sampleBlocks: null,
					observationsTotalCount: null,
					status: 'loading' as const,
					error: null
				}))
			: [
					{
						model: null,
						unscoped: true,
						windowedCount: 0,
						latestObservation: null,
						sampleBlocks: null,
						observationsTotalCount: null,
						status: 'loading' as const,
						error: null
					}
				];
	return base;
}

export function createPromptCatalogue(
	fetchers: PromptCatalogueFetchers = DEFAULT_FETCHERS
): PromptCatalogue {
	let classes = $state<CatalogueClass[]>([]);
	let status = $state<RowStatus>('loading');
	let error = $state<string | null>(null);
	let detailsSettled = $state(false);
	let inFlight: Promise<void> | null = null;
	const dtoByKey = new Map<string, PromptClassDto>();

	function detailParams(row: CatalogueModelRow): GetPromptClassObservationsParams {
		return row.unscoped ? { limit: 1 } : { model: row.model as string, limit: 1 };
	}

	function fetchDetail(
		dto: PromptClassDto,
		params: GetPromptClassObservationsParams
	): Promise<PromptClassDetailDto> {
		return dto.lane === 'main'
			? fetchers.loadMainDetail(params)
			: fetchers.loadSubagentDetail(dto.subagent_persona ?? UNRESOLVED_PERSONA_SEGMENT, params);
	}

	async function fetchModelRow(row: CatalogueModelRow, dto: PromptClassDto): Promise<void> {
		if (!row.unscoped && row.model === null) {
			row.status = 'ready';
			return;
		}
		row.status = 'loading';
		row.error = null;
		try {
			const detail = await fetchDetail(dto, detailParams(row));
			const latest = detail.recent_observations[0];
			if (row.unscoped) {
				row.model = latest?.model ?? null;
				row.observationsTotalCount = detail.observations_total_count;
			}
			row.latestObservation = toObservationRef(latest);
			row.sampleBlocks = detail.sample_blocks;
			row.status = 'ready';
		} catch (err) {
			row.status = 'error';
			row.error = errorMessage(err);
		}
	}

	async function fetchClassVersions(cls: CatalogueClass, dto: PromptClassDto): Promise<void> {
		cls.versionsStatus = 'loading';
		cls.versionsError = null;
		try {
			const result = await fetchers.loadVersions(dto.lane, dto.subagent_persona);
			cls.versions = result.versions;
			cls.versionsStatus = 'ready';
		} catch (err) {
			cls.versionsStatus = 'error';
			cls.versionsError = errorMessage(err);
		}
	}

	async function load(): Promise<void> {
		if (inFlight !== null) return inFlight;
		const promise = (async (): Promise<void> => {
			status = 'loading';
			error = null;
			detailsSettled = false;
			try {
				const dtos = await fetchers.loadClasses();
				const sorted = sortClasses(dtos);
				classes = sorted.map(buildClass);
				status = 'ready';
				for (const dto of sorted) {
					dtoByKey.set(classKey(dto.lane, dto.subagent_persona), dto);
				}

				const jobs: Array<Promise<void>> = [];
				for (let i = 0; i < sorted.length; i++) {
					const dto = sorted[i];
					const cls = classes[i];
					if (dto === undefined || cls === undefined || cls.noRoute) continue;
					for (const row of cls.models) jobs.push(fetchModelRow(row, dto));
					if (!isNeverObserved(dto)) jobs.push(fetchClassVersions(cls, dto));
				}
				await Promise.allSettled(jobs);
				detailsSettled = true;
			} catch (err) {
				classes = [];
				status = 'error';
				error = errorMessage(err);
				detailsSettled = true;
			} finally {
				inFlight = null;
			}
		})();
		inFlight = promise;
		return promise;
	}

	function retryModel(key: string, modelIndex: number): void {
		const dto = dtoByKey.get(key);
		const cls = classes.find((c) => c.key === key);
		if (dto === undefined || cls === undefined) return;
		const row = cls.models[modelIndex];
		if (row === undefined) return;
		void fetchModelRow(row, dto);
	}

	function retryVersions(key: string): void {
		const dto = dtoByKey.get(key);
		const cls = classes.find((c) => c.key === key);
		if (dto === undefined || cls === undefined) return;
		void fetchClassVersions(cls, dto);
	}

	return {
		get classes() {
			return classes;
		},
		get status() {
			return status;
		},
		get error() {
			return error;
		},
		get detailsSettled() {
			return detailsSettled;
		},
		load,
		retryModel,
		retryVersions
	};
}
