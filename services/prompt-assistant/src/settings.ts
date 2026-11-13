import type { InspectClient } from './inspectClient.js';
import type { AssistantProvider, Effort, MayApply, PromptAssistantSettings } from './types.js';

// Mirrors DEFAULT_PROMPT_ASSISTANT_INSTRUCTIONS in crates/mike-base/src/settings/meta_assistants.rs —
// the fallback used when mike's settings catalogue has no prompt_assistant.instructions row yet.
export const BUILTIN_INSTRUCTIONS = `You help the operator edit the Claude Code system prompt that mike captures and rewrites.
That prompt is shared across every project mike has captured traffic for — a global filter edit
reaches all of them, not just the project the operator is looking at right now. You can read: the
captured prompt they are looking at, their filter edits, and the project's CLAUDE.md and rules.
Before recommending a global edit, check whether the operator's other projects say something that
agrees or conflicts, using list_projects and compare_across_projects; when a conflict is
project-specific, propose a project-scoped edit instead of a global one. When asked to change
something, answer with a before/after pair. Match on the shortest text that is unique to the
paragraph, say which models and Claude Code versions the change will reach, and point out where it
contradicts the project's own instructions — or another project's. Never save an edit yourself. The
operator applies it.`;

export const SETTINGS_DEFAULTS: Omit<PromptAssistantSettings, 'enabled'> = {
	provider: 'anthropic',
	model: 'claude-sonnet-5',
	codex_model: 'gpt-6-astra',
	effort: 'medium',
	instructions: BUILTIN_INSTRUCTIONS,
	read_prompt: true,
	read_edits: true,
	read_project_instructions: true,
	read_versions: true,
	read_memory: false,
	read_other_projects: true,
	may_apply: 'suggest',
	capture: true,
	max_turns: 8,
	daily_budget_usd: 1.0,
	keep_days: 30
};

const KEY_PREFIX = 'prompt_assistant.';

function asBool(v: unknown, fallback: boolean): boolean {
	return typeof v === 'boolean' ? v : fallback;
}
function asNumber(v: unknown, fallback: number): number {
	return typeof v === 'number' && Number.isFinite(v) ? v : fallback;
}
function asString(v: unknown, fallback: string): string {
	return typeof v === 'string' && v.length > 0 ? v : fallback;
}
function asEffort(v: unknown, fallback: Effort): Effort {
	return v === 'low' || v === 'medium' || v === 'high' ? v : fallback;
}
function asProvider(v: unknown, fallback: AssistantProvider): AssistantProvider {
	return v === 'anthropic' || v === 'codex' ? v : fallback;
}
function asMayApply(v: unknown, fallback: MayApply): MayApply {
	return v === 'suggest' || v === 'apply' ? v : fallback;
}

/**
 * Fetches mike's settings catalogue and the `prompt_assistant` feature toggle,
 * then merges them over `SETTINGS_DEFAULTS`. Missing keys fall back to the
 * defaults above rather than erroring — the settings-registry half of this
 * change (`crates/mike-base/src/settings/meta_assistants.rs`) may not have shipped
 * yet, or an operator's install may predate it. Called fresh on every health
 * check and every message, per the spec ("no restart to pick up a change").
 */
export async function resolvePromptAssistantSettings(
	client: InspectClient
): Promise<PromptAssistantSettings> {
	const [settingsList, features] = await Promise.all([
		client.getSettings().catch(() => []),
		client.getFeatures().catch(() => [])
	]);
	const byKey = new Map(settingsList.map((s) => [s.key, s.value]));
	const get = (suffix: string): unknown => byKey.get(`${KEY_PREFIX}${suffix}`);
	const provider = byKey.get('assistant.provider');

	const feature = features.find((f) => f.key === 'prompt_assistant');
	const enabled = feature ? feature.enabled : true;

	// Operator/ops escape hatch — not in the spec's settings table, added so a
	// cheap model can be forced for local smoke-testing without needing the
	// prompt_assistant.model settings key to already exist in mike's registry.
	const modelOverride = process.env.PROMPT_ASSISTANT_MODEL_OVERRIDE;

	return {
		provider: asProvider(provider, SETTINGS_DEFAULTS.provider),
		model: asString(modelOverride, asString(get('model'), SETTINGS_DEFAULTS.model)),
		codex_model: asString(get('codex_model'), SETTINGS_DEFAULTS.codex_model),
		effort: asEffort(get('effort'), SETTINGS_DEFAULTS.effort),
		instructions: asString(get('instructions'), SETTINGS_DEFAULTS.instructions),
		read_prompt: asBool(get('read_prompt'), SETTINGS_DEFAULTS.read_prompt),
		read_edits: asBool(get('read_edits'), SETTINGS_DEFAULTS.read_edits),
		read_project_instructions: asBool(
			get('read_project_instructions'),
			SETTINGS_DEFAULTS.read_project_instructions
		),
		read_versions: asBool(get('read_versions'), SETTINGS_DEFAULTS.read_versions),
		read_memory: asBool(get('read_memory'), SETTINGS_DEFAULTS.read_memory),
		read_other_projects: asBool(get('read_other_projects'), SETTINGS_DEFAULTS.read_other_projects),
		may_apply: asMayApply(get('may_apply'), SETTINGS_DEFAULTS.may_apply),
		capture: asBool(get('capture'), SETTINGS_DEFAULTS.capture),
		max_turns: asNumber(get('max_turns'), SETTINGS_DEFAULTS.max_turns),
		daily_budget_usd: asNumber(get('daily_budget_usd'), SETTINGS_DEFAULTS.daily_budget_usd),
		keep_days: asNumber(get('keep_days'), SETTINGS_DEFAULTS.keep_days),
		enabled
	};
}
