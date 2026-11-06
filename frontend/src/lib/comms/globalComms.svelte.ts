import { ApiError } from '$lib/api';
import { getTags, getThread, isCommsUnavailable, listThreads, resolveThread } from './api';
import type { ProxyEvent } from '$lib/types';
import type { ThreadMessageDto, ThreadSummaryDto, ThreadTagDto } from './types';

export const GLOBAL_COMMS_STORE_KEY = Symbol('global-comms-store');

export type GlobalResolvedFilter = 'all' | 'open' | 'resolved';

interface MutableState {
	threads: ThreadSummaryDto[];
	threadsLoading: boolean;
	threadsLoaded: boolean;
	threadsError: string | null;
	threadsUnavailable: boolean;

	tags: ThreadTagDto[];

	selectedThreadUuid: string | null;
	selectedThread: ThreadSummaryDto | null;
	messages: ThreadMessageDto[];
	threadLoading: boolean;
	threadError: string | null;

	filterProject: string | null;
	filterTag: string | null;
	filterParticipant: string | null;
	filterResolved: GlobalResolvedFilter;

	resolvePhase: 'idle' | 'resolving';
	resolveError: string | null;
}

export interface GlobalCommsStore {
	readonly threads: ThreadSummaryDto[];
	readonly threadsLoading: boolean;
	readonly threadsLoaded: boolean;
	readonly threadsError: string | null;
	readonly threadsUnavailable: boolean;
	readonly tags: ThreadTagDto[];
	readonly selectedThreadUuid: string | null;
	readonly selectedThread: ThreadSummaryDto | null;
	readonly messages: ThreadMessageDto[];
	readonly threadLoading: boolean;
	readonly threadError: string | null;
	readonly filterProject: string | null;
	readonly filterTag: string | null;
	readonly filterParticipant: string | null;
	readonly filterResolved: GlobalResolvedFilter;
	readonly resolvePhase: 'idle' | 'resolving';
	readonly resolveError: string | null;

	load(): Promise<void>;
	/**
	 * Says a mounted view is showing this store's threads; the returned function ends that.
	 * Mailbox changes reload the thread list only while at least one view holds it, because
	 * the full list is a large answer and a costly query.
	 */
	retain(): () => void;
	refreshThreads(): Promise<void>;
	selectThread(uuid: string | null): Promise<void>;
	resolveSelected(): Promise<void>;
	setFilterProject(project: string | null): void;
	setFilterTag(tag: string | null): void;
	setFilterParticipant(sessionId: string | null): void;
	setFilterResolved(v: GlobalResolvedFilter): void;
	handleEvent(event: ProxyEvent): void;
	subscribe(listener: (event: ProxyEvent) => void): () => void;
}

function sortByCreatedAtDesc(threads: ThreadSummaryDto[]): ThreadSummaryDto[] {
	return [...threads].sort((a, b) => {
		const diff = new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
		return diff !== 0 ? diff : a.thread_uuid.localeCompare(b.thread_uuid);
	});
}

export function createGlobalCommsStore(options: { project?: string } = {}): GlobalCommsStore {
	const state = $state<MutableState>({
		threads: [],
		threadsLoading: false,
		threadsLoaded: false,
		threadsError: null,
		threadsUnavailable: false,
		tags: [],
		selectedThreadUuid: null,
		selectedThread: null,
		messages: [],
		threadLoading: false,
		threadError: null,
		filterProject: options.project ?? null,
		filterTag: null,
		filterParticipant: null,
		filterResolved: 'all',
		resolvePhase: 'idle',
		resolveError: null
	});

	async function loadThreads(): Promise<void> {
		state.threadsLoading = true;
		state.threadsError = null;
		state.threadsUnavailable = false;
		try {
			const res = await listThreads({
				project: state.filterProject ?? undefined,
				tag: state.filterTag ?? undefined,
				participant: state.filterParticipant ?? undefined,
				resolved: state.filterResolved === 'all' ? undefined : state.filterResolved === 'resolved'
			});
			state.threads = sortByCreatedAtDesc(res.threads);
			state.threadsLoaded = true;
		} catch (err) {
			if (isCommsUnavailable(err)) {
				state.threadsUnavailable = true;
				state.threads = [];
			} else {
				state.threadsError = err instanceof ApiError ? err.message : String(err);
			}
			state.threadsLoaded = true;
		} finally {
			state.threadsLoading = false;
		}
	}

	async function loadTags(): Promise<void> {
		try {
			const res = await getTags();
			state.tags = res.tags;
		} catch {
			state.tags = [];
		}
	}

	async function load(): Promise<void> {
		await Promise.all([loadThreads(), loadTags()]);
	}

	function refreshThreads(): Promise<void> {
		return loadThreads();
	}

	async function selectThread(uuid: string | null): Promise<void> {
		state.selectedThreadUuid = uuid;
		state.selectedThread = uuid
			? (state.threads.find((t) => t.thread_uuid === uuid) ?? null)
			: null;
		state.messages = [];
		state.threadError = null;
		if (!uuid) return;
		state.threadLoading = true;
		try {
			const res = await getThread(uuid);
			if (state.selectedThreadUuid !== uuid) return;
			state.selectedThread = res.thread;
			state.messages = res.messages;
		} catch (err) {
			if (state.selectedThreadUuid !== uuid) return;
			state.threadError = err instanceof ApiError ? err.message : String(err);
		} finally {
			if (state.selectedThreadUuid === uuid) state.threadLoading = false;
		}
	}

	async function resolveSelected(): Promise<void> {
		const threadUuid = state.selectedThreadUuid;
		if (!threadUuid) return;
		state.resolvePhase = 'resolving';
		state.resolveError = null;
		try {
			const res = await resolveThread(threadUuid);
			state.threads = state.threads.map((t) =>
				t.thread_uuid === threadUuid ? { ...t, resolved_at: res.resolved_at } : t
			);
			if (state.selectedThread?.thread_uuid === threadUuid) {
				state.selectedThread = { ...state.selectedThread, resolved_at: res.resolved_at };
			}
		} catch (err) {
			state.resolveError = err instanceof ApiError ? err.message : String(err);
		} finally {
			state.resolvePhase = 'idle';
		}
	}

	async function refreshSelectedThread(): Promise<void> {
		const uuid = state.selectedThreadUuid;
		if (!uuid) return;
		try {
			const res = await getThread(uuid);
			if (state.selectedThreadUuid !== uuid) return;
			state.selectedThread = res.thread;
			state.messages = res.messages;
		} catch {}
	}

	let listeners: ((event: ProxyEvent) => void)[] = [];

	function subscribe(listener: (event: ProxyEvent) => void): () => void {
		listeners = [...listeners, listener];
		return () => {
			listeners = listeners.filter((l) => l !== listener);
		};
	}

	let viewers = 0;

	function retain(): () => void {
		viewers += 1;
		let held = true;
		return () => {
			if (!held) return;
			held = false;
			viewers -= 1;
		};
	}

	function handleEvent(event: ProxyEvent): void {
		for (const listener of listeners) listener(event);
		if (viewers === 0) return;
		if (event.type === 'Lagged') {
			void refreshThreads();
			void refreshSelectedThread();
			return;
		}
		if (event.type !== 'RowChanged') return;
		if (event.table === 'mailbox_messages' || event.table === 'mailbox_threads') {
			void refreshThreads();
			void refreshSelectedThread();
		}
	}

	return {
		get threads() {
			return state.threads;
		},
		get threadsLoading() {
			return state.threadsLoading;
		},
		get threadsLoaded() {
			return state.threadsLoaded;
		},
		get threadsError() {
			return state.threadsError;
		},
		get threadsUnavailable() {
			return state.threadsUnavailable;
		},
		get tags() {
			return state.tags;
		},
		get selectedThreadUuid() {
			return state.selectedThreadUuid;
		},
		get selectedThread() {
			return state.selectedThread;
		},
		get messages() {
			return state.messages;
		},
		get threadLoading() {
			return state.threadLoading;
		},
		get threadError() {
			return state.threadError;
		},
		get filterProject() {
			return state.filterProject;
		},
		get filterTag() {
			return state.filterTag;
		},
		get filterParticipant() {
			return state.filterParticipant;
		},
		get filterResolved() {
			return state.filterResolved;
		},
		get resolvePhase() {
			return state.resolvePhase;
		},
		get resolveError() {
			return state.resolveError;
		},
		load,
		retain,
		refreshThreads,
		selectThread,
		resolveSelected,
		setFilterProject: (project) => {
			state.filterProject = project;
			void loadThreads();
		},
		setFilterTag: (tag) => {
			state.filterTag = tag;
			void loadThreads();
		},
		setFilterParticipant: (sessionId) => {
			state.filterParticipant = sessionId;
			void loadThreads();
		},
		setFilterResolved: (v) => {
			state.filterResolved = v;
			void loadThreads();
		},
		handleEvent,
		subscribe
	};
}
