import { describe, expect, it } from 'vitest';
import { isMessageDelivery } from './messagePhases';
import type { FlowActivity } from './types';

function activity(kind: string, phase: string): FlowActivity {
	return { kind, phase } as FlowActivity;
}

describe('isMessageDelivery', () => {
	it('treats a request send and a channel push as delivery', () => {
		expect(isMessageDelivery(activity('messages', 'sent'))).toBe(true);
		expect(isMessageDelivery(activity('messages', 'pushed'))).toBe(true);
	});

	it('rejects other message phases and other kinds', () => {
		expect(isMessageDelivery(activity('messages', 'queued'))).toBe(false);
		expect(isMessageDelivery(activity('messages', 'consumed'))).toBe(false);
		expect(isMessageDelivery(activity('requests', 'sent'))).toBe(false);
	});
});
