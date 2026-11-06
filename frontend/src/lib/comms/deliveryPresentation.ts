import type { ThreadDeliveryState, ThreadMessageDto, ThreadSummaryDto } from './types';

export type DeliveryTone = 'neutral' | 'info' | 'warn' | 'alert';

export interface DeliveryPresentation {
	tone: DeliveryTone;
	label: string;
	detail: string;
}

const STALE_PENDING_MINUTES = 10;

export function toneBadgeClass(tone: DeliveryTone): string {
	switch (tone) {
		case 'info':
			return 'badge-analyzed';
		case 'warn':
			return 'badge-warning';
		case 'alert':
			return 'badge-error';
		case 'neutral':
			return 'badge-muted';
	}
}

type MessageDeliveryFields = Pick<
	ThreadMessageDto,
	'status' | 'delivery_attempts' | 'sent_at' | 'consumed_at' | 'dead_lettered_at' | 'queued_at'
>;

export function describeDelivery(
	msg: MessageDeliveryFields,
	now: number = Date.now()
): DeliveryPresentation {
	if (msg.status === 'consumed') {
		return {
			tone: 'info',
			label: 'Read',
			detail: 'The recipient acknowledged this message.'
		};
	}

	if (msg.status === 'dead_lettered') {
		const attempts = msg.delivery_attempts;
		return {
			tone: 'warn',
			label: 'Delivery stopped',
			detail: `Automatic delivery gave up after ${attempts} attempt${attempts === 1 ? '' : 's'} — the recipient may still have read it directly from their mailbox.`
		};
	}

	if (msg.status === 'delivered' || msg.status === 'claimed') {
		return {
			tone: 'neutral',
			label: 'Delivered',
			detail: 'Injected into the recipient session — no read receipt yet.'
		};
	}

	if (msg.delivery_attempts > 0) {
		return {
			tone: 'neutral',
			label: 'Retrying',
			detail: `${msg.delivery_attempts} delivery attempt${msg.delivery_attempts === 1 ? '' : 's'} so far — still pending.`
		};
	}

	const queuedMs = now - new Date(msg.queued_at).getTime();
	const staleMs = STALE_PENDING_MINUTES * 60 * 1000;
	if (queuedMs > staleMs) {
		return {
			tone: 'warn',
			label: 'Waiting a while',
			detail: `Queued over ${STALE_PENDING_MINUTES} minutes ago with no delivery attempt yet — the recipient may not be reading their mailbox, but may have seen it another way.`
		};
	}

	return {
		tone: 'neutral',
		label: 'Queued',
		detail:
			'Not pushed yet — the recipient can still pick this up by reading their mailbox directly.'
	};
}

type ThreadDeliveryFields = Pick<ThreadSummaryDto, 'worst_delivery_state' | 'has_stale_pending'>;

export function describeThreadDelivery(thread: ThreadDeliveryFields): DeliveryPresentation {
	const state: ThreadDeliveryState = thread.worst_delivery_state;

	if (state === 'consumed') {
		return {
			tone: 'info',
			label: 'Read',
			detail: 'Every participant acknowledged their copy.'
		};
	}

	if (state === 'dead_lettered') {
		return {
			tone: 'warn',
			label: 'Delivery stopped',
			detail:
				'Automatic delivery gave up on at least one message — recipients may still have read it directly from their mailbox.'
		};
	}

	if (state === 'delivered' || state === 'claimed') {
		return {
			tone: 'neutral',
			label: 'Delivered',
			detail: 'Injected into recipient sessions — no read receipt yet.'
		};
	}

	if (thread.has_stale_pending) {
		return {
			tone: 'warn',
			label: 'Waiting a while',
			detail:
				'At least one message has sat unclaimed a long time — a recipient may not be reading their mailbox, but may have seen it another way.'
		};
	}

	return {
		tone: 'neutral',
		label: 'Queued',
		detail: 'Not pushed yet — recipients can still pick this up by reading their mailbox directly.'
	};
}
