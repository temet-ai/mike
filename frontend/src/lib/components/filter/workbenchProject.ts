/**
 * URL-param and localStorage plumbing for the filter workbench's Project picker (Global vs one
 * captured project). Pure functions so `FilterWorkbench.svelte`'s wiring stays testable without
 * mounting a component — mirrors `workbenchLayout.ts`'s split between pure logic and the Svelte
 * effects that call it.
 */

export const WORKBENCH_PROJECT_STORAGE_KEY = 'proxy-inspector:filter-workbench-project';

export const GLOBAL_PROJECT_PARAM = 'global';

/** `null` means Global. `?project=global`, a missing param, or an empty value all parse to it. */
export function parseProjectParam(raw: string | null): string | null {
	if (raw === null || raw.length === 0 || raw === GLOBAL_PROJECT_PARAM) return null;
	return raw;
}

/** The `project=` URL value for a picked project (`null` = Global). */
export function formatProjectParam(projectPath: string | null): string {
	return projectPath ?? GLOBAL_PROJECT_PARAM;
}

export function loadRememberedProject(): string | null {
	try {
		const raw = localStorage.getItem(WORKBENCH_PROJECT_STORAGE_KEY);
		return raw === null || raw.length === 0 ? null : raw;
	} catch {
		return null;
	}
}

export function saveRememberedProject(projectPath: string | null): void {
	try {
		if (projectPath === null) {
			localStorage.removeItem(WORKBENCH_PROJECT_STORAGE_KEY);
		} else {
			localStorage.setItem(WORKBENCH_PROJECT_STORAGE_KEY, projectPath);
		}
	} catch {
		// localStorage unavailable (private mode, storage full) — the choice just stops persisting.
	}
}

/**
 * The workbench's picked project at mount: the URL's own `project=` param when the URL carries
 * one at all (including an explicit `global`, which must override a REMEMBERED project rather
 * than being silently replaced by it — a shared link always wins), otherwise the remembered
 * choice from localStorage, otherwise Global.
 */
export function resolveInitialProject(urlParam: string | null): string | null {
	if (urlParam !== null && urlParam.length > 0) return parseProjectParam(urlParam);
	return loadRememberedProject();
}
