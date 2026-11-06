import { isUnknownProjectPath } from '$lib/fleet/projectTree';
import { formatCount } from '$lib/time';
import type { FleetProjectWithCounts } from '$lib/fleet/types';
import type { ThreadSummaryDto, ThreadTagDto } from './types';

export const NO_PROJECT_KEY = 'no-project';
export const NO_PROJECT_NAME = 'No project';

export type ThreadGroupKind = 'pinned' | 'project' | 'scratch' | 'none';

export interface ThreadGroup {
	key: string;
	kind: ThreadGroupKind;
	name: string;
	path: string | null;
	threads: ThreadSummaryDto[];
	messageCount: number;
	lastMessageAt: string;
}

const KIND_RANK: Record<ThreadGroupKind, number> = { pinned: 0, project: 1, scratch: 2, none: 3 };

/** The Projects page's name for a project: its custom name, else the last folder of its path. */
export function projectDisplayName(name: string, path: string): string {
	const custom = name.trim();
	if (custom !== '' && custom !== path) return custom;
	return path.split('/').filter(Boolean).at(-1) ?? path;
}

function kindOf(thread: ThreadSummaryDto): ThreadGroupKind {
	const project = thread.project ?? null;
	if (project === null || project.path === '(no project)') return 'none';
	if (project.pinned) return 'pinned';
	if (isUnknownProjectPath(project.path)) return 'scratch';
	return 'project';
}

function newGroup(thread: ThreadSummaryDto, kind: ThreadGroupKind): ThreadGroup {
	const project = thread.project ?? null;
	const none = kind === 'none' || project === null;
	return {
		key: none ? NO_PROJECT_KEY : project.project_uuid,
		kind,
		name: none ? NO_PROJECT_NAME : projectDisplayName(project.name, project.path),
		path: none ? null : project.path,
		threads: [],
		messageCount: 0,
		lastMessageAt: thread.last_message_at
	};
}

/** Fleet threads first, then the rest, each keeping the order it came in. */
export function fleetThreadsFirst(threads: ThreadSummaryDto[]): ThreadSummaryDto[] {
	const fleet = threads.filter((t) => (t.fleet ?? null) !== null);
	const rest = threads.filter((t) => (t.fleet ?? null) === null);
	return [...fleet, ...rest];
}

/**
 * Threads grouped by project in the Projects page's order: pinned projects first in the
 * order the operator arranged them there, then the rest by name, then scratch folders,
 * then "No project". Fleet threads lead inside a group.
 */
export function groupThreadsByProject(
	threads: ThreadSummaryDto[],
	pinnedOrder: readonly string[] = []
): ThreadGroup[] {
	const byKey = new Map<string, ThreadGroup>();

	for (const thread of threads) {
		const kind = kindOf(thread);
		const key = kind === 'none' ? NO_PROJECT_KEY : thread.project!.project_uuid;
		let group = byKey.get(key);
		if (group === undefined) {
			group = newGroup(thread, kind);
			byKey.set(key, group);
		}
		group.threads.push(thread);
		group.messageCount += thread.message_count;
		if (thread.last_message_at > group.lastMessageAt) group.lastMessageAt = thread.last_message_at;
	}

	const groups = [...byKey.values()];
	for (const group of groups) {
		group.threads = fleetThreadsFirst(group.threads);
	}

	const pinRank = (g: ThreadGroup): number => {
		const at = g.path === null ? -1 : pinnedOrder.indexOf(g.path);
		return at === -1 ? Number.MAX_SAFE_INTEGER : at;
	};

	return groups.sort(
		(a, b) =>
			KIND_RANK[a.kind] - KIND_RANK[b.kind] ||
			(a.kind === 'pinned' ? pinRank(a) - pinRank(b) : 0) ||
			a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }) ||
			a.key.localeCompare(b.key)
	);
}

/** Tags worth filtering by: shared by two or more threads, most used first. */
export function filterableTags(tags: ThreadTagDto[], selected: string | null): ThreadTagDto[] {
	return tags
		.filter((tag) => tag.usage_count > 1 || tag.code === selected)
		.toSorted((a, b) => b.usage_count - a.usage_count || a.code.localeCompare(b.code));
}

export function describeGroupCounts(group: ThreadGroup): string {
	const n = group.threads.length;
	const m = group.messageCount;
	return `${formatCount(n)} ${n === 1 ? 'thread' : 'threads'} · ${formatCount(m)} ${m === 1 ? 'message' : 'messages'}`;
}

export function describeMessageCount(count: number): string {
	return `${count} ${count === 1 ? 'message' : 'messages'}`;
}

export interface FleetSubject {
	lead: string;
	rest: string;
}

/** The fleet's own thread, `Fleet <slug>: <goal>`, split losslessly: `${lead}: ${rest} === subject`. */
export function splitFleetSubject(t: ThreadSummaryDto): FleetSubject | null {
	const fleet = t.fleet ?? null;
	if (fleet === null || t.thread_uuid !== fleet.fleet_uuid) return null;
	const lead = `Fleet ${fleet.slug}`;
	if (!t.subject.startsWith(`${lead}: `)) return null;
	return { lead, rest: t.subject.substring(lead.length + 2) };
}

export const NO_PROJECT_PATH = '(no project)';

export interface CommsProjects {
	projects: FleetProjectWithCounts[];
	threadsByPath: Map<string, ThreadSummaryDto[]>;
}

/**
 * The Projects page's project rows for the threads on screen: each project that has
 * threads, carrying its thread count in `session_count`, its message count in
 * `request_count` and its newest message time in `last_seen`. Pin state and custom
 * name come from `fleetProjects` when it knows the project, else from the thread's
 * own project. Threads with no project gather under `NO_PROJECT_PATH`.
 */
export function buildCommsProjects(
	threads: ThreadSummaryDto[],
	fleetProjects: readonly FleetProjectWithCounts[]
): CommsProjects {
	const fleetByPath = new Map(fleetProjects.map((p) => [p.project_path, p]));
	const projects: FleetProjectWithCounts[] = [];
	const threadsByPath = new Map<string, ThreadSummaryDto[]>();

	for (const group of groupThreadsByProject(threads)) {
		const path = group.path ?? NO_PROJECT_PATH;
		const known = fleetByPath.get(path);
		const own = group.threads[0]?.project ?? null;
		const ownName = own !== null && own.name.trim() !== '' && own.name !== path ? own.name : null;
		projects.push({
			project_path: path,
			project_uuid: known?.project_uuid ?? own?.project_uuid,
			project_id: known?.project_id ?? null,
			first_seen: known?.first_seen ?? group.lastMessageAt,
			pinned: known?.pinned ?? group.kind === 'pinned',
			custom_name: known?.custom_name ?? (group.kind === 'none' ? 'No project' : ownName),
			session_count: group.threads.length,
			request_count: group.messageCount,
			last_seen: group.lastMessageAt
		});
		threadsByPath.set(path, group.threads);
	}

	return { projects, threadsByPath };
}
