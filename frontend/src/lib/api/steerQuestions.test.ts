import { describe, expect, it } from 'vitest';
import { wakeLabel } from './steerQuestions';

describe('wakeLabel', () => {
	it('describes each delivery outcome', () => {
		expect(wakeLabel({})).toBeNull();
		expect(wakeLabel({ delivery_channel: 'append' })).toBe('queued for the next request');
		expect(wakeLabel({ delivery_channel: 'wake', delivered_request_id: 431907 })).toBe(
			'carried by request 431907'
		);
		expect(
			wakeLabel({ delivery_channel: 'wake', reverted_at: 'x', revert_reason: 'expired_ttl' })
		).toBe('dropped: expired_ttl');
		expect(
			wakeLabel({
				delivery_channel: 'wake',
				wake_state: 'pending',
				deliver_after: '2026-09-25T14:02:31Z'
			})
		).toBe('pending until 14:02:31Z');
		expect(wakeLabel({ delivery_channel: 'wake', wake_state: 'capped_hour' })).toBe('capped hour');
	});
});
