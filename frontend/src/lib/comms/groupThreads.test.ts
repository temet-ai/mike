import { describe, it, expect } from 'vitest';
import {
	describeGroupCounts,
	describeMessageCount,
	filterableTags,
	groupThreadsByProject,
	NO_PROJECT_NAME,
	splitFleetSubject
} from './groupThreads';
import type { ThreadFleetDto, ThreadProjectDto, ThreadSummaryDto } from './types';

const P1: ThreadProjectDto = { project_uuid: 'p1', name: 'alpha', path: '/work/alpha' };
const P2: ThreadProjectDto = { project_uuid: 'p2', name: 'beta', path: '/work/beta' };
const FLEET: ThreadFleetDto = { fleet_uuid: 'f1', slug: 'core-game', state: 'running' };
const OWN_SUBJECT = 'Fleet core-game: Build the whole game: all of it, with colons: kept';

function thread(
	id: string,
	lastMessageAt: string,
	overrides: Partial<ThreadSummaryDto> = {}
): ThreadSummaryDto {
	return {
		thread_uuid: id,
		subject: `subject ${id}`,
		created_at: lastMessageAt,
		last_message_at: lastMessageAt,
		resolved_at: null,
		created_by_session_uuid: null,
		participants: [],
		tags: [],
		message_count: 1,
		unread_count: 0,
		worst_delivery_state: 'delivered',
		has_stale_pending: false,
		...overrides
	};
}

describe('groupThreadsByProject', () => {
	it('returns no groups for no threads', () => {
		expect(groupThreadsByProject([])).toEqual([]);
	});

	it('groups threads by project and sums message counts', () => {
		const groups = groupThreadsByProject([
			thread('a', '2026-08-10T10:00:00Z', { project: P1, message_count: 3 }),
			thread('b', '2026-08-10T09:00:00Z', { project: P2, message_count: 1 }),
			thread('c', '2026-08-10T08:00:00Z', { project: P1, message_count: 4 })
		]);
		expect(groups.map((g) => g.name)).toEqual(['alpha', 'beta']);
		expect(groups[0]!.threads.map((t) => t.thread_uuid)).toEqual(['a', 'c']);
		expect(groups[0]!.messageCount).toBe(7);
		expect(groups[1]!.messageCount).toBe(1);
	});

	it('orders groups by project name, so new mail never moves a group', () => {
		const groups = groupThreadsByProject([
			thread('b', '2026-08-11T09:00:00Z', { project: P2 }),
			thread('a', '2026-08-10T10:00:00Z', { project: P1 }),
			thread('c', '2026-08-09T08:00:00Z', { project: P1 })
		]);
		expect(groups.map((g) => g.name)).toEqual(['alpha', 'beta']);
	});

	it('places the null-project group last even when it is newest', () => {
		const groups = groupThreadsByProject([
			thread('a', '2026-08-12T10:00:00Z', { project: null }),
			thread('b', '2026-08-01T09:00:00Z', { project: P1 }),
			thread('c', '2026-08-13T09:00:00Z')
		]);
		expect(groups.map((g) => g.name)).toEqual(['alpha', NO_PROJECT_NAME]);
		expect(groups[1]!.threads.map((t) => t.thread_uuid)).toEqual(['a', 'c']);
		expect(groups[1]!.path).toBeNull();
	});

	it('puts fleet threads first inside a group and keeps the rest in given order', () => {
		const groups = groupThreadsByProject([
			thread('a', '2026-08-10T10:00:00Z', { project: P1 }),
			thread('b', '2026-08-10T09:00:00Z', { project: P1, fleet: FLEET }),
			thread('c', '2026-08-10T08:00:00Z', { project: P1 }),
			thread('d', '2026-08-10T07:00:00Z', { project: P1, fleet: FLEET })
		]);
		expect(groups[0]!.threads.map((t) => t.thread_uuid)).toEqual(['b', 'd', 'a', 'c']);
	});

	it('treats a missing fleet like null', () => {
		const groups = groupThreadsByProject([
			thread('a', '2026-08-10T10:00:00Z', { project: P1, fleet: null }),
			thread('b', '2026-08-10T09:00:00Z', { project: P1 })
		]);
		expect(groups[0]!.threads.map((t) => t.thread_uuid)).toEqual(['a', 'b']);
	});
});

describe('Projects page naming and order', () => {
	const PINNED_A: ThreadProjectDto = {
		project_uuid: 'pa',
		name: '/mnt/data/projects/zeta',
		path: '/mnt/data/projects/zeta',
		pinned: true
	};
	const PINNED_B: ThreadProjectDto = {
		project_uuid: 'pb',
		name: 'mike',
		path: '/mnt/data/projects/temet/temet-proxy',
		pinned: true
	};
	const PLAIN: ThreadProjectDto = {
		project_uuid: 'pc',
		name: '/Users/me/projects/holdfast',
		path: '/Users/me/projects/holdfast'
	};
	const SCRATCH: ThreadProjectDto = {
		project_uuid: 'pd',
		name: '/tmp/x/0d7708f6-0cb9-4851-a953-e0553f75011f/repo',
		path: '/tmp/x/0d7708f6-0cb9-4851-a953-e0553f75011f/repo'
	};
	const PLACEHOLDER: ThreadProjectDto = {
		project_uuid: 'pe',
		name: '(no project)',
		path: '(no project)'
	};

	it('names a project by its custom name, else its folder, like the Projects page', () => {
		const groups = groupThreadsByProject([
			thread('a', '2026-08-10T10:00:00Z', { project: PINNED_B }),
			thread('b', '2026-08-10T10:00:00Z', { project: PLAIN })
		]);
		expect(groups.map((g) => g.name)).toEqual(['mike', 'holdfast']);
	});

	it('orders pinned projects as arranged, then projects, then scratch, then No project', () => {
		const groups = groupThreadsByProject(
			[
				thread('a', '2026-08-10T10:00:00Z', { project: PLACEHOLDER }),
				thread('b', '2026-08-10T10:00:00Z', { project: SCRATCH }),
				thread('c', '2026-08-10T10:00:00Z', { project: PLAIN }),
				thread('d', '2026-08-10T10:00:00Z', { project: PINNED_A }),
				thread('e', '2026-08-10T10:00:00Z', { project: PINNED_B })
			],
			['/mnt/data/projects/temet/temet-proxy', '/mnt/data/projects/zeta']
		);
		expect(groups.map((g) => [g.kind, g.name])).toEqual([
			['pinned', 'mike'],
			['pinned', 'zeta'],
			['project', 'holdfast'],
			['scratch', 'repo'],
			['none', NO_PROJECT_NAME]
		]);
	});

	it('keeps the newest message time of each group', () => {
		const [group] = groupThreadsByProject([
			thread('a', '2026-08-10T10:00:00Z', { project: PLAIN }),
			thread('b', '2026-08-12T10:00:00Z', { project: PLAIN })
		]);
		expect(group!.lastMessageAt).toBe('2026-08-12T10:00:00Z');
	});
});

describe('filterableTags', () => {
	it('offers tags shared by two or more threads, most used first, and keeps the chosen one', () => {
		const tags = [
			{ code: 'once', hue: null, usage_count: 1 },
			{ code: 'blocked', hue: null, usage_count: 48 },
			{ code: 'review', hue: null, usage_count: 3 },
			{ code: 'chosen', hue: null, usage_count: 1 }
		];
		expect(filterableTags(tags, 'chosen').map((t) => t.code)).toEqual([
			'blocked',
			'review',
			'chosen'
		]);
	});
});

describe('count wording', () => {
	it('pluralises threads and messages', () => {
		const [one] = groupThreadsByProject([
			thread('a', '2026-08-10T10:00:00Z', { project: P1, message_count: 1 })
		]);
		expect(describeGroupCounts(one!)).toBe('1 thread · 1 message');
		const [many] = groupThreadsByProject([
			thread('a', '2026-08-10T10:00:00Z', { project: P1, message_count: 2 }),
			thread('b', '2026-08-10T09:00:00Z', { project: P1, message_count: 10 })
		]);
		expect(describeGroupCounts(many!)).toBe('2 threads · 12 messages');
	});

	it('describes a row message count', () => {
		expect(describeMessageCount(1)).toBe('1 message');
		expect(describeMessageCount(12)).toBe('12 messages');
		expect(describeMessageCount(0)).toBe('0 messages');
	});
});

describe('splitFleetSubject', () => {
	it('splits the fleet own thread losslessly', () => {
		const own = thread('f1', '2026-08-10T10:00:00Z', { subject: OWN_SUBJECT, fleet: FLEET });
		const split = splitFleetSubject(own);
		expect(split).toEqual({
			lead: 'Fleet core-game',
			rest: 'Build the whole game: all of it, with colons: kept'
		});
		expect(`${split!.lead}: ${split!.rest}`).toBe(OWN_SUBJECT);
	});

	it('is null for a seat thread in the same fleet', () => {
		const seat = thread('seat-1', '2026-08-10T10:00:00Z', { subject: OWN_SUBJECT, fleet: FLEET });
		expect(splitFleetSubject(seat)).toBeNull();
	});

	it('is null for a thread with no fleet', () => {
		expect(
			splitFleetSubject(thread('f1', '2026-08-10T10:00:00Z', { subject: OWN_SUBJECT }))
		).toBeNull();
	});

	it('is null when the subject does not start with the lead', () => {
		const odd = thread('f1', '2026-08-10T10:00:00Z', { subject: 'Renamed goal', fleet: FLEET });
		expect(splitFleetSubject(odd)).toBeNull();
	});
});

describe('large counts', () => {
	it('groups thousands in a header', () => {
		const [group] = groupThreadsByProject([
			thread('a', '2026-08-10T10:00:00Z', { project: P1, message_count: 3106 })
		]);
		expect(describeGroupCounts(group!)).toBe('1 thread · 3,106 messages');
	});
});
