import type {
	FilterRuleDto,
	ObservedSystemBlock,
	PromptClassDto,
	PromptClassVersionDto
} from '$lib/types';
import { countRuleMatches } from './workbench';

export type PromptLane = 'main' | 'subagent';

export type RowStatus = 'loading' | 'ready' | 'error';

export function classKey(lane: PromptLane, persona: string | null): string {
	return `${lane}:${persona ?? ''}`;
}

export function classTitle(lane: PromptLane, persona: string | null): string {
	if (persona !== null && persona.length > 0) return persona;
	return lane === 'main' ? 'Main session' : 'Unresolved persona';
}

export function isMainLanePersonaRow(
	dto: Pick<PromptClassDto, 'lane' | 'subagent_persona'>
): boolean {
	return dto.lane === 'main' && dto.subagent_persona !== null && dto.subagent_persona.length > 0;
}

export function isNeverObserved(dto: Pick<PromptClassDto, 'first_seen'>): boolean {
	return dto.first_seen === null;
}

interface SortableClassDto {
	lane: PromptLane;
	subagent_persona: string | null;
	first_seen: string | null;
}

export function sortClasses<T extends SortableClassDto>(dtos: readonly T[]): T[] {
	return [...dtos].sort((a, b) => {
		const laneRank = (dto: T): 0 | 1 => (dto.lane === 'main' ? 0 : 1);
		if (laneRank(a) !== laneRank(b)) return laneRank(a) - laneRank(b);

		const neverRank = (dto: T): 0 | 1 => (isNeverObserved(dto) ? 1 : 0);
		if (neverRank(a) !== neverRank(b)) return neverRank(a) - neverRank(b);

		const personaRank = (dto: T): 0 | 1 => {
			const isNullPersona = dto.subagent_persona === null;
			if (dto.lane === 'main') return isNullPersona ? 0 : 1;
			return isNullPersona ? 1 : 0;
		};
		if (personaRank(a) !== personaRank(b)) return personaRank(a) - personaRank(b);

		if (a.subagent_persona === null || b.subagent_persona === null) return 0;
		return a.subagent_persona.localeCompare(b.subagent_persona);
	});
}

export interface RequestClassIdentity {
	lane: PromptLane;
	persona: string | null;
}

export function classForTurnOrigin(
	turnOrigin: string | null | undefined,
	subagentPersona: string | null | undefined
): RequestClassIdentity | null {
	if (turnOrigin === 'primary_first') return { lane: 'main', persona: null };
	if (turnOrigin === 'subagent') return { lane: 'subagent', persona: subagentPersona ?? null };
	if (turnOrigin === 'workflow_subagent') return { lane: 'subagent', persona: null };
	return null;
}

export interface ApplicabilityModelSample {
	sampleBlocks: ObservedSystemBlock[] | null;
}

export interface ApplicabilityClass {
	key: string;
	title: string;
	models: readonly ApplicabilityModelSample[];
}

export interface ClassApplicability {
	classKey: string;
	title: string;
	matchedModels: number;
	totalModels: number;
}

export function ruleApplicability(
	rule: FilterRuleDto,
	classes: readonly ApplicabilityClass[]
): ClassApplicability[] {
	const out: ClassApplicability[] = [];
	for (const cls of classes) {
		let matchedModels = 0;
		let totalModels = 0;
		for (const model of cls.models) {
			if (model.sampleBlocks === null) continue;
			totalModels++;
			const blockTexts = model.sampleBlocks.map((b) => b.text);
			if (countRuleMatches(rule, blockTexts).count > 0) matchedModels++;
		}
		if (totalModels > 0) {
			out.push({ classKey: cls.key, title: cls.title, matchedModels, totalModels });
		}
	}
	return out;
}

export function applicabilityLabel(applicability: readonly ClassApplicability[]): string {
	const matched = applicability.filter((a) => a.matchedModels > 0);
	if (matched.length === 0) return 'Applies to: no captured prompt';
	const parts = matched.map((a) => `${a.title} (${a.matchedModels} of ${a.totalModels} models)`);
	return `Applies to: ${parts.join(' · ')}`;
}

const CC_VERSION_RELEASE_PATTERN = /cc_version=(\d+\.\d+\.\d+)/;

export function parseReleaseFromBlockText(blockText: string): string | null {
	return blockText.match(CC_VERSION_RELEASE_PATTERN)?.[1] ?? null;
}

export interface ModelFirstReleaseRow {
	isCurrent: boolean;
	version: string | null;
	builds: number;
	observationCount: number;
	firstSeen: string;
	lastSeen: string;
	latestRequestId: string;
	latestSessionId: string;
}

export function extractModelReleases(
	versions: readonly PromptClassVersionDto[],
	model: string
): ModelFirstReleaseRow[] {
	const rows: ModelFirstReleaseRow[] = [];
	for (const v of versions) {
		const m = v.models.find((entry) => entry.model === model);
		if (m === undefined) continue;
		rows.push({
			isCurrent: false,
			version: v.version,
			builds: v.builds,
			observationCount: m.observation_count,
			firstSeen: m.first_seen,
			lastSeen: m.last_seen,
			latestRequestId: m.latest_request_id,
			latestSessionId: m.latest_session_id
		});
	}
	if (rows.length > 0) rows[0]!.isCurrent = true;
	return rows;
}

export interface ModelFirstSourceModelRow {
	model: string | null;
	windowedCount: number;
	lastSeen: string | null;
	sampleBlocks: readonly ObservedSystemBlock[] | null;
	status: RowStatus;
	error: string | null;
}

export interface ModelFirstSourceClass {
	key: string;
	lane: PromptLane;
	title: string;
	registrationLabel: string | null;
	noRoute: boolean;
	neverObserved: boolean;
	versionsStatus: RowStatus;
	versionsError: string | null;
	versions: readonly PromptClassVersionDto[];
	modelRows: readonly ModelFirstSourceModelRow[];
}

export interface ModelFirstClassGroup {
	classKey: string;
	lane: PromptLane;
	title: string;
	versionsStatus: RowStatus;
	versionsError: string | null;
	releases: ModelFirstReleaseRow[];
	currentSampleBlocks: readonly ObservedSystemBlock[] | null;
	currentSampleStatus: RowStatus;
	currentSampleError: string | null;
}

export interface ModelFirstBlock {
	model: string;
	observationCount: number;
	lastSeen: string | null;
	classes: ModelFirstClassGroup[];
}

export interface NeverObservedRow {
	classKey: string;
	lane: PromptLane;
	title: string;
	registrationLabel: string | null;
}

export interface ModelFirstTree {
	models: ModelFirstBlock[];
	neverObserved: NeverObservedRow[];
}

export function buildModelFirstTree(classes: readonly ModelFirstSourceClass[]): ModelFirstTree {
	const neverObserved: NeverObservedRow[] = [];
	const byModel = new Map<string, ModelFirstBlock>();

	for (const cls of classes) {
		if (cls.noRoute || cls.neverObserved) {
			neverObserved.push({
				classKey: cls.key,
				lane: cls.lane,
				title: cls.title,
				registrationLabel: cls.registrationLabel
			});
			continue;
		}

		const modelIds = new Set<string>();
		for (const v of cls.versions) {
			for (const m of v.models) {
				if (m.model !== null) modelIds.add(m.model);
			}
		}
		if (modelIds.size === 0) {
			for (const row of cls.modelRows) {
				if (row.model !== null) modelIds.add(row.model);
			}
		}

		const modelRowByModel = new Map(cls.modelRows.map((r) => [r.model, r] as const));

		for (const model of modelIds) {
			const releases = extractModelReleases(cls.versions, model);
			const row = modelRowByModel.get(model);
			const group: ModelFirstClassGroup = {
				classKey: cls.key,
				lane: cls.lane,
				title: cls.title,
				versionsStatus: cls.versionsStatus,
				versionsError: cls.versionsError,
				releases,
				currentSampleBlocks: row?.sampleBlocks ?? null,
				currentSampleStatus: row?.status ?? 'ready',
				currentSampleError: row?.error ?? null
			};

			let block = byModel.get(model);
			if (block === undefined) {
				block = { model, observationCount: 0, lastSeen: null, classes: [] };
				byModel.set(model, block);
			}
			block.classes.push(group);
			if (row !== undefined) {
				block.observationCount += row.windowedCount;
				if (row.lastSeen !== null && (block.lastSeen === null || row.lastSeen > block.lastSeen)) {
					block.lastSeen = row.lastSeen;
				}
			}
		}
	}

	const models = [...byModel.values()].sort((a, b) => a.model.localeCompare(b.model));
	return { models, neverObserved };
}

export function splitReleasesForDisplay<T>(
	releases: readonly T[],
	visibleCount = 5
): { visible: readonly T[]; collapsed: readonly T[] } {
	if (releases.length <= visibleCount) return { visible: releases, collapsed: [] };
	const visible = releases.filter((_, index) => index < visibleCount);
	const collapsed = releases.filter((_, index) => index >= visibleCount);
	return { visible, collapsed };
}
