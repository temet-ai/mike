import { UNRESOLVED_PERSONA_SEGMENT } from '$lib/api';
import { formatRelative } from '$lib/time';
import type { PromptAnomalyGroup, PromptClassDto, PromptSubPattern } from '$lib/types';

export type PromptRowState = 'drifted' | 'needs-investigation' | 'unregistered' | 'registered';

export interface PromptCatalogueRowViewModel {
	key: string;
	lane: 'main' | 'subagent' | null;
	persona: string | null;
	displayName: string;
	isHumanizedReason: boolean;
	state: PromptRowState;
	badgeText: string;
	badgeClass: 'badge-error' | 'badge-warning' | 'badge-active';
	observationCount: number | null;
	lastSeenLabel: string | null;
	firingRuleCount: number;
	actionLabel: string;
	href: string;
	sortAnchor: number;
}

const TIER_ORDER: Record<PromptRowState, number> = {
	drifted: 0,
	'needs-investigation': 1,
	unregistered: 2,
	registered: 3
};

export function compareCatalogueRows(
	a: PromptCatalogueRowViewModel,
	b: PromptCatalogueRowViewModel
): number {
	const tierDiff = TIER_ORDER[a.state] - TIER_ORDER[b.state];
	if (tierDiff !== 0) return tierDiff;
	return b.sortAnchor - a.sortAnchor;
}

export function block2Text(blocks: readonly { text: string }[] | null): string | null {
	if (blocks === null) return null;
	const block = blocks[2];
	return block === undefined ? null : block.text;
}

export function hasBlock2Drift(
	registeredBlocks: readonly { text: string }[] | null,
	observedBlocks: readonly { text: string }[] | null
): boolean {
	const registered = block2Text(registeredBlocks);
	const observed = block2Text(observedBlocks);
	if (registered === null || observed === null) return false;
	return registered !== observed;
}

function displayNameForClass(dto: PromptClassDto): string {
	if (dto.lane === 'main') return 'Claude Code (main agent)';
	if (dto.subagent_persona !== null && dto.subagent_persona.length > 0) {
		return dto.subagent_persona;
	}
	return 'Subagent (persona unresolved)';
}

function classHref(dto: PromptClassDto): string {
	return dto.lane === 'main'
		? '/prompts/main'
		: `/prompts/subagent/${encodeURIComponent(dto.subagent_persona ?? UNRESOLVED_PERSONA_SEGMENT)}`;
}

export interface ClassifyOptions {
	isDrifted: boolean;
	firingRuleCount?: number;
	now?: number;
}

export function classifyPromptClass(
	dto: PromptClassDto,
	opts: ClassifyOptions
): PromptCatalogueRowViewModel {
	const persona = dto.subagent_persona;
	const displayName = displayNameForClass(dto);
	const registered = dto.registration.registered;
	const href = classHref(dto);

	let state: PromptRowState;
	let badgeText: string;
	let badgeClass: PromptCatalogueRowViewModel['badgeClass'];
	let actionLabel: string;

	if (registered && opts.isDrifted) {
		state = 'drifted';
		badgeText = 'drifted';
		badgeClass = 'badge-error';
		actionLabel = 'Review drift';
	} else if (registered) {
		state = 'registered';
		badgeText = 'registered';
		badgeClass = 'badge-active';
		actionLabel = 'Open';
	} else {
		state = 'unregistered';
		badgeText = 'unregistered';
		badgeClass = 'badge-warning';
		actionLabel = 'Register this';
	}

	const registerHref =
		dto.lane === 'main'
			? '/prompts/new?lane=main'
			: `/prompts/new?persona=${encodeURIComponent(persona ?? UNRESOLVED_PERSONA_SEGMENT)}`;

	return {
		key: `${dto.lane}:${persona ?? ''}`,
		lane: dto.lane,
		persona,
		displayName,
		isHumanizedReason: false,
		state,
		badgeText,
		badgeClass,
		observationCount: dto.observation_count,
		lastSeenLabel:
			dto.last_seen !== null ? formatRelative(dto.last_seen, opts.now ?? Date.now()) : null,
		firingRuleCount: opts.firingRuleCount ?? 0,
		actionLabel,
		href: registered ? (state === 'drifted' ? `${href}?tab=drift` : href) : registerHref,
		sortAnchor: dto.first_seen !== null ? Date.parse(dto.first_seen) : 0
	};
}

export function humanizeUnknownBlocks(blocks: Record<string, unknown> | null): string {
	if (blocks === null) return 'composer rejected the system field (no diagnostic recorded)';
	const reason = typeof blocks.reason === 'string' ? blocks.reason : null;
	switch (reason) {
		case 'missing':
			return 'system field missing or null on the request';
		case 'string_form':
			return 'system was a JSON string instead of a 3-block array';
		case 'non_3_block': {
			const len = typeof blocks.len === 'number' ? blocks.len : '?';
			return `system was a ${len}-element array (expected exactly 3 blocks)`;
		}
		case 'non_text_block': {
			const idx = typeof blocks.index === 'number' ? blocks.index : '?';
			return `system[${idx}] is not a {type: "text"} block`;
		}
		case 'block_2_text_missing':
			return 'system[2] is missing its `text` field (or it is not a string)';
		default:
			try {
				return `composer rejected system: ${JSON.stringify(blocks)}`;
			} catch {
				return 'composer rejected system (unstringifiable diagnostic payload)';
			}
	}
}

function anomalyRowNav(
	recommendedAction: PromptSubPattern['recommended_action'],
	requestId: number | null,
	sessionId: string | null
): { href: string; actionLabel: string } {
	if (requestId === null || sessionId === null) {
		return { href: '/prompts', actionLabel: 'Investigate' };
	}
	const requestHref = `/sessions/${encodeURIComponent(sessionId)}/requests/${requestId}`;
	switch (recommendedAction) {
		case 'view-filter-diff':
			return {
				href: `${requestHref}?drawer=request&bodyTab=transformations`,
				actionLabel: 'View filter diff'
			};
		case 'register-class':
		case 'register-variant':
			return { href: `/prompts/new?from=request:${requestId}`, actionLabel: 'Register this' };
		case 'investigate-caller':
		case 'open-sample-request':
		default:
			return { href: requestHref, actionLabel: 'Investigate' };
	}
}

export function classifyAnomalyRows(groups: PromptAnomalyGroup[]): PromptCatalogueRowViewModel[] {
	return groups
		.filter((g) => g.classification === 'structural-mismatch')
		.map((group) => {
			const newest = group.observations[0] ?? null;
			const requestId = newest?.request_id ?? null;
			const sessionId = newest?.session_id ?? null;
			const { href, actionLabel } = anomalyRowNav(
				group.sub_pattern.recommended_action,
				requestId,
				sessionId
			);
			const description = group.sub_pattern.description.trim();
			const displayName =
				group.block_text !== null && group.block_text.length > 0
					? group.block_text
					: description.length > 0
						? description
						: humanizeUnknownBlocks(group.unknown_blocks);

			return {
				key: `anomaly:${group.signature_hash}`,
				lane: null,
				persona: group.persona ?? null,
				displayName,
				isHumanizedReason: group.block_text === null || group.block_text.length === 0,
				state: 'needs-investigation',
				badgeText: 'needs investigation',
				badgeClass: 'badge-warning',
				observationCount: group.count,
				lastSeenLabel: formatRelative(group.last_seen),
				firingRuleCount: group.firing_rule_ids.length,
				actionLabel,
				href,
				sortAnchor: Date.parse(group.first_seen)
			} satisfies PromptCatalogueRowViewModel;
		});
}
