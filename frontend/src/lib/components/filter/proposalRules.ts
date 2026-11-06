import type { FilterRuleCreate, FilterRuleDto, FilterRulePatch } from '$lib/types';
import type { AssistantProposal } from '$lib/api/assistant';

/**
 * The raw text to SAVE for a proposal — never `proposal.after`, which is display-only and, for
 * `append`, is the service's own `match + text` concatenation (sending that straight back as a
 * rule's `text` duplicates the anchor: `# Session-specific guidance# Session-specific guidance
 * PROPOSAL_TAIL`, seen live on request 421514).
 *
 * Prefers `proposal.rule_text` once the service sends it. Falls back gracefully for an older
 * service that hasn't shipped the field yet: `drop` has nothing to save (`null`); `append` strips
 * a leading `match` off `after` if present (recovering the pre-`rule_text` shape); anything else
 * (`replace`, or an append whose `after` doesn't start with `match`) uses `after` as-is.
 */
export function resolveProposalRuleText(
	proposal: Pick<AssistantProposal, 'action' | 'match' | 'after' | 'rule_text'>
): string | null {
	if (proposal.action === 'drop') return null;
	if (proposal.rule_text !== undefined && proposal.rule_text !== null) return proposal.rule_text;
	if (proposal.action === 'append' && proposal.match.length > 0 && proposal.after.startsWith(proposal.match)) {
		return proposal.after.slice(proposal.match.length);
	}
	return proposal.after;
}

/**
 * The enabled-rules set PLUS a synthetic rule standing in for a proposal being previewed — fed
 * to the dry-run endpoint so the prompt pane can show what the proposal would change without
 * saving it. Replaces the matching real rule in place when the proposal rewords an existing
 * edit (`edit_id` set and already enabled), so the preview reflects "this edit as reworded"
 * rather than "this edit twice".
 */
export function buildPreviewRules(
	enabledRules: readonly FilterRuleDto[],
	proposal: AssistantProposal
): FilterRuleDto[] {
	const synthetic: FilterRuleDto = {
		id: proposal.edit_id ?? '__assistant_preview__',
		kind: 'text_substring',
		value: proposal.match,
		action: proposal.action,
		text: resolveProposalRuleText(proposal),
		title: proposal.title,
		enabled: true,
		source: 'ui',
		updated_at: new Date().toISOString(),
		trigger_token: null,
		sort_order: 0,
		fires_24h: 0,
		fires_7d: 0,
		last_fired_at: null,
		project_path: proposal.project_path ?? null
	};
	const hasExisting = enabledRules.some((r) => r.id === synthetic.id);
	return hasExisting
		? enabledRules.map((r) => (r.id === synthetic.id ? synthetic : r))
		: [...enabledRules, synthetic];
}

export type ApplyProposalPlan =
	| { kind: 'create'; body: FilterRuleCreate }
	| { kind: 'update'; id: string; patch: FilterRulePatch }
	| { kind: 'refuse'; reason: string };

export interface PlanApplyProposalOptions {
	/** The operator explicitly opted in to replace a saved title with the model's suggestion. */
	useSuggestedTitle?: boolean;
	/**
	 * The workbench's picked project (null = Global) — takes over as the project_path for a
	 * newly CREATED rule (spec: "New edits and assistant Apply use the picked project's path"),
	 * in place of whatever `proposal.project_path` the model itself suggested. `undefined`
	 * (the default) falls back to `proposal.project_path`, so an existing caller/test that
	 * never passes this option keeps its old behaviour. Never touches an UPDATE's project_path
	 * — rewording an existing rule doesn't silently re-scope it; that's what "Make global" /
	 * "Only <project>" are for.
	 */
	workbenchProjectPath?: string | null;
}

/**
 * Decides what applying a proposal should DO — never performs the write itself, so this stays a
 * pure function the panel and its tests can share.
 *
 * - No `edit_id`, or an `edit_id` that no longer matches a real rule (deleted mid-conversation):
 *   create a new rule.
 * - `edit_id` matching a rule whose `kind` isn't `text_substring` (hand-edited into an `xml_tag`
 *   or `lazy_section` rule, say): refuse rather than silently forcing `value`/`text` onto a shape
 *   they don't fit — the operator edits it by hand instead.
 * - `edit_id` matching a plain rule: update `action`/`value`/`text`, and — unless the rule
 *   already carries an operator-saved title, or `useSuggestedTitle` says to override it —
 *   OMIT `title` from the patch so the operator's own name survives the reword.
 */
export function planApplyProposal(
	proposal: AssistantProposal,
	existingRule: FilterRuleDto | null,
	options: PlanApplyProposalOptions = {}
): ApplyProposalPlan {
	const text = resolveProposalRuleText(proposal);

	if (proposal.edit_id === null || existingRule === null) {
		const body: FilterRuleCreate = {
			kind: 'text_substring',
			value: proposal.match,
			action: proposal.action,
			text,
			title: proposal.title
		};
		const projectPath =
			options.workbenchProjectPath !== undefined ? options.workbenchProjectPath : proposal.project_path;
		if (projectPath) {
			body.project_path = projectPath;
		}
		return { kind: 'create', body };
	}

	if (existingRule.kind !== 'text_substring') {
		const label = existingRule.title ?? proposal.title;
		return {
			kind: 'refuse',
			reason: `"${label}" isn't a plain text edit — open it and edit it by hand instead.`
		};
	}

	const hasSavedTitle = existingRule.title !== null && existingRule.title.trim().length > 0;
	const patch: FilterRulePatch = { action: proposal.action, value: proposal.match, text };
	if (!hasSavedTitle || options.useSuggestedTitle === true) {
		patch.title = proposal.title;
	}
	if (proposal.project_path) {
		patch.project_path = proposal.project_path;
	}
	return { kind: 'update', id: existingRule.id, patch };
}
