import type { FlowActivity } from './types';

const DELIVERY_PHASES = new Set(['sent', 'pushed']);

export function isMessageDelivery(activity: FlowActivity): boolean {
	return activity.kind === 'messages' && DELIVERY_PHASES.has(activity.phase);
}
