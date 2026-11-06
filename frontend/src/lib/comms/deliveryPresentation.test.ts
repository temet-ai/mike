import { describe, it, expect } from 'vitest';
import { describeDelivery, describeThreadDelivery, toneBadgeClass } from './deliveryPresentation';
import type { ThreadMessageDto, ThreadSummaryDto } from './types';

const NOW = new Date('2026-08-16T12:00:00Z').getTime();

function msg(
	overrides: Partial<ThreadMessageDto>
): Pick<
	ThreadMessageDto,
	'status' | 'delivery_attempts' | 'sent_at' | 'consumed_at' | 'dead_lettered_at' | 'queued_at'
> {
	return {
		status: 'pending',
		delivery_attempts: 0,
		sent_at: null,
		consumed_at: null,
		dead_lettered_at: null,
		queued_at: new Date(NOW).toISOString(),
		...overrides
	};
}

function thread(
	overrides: Partial<ThreadSummaryDto>
): Pick<ThreadSummaryDto, 'worst_delivery_state' | 'has_stale_pending'> {
	return {
		worst_delivery_state: 'pending',
		has_stale_pending: false,
		...overrides
	};
}

describe('describeDelivery', () => {
	it('pending with zero attempts, just queued, reads as neutral "not pushed yet" — not a failure', () => {
		const result = describeDelivery(msg({ status: 'pending', delivery_attempts: 0 }), NOW);
		expect(result.tone).toBe('neutral');
		expect(result.label).toBe('Queued');
		expect(result.detail).not.toMatch(/fail/i);
	});

	it('pending with zero attempts right at the staleness boundary is still neutral', () => {
		const queuedAt = new Date(NOW - 10 * 60 * 1000).toISOString();
		const result = describeDelivery(
			msg({ status: 'pending', delivery_attempts: 0, queued_at: queuedAt }),
			NOW
		);
		expect(result.tone).toBe('neutral');
	});

	it('pending with zero attempts past the staleness window warns but does not alarm', () => {
		const queuedAt = new Date(NOW - 11 * 60 * 1000).toISOString();
		const result = describeDelivery(
			msg({ status: 'pending', delivery_attempts: 0, queued_at: queuedAt }),
			NOW
		);
		expect(result.tone).toBe('warn');
		expect(result.label).toMatch(/waiting/i);
	});

	it('pending with attempts > 0 reads as retrying, neutral', () => {
		const result = describeDelivery(msg({ status: 'pending', delivery_attempts: 2 }), NOW);
		expect(result.tone).toBe('neutral');
		expect(result.label).toBe('Retrying');
		expect(result.detail).toContain('2 delivery attempts');
	});

	it('claimed reads as delivered, neutral', () => {
		const result = describeDelivery(msg({ status: 'claimed', delivery_attempts: 1 }), NOW);
		expect(result.tone).toBe('neutral');
		expect(result.label).toBe('Delivered');
	});

	it('delivered reads as delivered, neutral, and notes no read receipt', () => {
		const result = describeDelivery(msg({ status: 'delivered', delivery_attempts: 1 }), NOW);
		expect(result.tone).toBe('neutral');
		expect(result.detail).toMatch(/no read receipt/i);
	});

	it('dead_lettered warns but explicitly says receipt is still possible', () => {
		const result = describeDelivery(msg({ status: 'dead_lettered', delivery_attempts: 5 }), NOW);
		expect(result.tone).toBe('warn');
		expect(result.label).toBe('Delivery stopped');
		expect(result.detail).toMatch(/may still have read it/i);
		expect(result.detail).toContain('5 attempts');
	});

	it('dead_lettered with exactly 1 attempt uses singular "attempt"', () => {
		const result = describeDelivery(msg({ status: 'dead_lettered', delivery_attempts: 1 }), NOW);
		expect(result.detail).toContain('1 attempt ');
	});

	it('consumed is the only "info"-tone positive receipt signal', () => {
		const result = describeDelivery(msg({ status: 'consumed', delivery_attempts: 1 }), NOW);
		expect(result.tone).toBe('info');
		expect(result.label).toBe('Read');
	});
});

describe('describeThreadDelivery', () => {
	it('pending, not stale, is neutral', () => {
		const result = describeThreadDelivery(
			thread({ worst_delivery_state: 'pending', has_stale_pending: false })
		);
		expect(result.tone).toBe('neutral');
		expect(result.label).toBe('Queued');
	});

	it('pending and stale warns, findable but not alarming', () => {
		const result = describeThreadDelivery(
			thread({ worst_delivery_state: 'pending', has_stale_pending: true })
		);
		expect(result.tone).toBe('warn');
		expect(result.label).toMatch(/waiting/i);
	});

	it('claimed/delivered are neutral', () => {
		expect(describeThreadDelivery(thread({ worst_delivery_state: 'claimed' })).tone).toBe(
			'neutral'
		);
		expect(describeThreadDelivery(thread({ worst_delivery_state: 'delivered' })).tone).toBe(
			'neutral'
		);
	});

	it('dead_lettered warns, not alerts', () => {
		const result = describeThreadDelivery(thread({ worst_delivery_state: 'dead_lettered' }));
		expect(result.tone).toBe('warn');
	});

	it('consumed is info, the positive case', () => {
		const result = describeThreadDelivery(thread({ worst_delivery_state: 'consumed' }));
		expect(result.tone).toBe('info');
	});
});

describe('toneBadgeClass', () => {
	it('maps every tone to an existing app.css badge modifier, no invented classes', () => {
		expect(toneBadgeClass('neutral')).toBe('badge-muted');
		expect(toneBadgeClass('info')).toBe('badge-analyzed');
		expect(toneBadgeClass('warn')).toBe('badge-warning');
		expect(toneBadgeClass('alert')).toBe('badge-error');
	});
});
