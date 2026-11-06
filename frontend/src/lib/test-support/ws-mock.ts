import { vi } from 'vitest';

export function makeWsMock() {
	return {
		state: 'connected' as 'connected' | 'connecting' | 'reconnecting' | 'disconnected',
		connect: vi.fn(),
		disconnect: vi.fn(),
		events: [] as unknown[],
		laggedCount: 0,
		clear: vi.fn()
	};
}

export type WsMock = ReturnType<typeof makeWsMock>;
