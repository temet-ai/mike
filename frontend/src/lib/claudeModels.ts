import type { WorkerModel } from '$lib/components/settings/WorkerCard.svelte';

interface MaintainedClaudeModel {
	id: string;
	description: string;
}

/**
 * Anthropic Claude models known at the time this list was last updated. This is a floor,
 * not a ceiling — `buildClaudeModelOptions` unions it with whatever the capture DB has
 * actually seen (`GET /api/usage/models`), so a model that ships before this list is
 * updated still appears, and a retired model that stopped seeing traffic still appears as
 * long as it is listed here. Never trim an entry just because it fell out of recent usage.
 */
const MAINTAINED_CLAUDE_MODELS: MaintainedClaudeModel[] = [
	{
		id: 'claude-sonnet-5',
		description: 'The default — the work is reading and rewording, not long planning.'
	},
	{
		id: 'claude-opus-5-5',
		description: 'Slower and more expensive; reach for it only for genuinely hard rewrites.'
	},
	{
		id: 'claude-opus-5',
		description: 'An earlier Opus release than 5-5 above; still slower and pricier than Sonnet.'
	},
	{
		id: 'claude-opus-4-8',
		description: 'An older Opus generation, kept here because captured traffic still uses it.'
	},
	{
		id: 'claude-haiku-4-5-20251001',
		description: 'Fast and cheap — short, mechanical rewords.'
	},
	{
		id: 'claude-haiku-4-5',
		description:
			'An earlier Haiku build with the same fast, cheap profile as the dated build above.'
	},
	{
		id: 'claude-fable-5-1',
		description: 'Built for hard judgment calls, not loops — expensive per token.'
	},
	{
		id: 'claude-fable-5',
		description: 'An earlier Fable release, superseded by 5-1 above.'
	}
];

const UNKNOWN_MODEL_DESCRIPTION = 'Seen in captured traffic; not yet in the maintained list.';

/**
 * Unions the maintained catalogue above with every model id actually observed in captured
 * traffic, so the picker never silently hides a model the account can use — whether that
 * model is too new for the maintained list or too old to still show up in a usage window.
 * `observedIds` should come straight from `getModelUsage`'s rows (nulls/empty already
 * filtered by the caller); duplicates and blanks are ignored defensively here too.
 */
export function buildClaudeModelOptions(observedIds: readonly string[]): WorkerModel[] {
	const byId = new Map<string, WorkerModel>();
	for (const m of MAINTAINED_CLAUDE_MODELS) {
		byId.set(m.id, { id: m.id, label: m.id, description: m.description });
	}
	for (const id of observedIds) {
		if (id === '' || byId.has(id)) continue;
		byId.set(id, { id, label: id, description: UNKNOWN_MODEL_DESCRIPTION });
	}
	return [...byId.values()].sort((a, b) => a.id.localeCompare(b.id));
}
