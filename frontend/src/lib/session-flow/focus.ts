import { chipFor, type MikeChip } from './serviceRoles';
import { rolesOf } from './roles';
import type { FlowDataset } from './types';

export const REQUEST_PATH_CHIPS: readonly MikeChip[] = [
	'r_capture',
	'filters',
	'composer',
	'inject',
	'compress',
	's_capture'
];
export const RESPONSE_PATH_CHIPS: readonly MikeChip[] = ['intercept', 'extract', 'persist'];

export interface FlowFocusResult {
	keys: ReadonlySet<string>;
	chipKeys: ReadonlySet<MikeChip>;
}

export function connectedSet(dataset: FlowDataset, selectionActorId: string): FlowFocusResult {
	const roles = rolesOf(dataset.actors.values(), dataset.sessionId);
	const keys = new Set<string>([selectionActorId, 'upstream']);
	const chipKeys = new Set<MikeChip>();

	let coordinatorId: string | null = null;
	for (const actor of dataset.actors.values())
		if (roles.get(actor.id) === 'coordinator') coordinatorId = actor.id;
	if (coordinatorId !== null) keys.add(coordinatorId);

	let hasRequests = false;
	const ownActivityIds = new Set<string>();
	for (const activity of dataset.activities.values()) {
		if (activity.actor_id !== selectionActorId) continue;
		ownActivityIds.add(activity.id);
		if (activity.kind === 'requests' && activity.time_semantics === 'capture_interval')
			hasRequests = true;
	}
	if (hasRequests) {
		for (const chip of REQUEST_PATH_CHIPS) chipKeys.add(chip);
		for (const chip of RESPONSE_PATH_CHIPS) chipKeys.add(chip);
	}

	for (const relationship of dataset.relationships.values()) {
		if (relationship.kind !== 'request_tool' && relationship.kind !== 'dispatch_tool') continue;
		if (relationship.from_id === null || !ownActivityIds.has(relationship.from_id)) continue;
		if (relationship.to_id === null) continue;
		const toolActivity = dataset.activities.get(relationship.to_id);
		if (toolActivity) keys.add(toolActivity.actor_id);
	}

	for (const relationship of dataset.relationships.values()) {
		if (relationship.kind !== 'activity_request') continue;
		if (relationship.to_id === null || !ownActivityIds.has(relationship.to_id)) continue;
		if (relationship.from_id === null) continue;
		const sourceActivity = dataset.activities.get(relationship.from_id);
		if (!sourceActivity) continue;
		keys.add(sourceActivity.actor_id);
		const sourceActor = dataset.actors.get(sourceActivity.actor_id);
		const chip = chipFor(sourceActor, sourceActivity);
		if (chip !== null) chipKeys.add(chip);
	}

	if (selectionActorId === coordinatorId) {
		keys.add('terminal');
		for (const actor of dataset.actors.values()) {
			const role = roles.get(actor.id);
			if (role === 'peer' || role === 'agent') keys.add(actor.id);
		}
	}

	return { keys, chipKeys };
}
