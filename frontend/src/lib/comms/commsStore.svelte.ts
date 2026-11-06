import { getSessionMessages } from '$lib/api/mailbox';
import { ApiError } from '$lib/api';
import { getFleetGraph } from '$lib/fleet/api';
import type { FleetGraphNode } from '$lib/fleet/types';
import {
	getSessionFleet,
	getSessionThreads,
	getTags,
	getThread,
	isCommsUnavailable,
	postNewThreadMessage,
	postThreadMessage,
	resolveThread
} from './api';
import { ackSessionMessage } from '$lib/api/mailbox';
import type { ProxyEvent } from '$lib/types';
import type {
	ContextItem,
	PeerFleetIdentity,
	PeerSummary,
	SessionFleetDto,
	ThreadMessageDto,
	ThreadSummaryDto,
	ThreadTagDto
} from './types';

export const COMMS_STORE_KEY = Symbol('comms-store');

export type ResolvedFilter = 'all' | 'open' | 'resolved';

interface MutableState {
	sessionId: string | null;
	project: string | null;

	threads: ThreadSummaryDto[];
	threadsLoading: boolean;
	threadsLoaded: boolean;
	threadsError: string | null;
	threadsUnavailable: boolean;

	peers: PeerSummary[];

	fleetDto: SessionFleetDto | null;
	fleetUnavailable: boolean;

	tags: ThreadTagDto[];

	selectedThreadUuid: string | null;
	selectedThread: ThreadSummaryDto | null;
	messages: ThreadMessageDto[];
	threadLoading: boolean;
	threadError: string | null;

	filterTag: string | null;
	filterParticipant: string | null;
	filterResolved: ResolvedFilter;

	replyPhase: 'idle' | 'sending';
	replyError: string | null;
	replyNotice: string | null;
	resolvePhase: 'idle' | 'resolving';
	ackErrors: Record<string, string>;

	newThreadPhase: 'idle' | 'sending';
	newThreadError: string | null;
}

export interface CommsStore {
	readonly sessionId: string | null;
	readonly threads: ThreadSummaryDto[];
	readonly threadsLoading: boolean;
	readonly threadsLoaded: boolean;
	readonly threadsError: string | null;
	readonly threadsUnavailable: boolean;
	readonly filteredThreads: ThreadSummaryDto[];
	readonly peers: PeerSummary[];
	readonly fleetDto: SessionFleetDto | null;
	readonly fleetUnavailable: boolean;
	readonly tags: ThreadTagDto[];
	readonly selectedThreadUuid: string | null;
	readonly selectedThread: ThreadSummaryDto | null;
	readonly messages: ThreadMessageDto[];
	readonly threadLoading: boolean;
	readonly threadError: string | null;
	readonly filterTag: string | null;
	readonly filterParticipant: string | null;
	readonly filterResolved: ResolvedFilter;
	readonly replyPhase: 'idle' | 'sending';
	readonly replyError: string | null;
	readonly replyNotice: string | null;
	readonly resolvePhase: 'idle' | 'resolving';
	readonly ackErrors: Record<string, string>;
	readonly newThreadPhase: 'idle' | 'sending';
	readonly newThreadError: string | null;

	load(sessionId: string, project: string | null): Promise<void>;
	refreshThreads(): Promise<void>;
	selectThread(uuid: string | null): Promise<void>;
	sendReply(
		body: string,
		options?: { inReplyTo?: string | null; context?: ContextItem[] }
	): Promise<void>;
	resolveSelected(): Promise<void>;
	ack(messageUuid: string): Promise<void>;
	startNewThread(to: string, subject: string, body: string, tags: string[]): Promise<void>;
	setFilterTag(tag: string | null): void;
	setFilterParticipant(sessionId: string | null): void;
	setFilterResolved(v: ResolvedFilter): void;
	handleEvent(event: ProxyEvent): void;
}

function sortByCreatedAtDesc(threads: ThreadSummaryDto[]): ThreadSummaryDto[] {
	return [...threads].sort((a, b) => {
		const diff = new Date(b.created_at).getTime() - new Date(a.created_at).getTime();
		return diff !== 0 ? diff : a.thread_uuid.localeCompare(b.thread_uuid);
	});
}

function identityFor(node: FleetGraphNode | undefined, label: string | null): PeerFleetIdentity {
	if (!node) {
		return { role: null, agentType: null, state: 'unknown', unconsumedMail: 0, label };
	}
	return {
		role: node.role,
		agentType: node.agent_type,
		state: node.state,
		unconsumedMail: node.unconsumed_mail ?? 0,
		label
	};
}

function labelsFromThreads(threads: ThreadSummaryDto[]): Map<string, string> {
	const labels = new Map<string, string>();
	for (const t of threads) {
		for (const p of t.participant_identities ?? []) {
			if (!labels.has(p.session_uuid)) labels.set(p.session_uuid, p.label);
		}
	}
	return labels;
}

function derivePeers(
	sessionId: string,
	inbox: { from_session_uuid: string | null; status: string }[],
	outbox: { to_session_uuid: string; status: string }[],
	identityByPeer: Map<string, FleetGraphNode>,
	labelByPeer: Map<string, string>
): PeerSummary[] {
	const counts = new Map<string, { messageCount: number; unconsumedCount: number }>();
	const bump = (peer: string | null, status: string): void => {
		if (!peer || peer === sessionId) return;
		const entry = counts.get(peer) ?? { messageCount: 0, unconsumedCount: 0 };
		entry.messageCount += 1;
		if (status !== 'consumed' && status !== 'dead_lettered') entry.unconsumedCount += 1;
		counts.set(peer, entry);
	};
	for (const m of inbox) bump(m.from_session_uuid, m.status);
	for (const m of outbox) bump(m.to_session_uuid, m.status);

	return Array.from(counts.entries())
		.map(([peerId, c]) => ({
			sessionId: peerId,
			messageCount: c.messageCount,
			unconsumedCount: c.unconsumedCount,
			identity: identityFor(identityByPeer.get(peerId), labelByPeer.get(peerId) ?? null)
		}))
		.sort((a, b) => a.sessionId.localeCompare(b.sessionId));
}

export function createCommsStore(): CommsStore {
	const state = $state<MutableState>({
		sessionId: null,
		project: null,
		threads: [],
		threadsLoading: false,
		threadsLoaded: false,
		threadsError: null,
		threadsUnavailable: false,
		peers: [],
		fleetDto: null,
		fleetUnavailable: false,
		tags: [],
		selectedThreadUuid: null,
		selectedThread: null,
		messages: [],
		threadLoading: false,
		threadError: null,
		filterTag: null,
		filterParticipant: null,
		filterResolved: 'all',
		replyPhase: 'idle',
		replyError: null,
		replyNotice: null,
		ackErrors: {},
		resolvePhase: 'idle',
		newThreadPhase: 'idle',
		newThreadError: null
	});

	async function loadThreads(sessionId: string): Promise<void> {
		state.threadsLoading = true;
		state.threadsError = null;
		state.threadsUnavailable = false;
		try {
			const res = await getSessionThreads(sessionId);
			if (state.sessionId !== sessionId) return;
			state.threads = sortByCreatedAtDesc(res.threads);
			state.threadsLoaded = true;
		} catch (err) {
			if (state.sessionId !== sessionId) return;
			if (isCommsUnavailable(err)) {
				state.threadsUnavailable = true;
				state.threads = [];
			} else {
				state.threadsError = err instanceof ApiError ? err.message : String(err);
			}
			state.threadsLoaded = true;
		} finally {
			if (state.sessionId === sessionId) state.threadsLoading = false;
		}
	}

	async function loadPeers(sessionId: string, project: string | null): Promise<void> {
		let identityByPeer = new Map<string, FleetGraphNode>();
		if (project) {
			try {
				const windowHours = state.fleetDto?.window_hours ?? 24;
				const graph = await getFleetGraph(project, windowHours);
				if (state.sessionId !== sessionId) return;
				identityByPeer = new Map(graph.nodes.map((n) => [n.session_id, n]));
			} catch {
				if (state.sessionId !== sessionId) return;
			}
		}
		try {
			const envelope = await getSessionMessages(sessionId);
			if (state.sessionId !== sessionId) return;
			state.peers = derivePeers(
				sessionId,
				envelope.inbox,
				envelope.outbox,
				identityByPeer,
				labelsFromThreads(state.threads)
			);
		} catch {
			if (state.sessionId !== sessionId) return;
			state.peers = [];
		}
	}

	async function loadFleetDto(sessionId: string): Promise<void> {
		try {
			const dto = await getSessionFleet(sessionId);
			if (state.sessionId !== sessionId) return;
			state.fleetDto = dto;
			state.fleetUnavailable = false;
		} catch (err) {
			if (state.sessionId !== sessionId) return;
			state.fleetDto = null;
			state.fleetUnavailable = isCommsUnavailable(err);
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

	async function load(sessionId: string, project: string | null): Promise<void> {
		if (state.sessionId === sessionId) return;
		state.sessionId = sessionId;
		state.project = project;
		state.threads = [];
		state.threadsLoaded = false;
		state.threadsError = null;
		state.threadsUnavailable = false;
		state.peers = [];
		state.fleetDto = null;
		state.fleetUnavailable = false;
		state.selectedThreadUuid = null;
		state.selectedThread = null;
		state.messages = [];
		state.filterTag = null;
		state.filterParticipant = null;
		state.filterResolved = 'all';
		state.replyNotice = null;
		state.ackErrors = {};

		await Promise.all([loadThreads(sessionId), loadFleetDto(sessionId), loadTags()]);
		await loadPeers(sessionId, project);
	}

	function refreshThreads(): Promise<void> {
		const id = state.sessionId;
		if (!id) return Promise.resolve();
		return loadThreads(id);
	}

	async function selectThread(uuid: string | null): Promise<void> {
		state.selectedThreadUuid = uuid;
		state.selectedThread = uuid
			? (state.threads.find((t) => t.thread_uuid === uuid) ?? null)
			: null;
		state.messages = [];
		state.threadError = null;
		state.replyNotice = null;
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

	async function sendReply(
		body: string,
		options?: { inReplyTo?: string | null; context?: ContextItem[] }
	): Promise<void> {
		const threadUuid = state.selectedThreadUuid;
		const sessionId = state.sessionId;
		if (!threadUuid || !sessionId || body.trim().length === 0) return;
		const inReplyTo = options?.inReplyTo ?? undefined;
		const context = options?.context && options.context.length > 0 ? options.context : undefined;
		state.replyPhase = 'sending';
		state.replyError = null;
		state.replyNotice = null;
		try {
			await postThreadMessage(threadUuid, {
				from_session_id: sessionId,
				body,
				...(inReplyTo ? { in_reply_to: inReplyTo } : {}),
				...(context ? { context } : {})
			});
			await selectThread(threadUuid);
			await refreshThreads();
			state.replyPhase = 'idle';
		} catch (err) {
			if (inReplyTo && err instanceof ApiError && err.status === 400) {
				try {
					await postThreadMessage(threadUuid, {
						from_session_id: sessionId,
						body,
						...(context ? { context } : {})
					});
					await selectThread(threadUuid);
					await refreshThreads();
					state.replyNotice = 'Reply target not found in this thread — sent to the thread instead.';
				} catch (fallbackErr) {
					state.replyError =
						fallbackErr instanceof ApiError ? fallbackErr.message : String(fallbackErr);
				}
				state.replyPhase = 'idle';
				return;
			}
			state.replyError = err instanceof ApiError ? err.message : String(err);
			state.replyPhase = 'idle';
		}
	}

	async function resolveSelected(): Promise<void> {
		const threadUuid = state.selectedThreadUuid;
		if (!threadUuid) return;
		state.resolvePhase = 'resolving';
		try {
			const res = await resolveThread(threadUuid);
			state.threads = state.threads.map((t) =>
				t.thread_uuid === threadUuid ? { ...t, resolved_at: res.resolved_at } : t
			);
			if (state.selectedThread?.thread_uuid === threadUuid) {
				state.selectedThread = { ...state.selectedThread, resolved_at: res.resolved_at };
			}
		} catch (err) {
			state.replyError = err instanceof ApiError ? err.message : String(err);
		} finally {
			state.resolvePhase = 'idle';
		}
	}

	async function ack(messageUuid: string): Promise<void> {
		const sessionId = state.sessionId;
		if (!sessionId) return;
		const nextAckErrors = { ...state.ackErrors };
		delete nextAckErrors[messageUuid];
		state.ackErrors = nextAckErrors;
		try {
			const res = await ackSessionMessage(sessionId, messageUuid);
			state.messages = state.messages.map((m) =>
				m.message_uuid === messageUuid
					? { ...m, consumed_at: res.consumed_at, status: 'consumed' as const }
					: m
			);
		} catch (err) {
			state.ackErrors = {
				...state.ackErrors,
				[messageUuid]: err instanceof ApiError ? err.message : String(err)
			};
		}
	}

	async function startNewThread(
		to: string,
		subject: string,
		body: string,
		tags: string[]
	): Promise<void> {
		const sessionId = state.sessionId;
		const recipient = to.trim();
		if (
			!sessionId ||
			recipient.length === 0 ||
			subject.trim().length === 0 ||
			body.trim().length === 0
		)
			return;
		state.newThreadPhase = 'sending';
		state.newThreadError = null;
		const knownUuids = new Set(state.threads.map((t) => t.thread_uuid));
		try {
			await postNewThreadMessage(recipient, {
				from_session_id: sessionId,
				message: body,
				subject,
				...(tags.length > 0 ? { tags } : {})
			});
			await refreshThreads();
			const opened = state.threads.find((t) => !knownUuids.has(t.thread_uuid));
			state.newThreadPhase = 'idle';
			if (opened) await selectThread(opened.thread_uuid);
		} catch (err) {
			state.newThreadError = err instanceof ApiError ? err.message : String(err);
			state.newThreadPhase = 'idle';
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

	function handleEvent(event: ProxyEvent): void {
		const id = state.sessionId;
		if (!id) return;
		if (event.type === 'Lagged') {
			void refreshThreads();
			void refreshSelectedThread();
			return;
		}
		if (event.type !== 'RowChanged') return;
		if (event.table === 'mailbox_messages' && event.session_id === id) {
			void refreshThreads();
			void refreshSelectedThread();
			return;
		}
		if (event.table === 'mailbox_threads') {
			void refreshThreads();
			void refreshSelectedThread();
		}
	}

	const filteredThreads = (): ThreadSummaryDto[] =>
		state.threads.filter((t) => {
			if (state.filterTag && !t.tags.includes(state.filterTag)) return false;
			if (state.filterParticipant && !t.participants.includes(state.filterParticipant))
				return false;
			if (state.filterResolved === 'open' && t.resolved_at !== null) return false;
			if (state.filterResolved === 'resolved' && t.resolved_at === null) return false;
			return true;
		});

	return {
		get sessionId() {
			return state.sessionId;
		},
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
		get filteredThreads() {
			return filteredThreads();
		},
		get peers() {
			return state.peers;
		},
		get fleetDto() {
			return state.fleetDto;
		},
		get fleetUnavailable() {
			return state.fleetUnavailable;
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
		get filterTag() {
			return state.filterTag;
		},
		get filterParticipant() {
			return state.filterParticipant;
		},
		get filterResolved() {
			return state.filterResolved;
		},
		get replyPhase() {
			return state.replyPhase;
		},
		get replyError() {
			return state.replyError;
		},
		get replyNotice() {
			return state.replyNotice;
		},
		get ackErrors() {
			return state.ackErrors;
		},
		get resolvePhase() {
			return state.resolvePhase;
		},
		get newThreadPhase() {
			return state.newThreadPhase;
		},
		get newThreadError() {
			return state.newThreadError;
		},
		load,
		refreshThreads,
		selectThread,
		sendReply,
		resolveSelected,
		ack,
		startNewThread,
		handleEvent,
		setFilterTag: (tag) => {
			state.filterTag = tag;
		},
		setFilterParticipant: (sessionId) => {
			state.filterParticipant = sessionId;
		},
		setFilterResolved: (v) => {
			state.filterResolved = v;
		}
	};
}
