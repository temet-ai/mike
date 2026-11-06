import {
	ApiError,
	getPromptClassMain,
	getPromptClassSubagent,
	listPromptClasses,
	UNRESOLVED_PERSONA_SEGMENT
} from '$lib/api';
import type { PromptClassDto } from '$lib/types';
import type { RequestClassIdentity } from './promptCatalogue';

/**
 * Resolves "the latest capture of the same model·class from that project" (spec) — the lookup
 * that drives the workbench's Project picker for anything other than Global. Injectable fetchers
 * mirror `promptCatalogue.svelte.ts`'s `PromptCatalogueFetchers`, so this stays testable without
 * a live API.
 */
export interface ProjectPromptLookupFetchers {
	loadMainDetail: typeof getPromptClassMain;
	loadSubagentDetail: typeof getPromptClassSubagent;
	loadClasses: typeof listPromptClasses;
}

export const DEFAULT_PROJECT_PROMPT_LOOKUP_FETCHERS: ProjectPromptLookupFetchers = {
	loadMainDetail: getPromptClassMain,
	loadSubagentDetail: getPromptClassSubagent,
	loadClasses: listPromptClasses
};

export interface ProjectPromptFound {
	kind: 'found';
	requestId: number;
}

/** No capture of this (lane, persona, model) exists in the picked project — the fallback UI
 *  shows one line saying so, plus the project's OWN available model·classes as links. */
export interface ProjectPromptNoCapture {
	kind: 'no_capture';
	availableClasses: PromptClassDto[];
}

export interface ProjectPromptLookupError {
	kind: 'error';
	message: string;
}

export type ProjectPromptLookupResult =
	| ProjectPromptFound
	| ProjectPromptNoCapture
	| ProjectPromptLookupError;

async function fallbackToProjectClasses(
	projectPath: string,
	fetchers: ProjectPromptLookupFetchers
): Promise<ProjectPromptLookupResult> {
	try {
		const availableClasses = await fetchers.loadClasses({ project_path: projectPath });
		return { kind: 'no_capture', availableClasses };
	} catch (err) {
		return { kind: 'error', message: err instanceof ApiError ? err.message : String(err) };
	}
}

export async function findLatestCaptureInProject(
	identity: RequestClassIdentity,
	model: string | null,
	projectPath: string,
	fetchers: ProjectPromptLookupFetchers = DEFAULT_PROJECT_PROMPT_LOOKUP_FETCHERS
): Promise<ProjectPromptLookupResult> {
	const params = { model: model ?? undefined, project_path: projectPath, limit: 1 };
	try {
		const detail =
			identity.lane === 'main'
				? await fetchers.loadMainDetail(params)
				: await fetchers.loadSubagentDetail(identity.persona ?? UNRESOLVED_PERSONA_SEGMENT, params);
		const latest = detail.recent_observations[0];
		if (latest === undefined) return fallbackToProjectClasses(projectPath, fetchers);
		const requestId = Number.parseInt(latest.request_id, 10);
		if (!Number.isFinite(requestId)) {
			return { kind: 'error', message: `malformed request_id "${latest.request_id}" in observation` };
		}
		return { kind: 'found', requestId };
	} catch (err) {
		if (err instanceof ApiError && err.status === 404) {
			return fallbackToProjectClasses(projectPath, fetchers);
		}
		return { kind: 'error', message: err instanceof ApiError ? err.message : String(err) };
	}
}
