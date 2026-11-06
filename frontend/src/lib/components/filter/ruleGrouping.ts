import type { FilterRuleDto } from '$lib/types';
import type { CatalogueClass } from './promptCatalogue.svelte';
import { countRuleMatches } from './workbench';
import { ruleLandingLabel } from './rule-matching';

/**
 * Whether a rule fires on the currently open request: the dry-run's authoritative
 * `firingRuleIds` for anchored rules, or unconditionally true for `inject_reminder`
 * rules (response-triggered, not anchored to any prompt text — matching the existing
 * client-side convention in `WorkbenchRuleStack`'s pre-grouping `matchesHere`). A
 * disabled rule never fires: the server never evaluates it, so it can never appear in
 * `firingRuleIds` even when its anchor text is present.
 */
export function firesOnOpenRequest(
	rule: Pick<FilterRuleDto, 'id' | 'kind' | 'enabled'>,
	firingRuleIds: ReadonlySet<string> | null
): boolean {
	if (rule.kind === 'inject_reminder') return rule.enabled;
	if (!rule.enabled || firingRuleIds === null) return false;
	return firingRuleIds.has(rule.id);
}

export interface LandingGroup {
	heading: string;
	rules: FilterRuleDto[];
}

/**
 * Groups the rules that fire on the open request by WHERE they land (spec: "grouped by
 * where they land, the section heading label") — the same worded landing text
 * `ruleLandingLabel` already derives per-rule, now used as a group heading rather than a
 * per-row caption. Group order follows first appearance among `rules` (already sorted
 * enabled-then-disabled / by `sort_order`).
 */
export function groupFiringRules(
	rules: readonly FilterRuleDto[],
	blockTexts: readonly string[]
): LandingGroup[] {
	const order: string[] = [];
	const byHeading = new Map<string, FilterRuleDto[]>();
	for (const rule of rules) {
		const heading = ruleLandingLabel(rule, blockTexts);
		let bucket = byHeading.get(heading);
		if (bucket === undefined) {
			bucket = [];
			byHeading.set(heading, bucket);
			order.push(heading);
		}
		bucket.push(rule);
	}
	return order.map((heading) => ({ heading, rules: byHeading.get(heading) ?? [] }));
}

export interface OtherPromptGroup {
	key: string;
	model: string;
	classTitle: string;
	latestRequestId: string | null;
	rules: FilterRuleDto[];
}

function ruleMatchesSample(rule: FilterRuleDto, blockTexts: readonly string[]): boolean {
	if (rule.kind === 'inject_reminder') return false;
	return countRuleMatches(rule, blockTexts).count > 0;
}

/**
 * Groups every rule NOT already shown in the firing-here list under the
 * `model · prompt class` where its anchor is present in that class's latest captured
 * sample — "keep it with that model" (operator report). A rule can legitimately appear
 * under more than one model/class when its anchor is present in several. Classes with
 * no sampled blocks yet (`sampleBlocks === null`, still loading/never observed) are
 * skipped rather than treated as a non-match.
 */
export function groupOtherPromptRules(
	candidateRules: readonly FilterRuleDto[],
	classes: readonly CatalogueClass[]
): OtherPromptGroup[] {
	const groups: OtherPromptGroup[] = [];
	for (const cls of classes) {
		for (const modelRow of cls.models) {
			if (modelRow.model === null || modelRow.sampleBlocks === null) continue;
			const blockTexts = modelRow.sampleBlocks.map((b) => b.text);
			const matched = candidateRules.filter((r) => ruleMatchesSample(r, blockTexts));
			if (matched.length === 0) continue;
			groups.push({
				key: `${modelRow.model}:${cls.key}`,
				model: modelRow.model,
				classTitle: cls.title,
				latestRequestId: modelRow.latestObservation?.requestId ?? null,
				rules: matched
			});
		}
	}
	return groups;
}

/**
 * The remainder: rules that neither fire on the open request nor match any captured
 * prompt sample in the catalogue window — dead weight the operator can now find and
 * clean up (spec: "goes in 'Matches no recent prompt' with its last-fired time").
 */
export function rulesMatchingNoPrompt(
	candidateRules: readonly FilterRuleDto[],
	otherGroups: readonly OtherPromptGroup[]
): FilterRuleDto[] {
	const covered = new Set<string>();
	for (const group of otherGroups) {
		for (const rule of group.rules) covered.add(rule.id);
	}
	return candidateRules.filter((r) => !covered.has(r.id));
}

/**
 * "Active" rules for the project picker's currently-open context (spec: "global edits + the
 * picked project's") — global (`project_path` unset) plus whatever's scoped to the WORKBENCH's
 * picked project. Everything else is scoped to some OTHER project and can never fire here, so
 * it's excluded from the firing/other-prompt/no-match pipeline entirely and handled by
 * `groupRulesByOtherProject` instead.
 */
export function activeScopeRules(
	rules: readonly FilterRuleDto[],
	pickedProject: string | null
): FilterRuleDto[] {
	return rules.filter((r) => {
		const scope = r.project_path ?? null;
		return scope === null || scope === pickedProject;
	});
}

export interface OtherProjectGroup {
	projectPath: string;
	projectName: string;
	rules: FilterRuleDto[];
}

/**
 * Rules scoped to a project OTHER than the one currently picked (never Global, never the picked
 * project) — grouped under their own project, landing in "Edits for other prompts" (spec)
 * alongside the existing model·class groups rather than a separate section.
 */
export function groupRulesByOtherProject(
	rules: readonly FilterRuleDto[],
	pickedProject: string | null,
	projects: readonly { project_path: string; custom_name: string | null }[]
): OtherProjectGroup[] {
	const order: string[] = [];
	const byProject = new Map<string, FilterRuleDto[]>();
	for (const rule of rules) {
		const scope = rule.project_path ?? null;
		if (scope === null || scope === pickedProject) continue;
		let bucket = byProject.get(scope);
		if (bucket === undefined) {
			bucket = [];
			byProject.set(scope, bucket);
			order.push(scope);
		}
		bucket.push(rule);
	}
	return order.map((projectPath) => {
		const known = projects.find((p) => p.project_path === projectPath);
		const name = known?.custom_name?.trim();
		return {
			projectPath,
			projectName: name && name.length > 0 ? name : projectPath,
			rules: byProject.get(projectPath) ?? []
		};
	});
}
