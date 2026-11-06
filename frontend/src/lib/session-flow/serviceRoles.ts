import type { SessionFlowActivity } from '$lib/api/generated/SessionFlowActivity';
import type { SessionFlowActor } from '$lib/api/generated/SessionFlowActor';

type ChipEvidence = Pick<SessionFlowActivity, 'kind' | 'metadata'>;

export const MIKE_CHIP_KEYS = [
	'r_capture',
	'filters',
	'composer',
	'inject',
	'compress',
	's_capture',
	'intercept',
	'analyzers',
	'extract',
	'persist',
	'judge',
	'verifier',
	'embed',
	'mailbox',
	'upkeep'
] as const;
export type MikeChip = (typeof MIKE_CHIP_KEYS)[number];

export const EVALUATIVE_CHIPS: ReadonlySet<MikeChip> = new Set(['judge', 'verifier', 'embed']);

function chipForManifestStage(activity: ChipEvidence): MikeChip {
	const stage = activity.metadata.stage;
	if (stage === 'filter') return 'filters';
	if (stage === 'inject') return 'inject';
	return 'composer';
}

function chipForLlmCallRole(role: string | null): MikeChip | null {
	if (role === 'judge') return 'judge';
	if (role === 'steerer' || role === 'verifier') return 'verifier';
	return null;
}

export function chipForActivity(activity: ChipEvidence): MikeChip | null {
	switch (activity.kind) {
		case 'manifest_stages':
			return chipForManifestStage(activity);
		case 'lesson_injections':
			return 'inject';
		case 'judgments':
			return 'judge';
		case 'embeddings':
			return 'embed';
		case 'verifier_evaluations':
		case 'interventions':
			return 'verifier';
		case 'messages':
			return 'mailbox';
		case 'consolidation':
			return 'upkeep';
		case 'llm_calls':
			return chipForLlmCallRole(activity.metadata.role);
		default:
			return null;
	}
}

export function chipForActor(actor: SessionFlowActor): MikeChip | null {
	return actor.id.startsWith('mailbox:') ? 'mailbox' : null;
}

export function chipFor(
	actor: SessionFlowActor | undefined,
	activity: ChipEvidence
): MikeChip | null {
	return (actor && chipForActor(actor)) ?? chipForActivity(activity);
}

export function chipForLiveEvent(eventType: string): MikeChip | null {
	return eventType === 'DiagnosticReady' ? 'analyzers' : null;
}
