import { describe, it, expect } from 'vitest';
import { buildMessageTree } from './messageTree';
import type { ThreadMessageDto } from './types';

function msg(overrides: Partial<ThreadMessageDto> & { message_uuid: string }): ThreadMessageDto {
	return {
		from_session_uuid: null,
		to_session_uuid: 'sess-page',
		body: `body for ${overrides.message_uuid}`,
		queued_at: '2026-08-10T09:00:00Z',
		claimed_at: null,
		sent_request_id: null,
		sent_at: null,
		consumed_at: null,
		dead_lettered_at: null,
		delivery_attempts: 0,
		status: 'delivered',
		thread_uuid: 'thread-1',
		in_reply_to_uuid: null,
		context_items: [],
		...overrides
	};
}

describe('buildMessageTree', () => {
	it('returns an empty array for empty input', () => {
		expect(buildMessageTree([])).toEqual([]);
	});

	it('nests two children under one root, oldest-first', () => {
		const messages: ThreadMessageDto[] = [
			msg({ message_uuid: 'root', queued_at: '2026-08-10T09:00:00Z' }),
			msg({ message_uuid: 'child-a', in_reply_to_uuid: 'root', queued_at: '2026-08-10T09:01:00Z' }),
			msg({ message_uuid: 'child-b', in_reply_to_uuid: 'root', queued_at: '2026-08-10T09:02:00Z' })
		];

		const tree = buildMessageTree(messages);

		expect(tree).toHaveLength(1);
		expect(tree[0]?.message.message_uuid).toBe('root');
		expect(tree[0]?.depth).toBe(0);
		expect(tree[0]?.children.map((c) => c.message.message_uuid)).toEqual(['child-a', 'child-b']);
		expect(tree[0]?.children.every((c) => c.depth === 1)).toBe(true);
	});

	it('renders a message whose in_reply_to_uuid points outside the set as a root, never dropping it', () => {
		const messages: ThreadMessageDto[] = [
			msg({ message_uuid: 'orphan', in_reply_to_uuid: 'not-in-this-set' })
		];

		const tree = buildMessageTree(messages);

		expect(tree).toHaveLength(1);
		expect(tree[0]?.message.message_uuid).toBe('orphan');
		expect(tree[0]?.depth).toBe(0);
	});

	it('produces an unbounded true depth on a 5-deep reply chain, independent of any visual indent cap', () => {
		const messages: ThreadMessageDto[] = [
			msg({ message_uuid: 'm0' }),
			msg({ message_uuid: 'm1', in_reply_to_uuid: 'm0' }),
			msg({ message_uuid: 'm2', in_reply_to_uuid: 'm1' }),
			msg({ message_uuid: 'm3', in_reply_to_uuid: 'm2' }),
			msg({ message_uuid: 'm4', in_reply_to_uuid: 'm3' }),
			msg({ message_uuid: 'm5', in_reply_to_uuid: 'm4' })
		];

		const tree = buildMessageTree(messages);

		let node = tree[0];
		const depths: number[] = [];
		while (node) {
			depths.push(node.depth);
			node = node.children[0];
		}

		expect(depths).toEqual([0, 1, 2, 3, 4, 5]);
	});
});
