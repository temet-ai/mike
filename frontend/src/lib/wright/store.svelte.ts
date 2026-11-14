import * as api from '$lib/api/wright';
import type {
	MessagesPage,
	ProblemsPage,
	ProblemState,
	WrightAction,
	WrightEpisode,
	WrightFinding,
	WrightInvestigation,
	WrightMessage,
	WrightProblem,
	WrightRemedy,
	WrightRepair
} from '$lib/api/wright';
import {
	emptyFeed,
	errorMessage,
	fillFeed,
	moreFeed,
	refreshFeed,
	showHeld,
	type Feed,
	type FeedPage
} from './feed';

const PAGE = 50;
const MESSAGE_PAGE = 100;
const POLL_MS = 15_000;
const POLL_PENDING_MS = 3_000;

export type UndoStatus = { state: 'undoing' | 'failed'; reason: string | null };

export type DetailFeed = 'episodes' | 'actions' | 'findings' | 'repairs' | 'investigations';

export interface ProblemDetailState {
	episodes: Feed<WrightEpisode>;
	actions: Feed<WrightAction>;
	findings: Feed<WrightFinding>;
	repairs: Feed<WrightRepair>;
	investigations: Feed<WrightInvestigation>;
}

const remedyKey = (r: WrightRemedy): string => r.name;

async function remedyPage(): Promise<FeedPage<WrightRemedy>> {
	const res = await api.listRemedies();
	return { rows: res.remedies, cursor: null, total: res.remedies.length };
}

const problemKey = (p: WrightProblem): number => p.id;
const actionKey = (a: WrightAction): number => a.id;
const byOpenedDesc = (a: WrightProblem, b: WrightProblem): number =>
	b.opened_at.localeCompare(a.opened_at) || b.id - a.id;
const byCreatedDesc = (a: WrightAction, b: WrightAction): number =>
	b.created_at.localeCompare(a.created_at) || b.id - a.id;

/** Problems newest first, from offset 0 until `minRows` are covered, so loaded rows all refresh. */
async function problemsCovering(
	state: ProblemState,
	minRows: number,
	onPage?: (page: ProblemsPage) => void
): Promise<FeedPage<WrightProblem>> {
	const rows: WrightProblem[] = [];
	let offset = 0;
	let total = 0;
	let cursor: number | null = 0;
	while (cursor !== null && (rows.length === 0 || rows.length < minRows)) {
		const page = await api.listProblems({ state, limit: PAGE, offset });
		onPage?.(page);
		rows.push(...page.problems);
		total = page.total;
		cursor = page.next_offset;
		offset = cursor ?? offset;
	}
	return { rows, cursor, total };
}

async function problemsFrom(state: ProblemState, offset: number): Promise<FeedPage<WrightProblem>> {
	const page = await api.listProblems({ state, limit: PAGE, offset });
	return { rows: page.problems, cursor: page.next_offset, total: page.total };
}

async function actionsFrom(
	before: number | null,
	problemId?: number
): Promise<FeedPage<WrightAction>> {
	const page = await api.listActions({ limit: PAGE, before, problem_id: problemId });
	return { rows: page.actions, cursor: page.next_before };
}

async function allMessages(after: number | null): Promise<MessagesPage> {
	const out: WrightMessage[] = [];
	let cursor = after;
	for (;;) {
		const page = await api.listMessages({ after: cursor, limit: MESSAGE_PAGE });
		out.push(...page.messages);
		const last = page.messages.at(-1);
		if (last === undefined || page.messages.length < MESSAGE_PAGE) {
			return { messages: out, pending_reply: page.pending_reply };
		}
		cursor = last.id;
	}
}

function newClientMessageId(): string {
	try {
		return crypto.randomUUID();
	} catch {
		return `wright-${Date.now()}-${Math.random().toString(16).slice(2)}`;
	}
}

export function createWrightStore() {
	const open = $state<Feed<WrightProblem>>(emptyFeed());
	const earlier = $state<Feed<WrightProblem>>(emptyFeed());
	const actions = $state<Feed<WrightAction>>(emptyFeed());
	const remedies = $state<Feed<WrightRemedy>>(emptyFeed());
	const messages = $state<Feed<WrightMessage>>(emptyFeed());
	let pendingReply = $state(false);
	let watchEnabled = $state<boolean | null>(null);
	const undo = $state<Record<string, UndoStatus>>({});
	const details = $state<Record<number, ProblemDetailState>>({});
	let sending = $state(false);
	let sendError = $state<string | null>(null);

	function detailFor(id: number): ProblemDetailState {
		const existing = details[id];
		if (existing !== undefined) return existing;
		details[id] = {
			episodes: emptyFeed(),
			actions: emptyFeed(),
			findings: emptyFeed(),
			repairs: emptyFeed(),
			investigations: emptyFeed()
		};
		return details[id];
	}

	function noteWatch(page: ProblemsPage): void {
		watchEnabled = page.watch_enabled;
	}

	async function loadMessages(): Promise<void> {
		await fillFeed(messages, async () => {
			const page = await allMessages(null);
			pendingReply = page.pending_reply;
			const last = page.messages.at(-1);
			return { rows: page.messages, cursor: last?.id ?? null };
		});
	}

	async function loadAll(): Promise<void> {
		await Promise.all([
			fillFeed(open, () => problemsCovering('open', 0, noteWatch)),
			fillFeed(earlier, () => problemsFrom('healthy', 0)),
			fillFeed(actions, () => actionsFrom(null)),
			fillFeed(remedies, remedyPage),
			loadMessages()
		]);
	}

	async function refreshMessages(): Promise<void> {
		if (messages.status === 'loading') return;
		try {
			const last = messages.rows.at(-1)?.id ?? null;
			const page = await allMessages(last);
			pendingReply = page.pending_reply;
			const known = messages.rows.map((m) => m.id);
			const fresh = page.messages.filter((m) => !known.includes(m.id));
			if (fresh.length > 0) messages.rows = [...messages.rows, ...fresh];
			messages.cursor = messages.rows.at(-1)?.id ?? null;
			messages.status = 'ready';
		} catch (err) {
			if (messages.status !== 'ready') messages.error = errorMessage(err);
		}
	}

	async function refresh(): Promise<void> {
		await Promise.all([
			refreshFeed(open, () => problemsCovering('open', open.rows.length, noteWatch), problemKey),
			refreshFeed(earlier, () => problemsCovering('healthy', earlier.rows.length), problemKey),
			refreshFeed(actions, () => actionsFrom(null), actionKey),
			refreshFeed(remedies, remedyPage, remedyKey),
			refreshMessages(),
			...Object.entries(details).map(([id]) => refreshDetail(Number(id)))
		]);
	}

	async function loadDetail(id: number): Promise<void> {
		const d = detailFor(id);
		await Promise.all([
			fillFeed(d.episodes, async () => {
				const res = await api.getProblem(id, { episode_limit: PAGE, episode_offset: 0 });
				return { rows: res.episodes, cursor: res.next_episode_offset, total: res.problem.episodes };
			}),
			fillFeed(d.actions, () => actionsFrom(null, id)),
			fillFeed(d.findings, async () => {
				const page = await api.listFindings({ problem_id: id, limit: PAGE });
				return { rows: page.findings, cursor: page.next_before };
			}),
			fillFeed(d.repairs, async () => {
				const page = await api.listRepairs({ problem_id: id, limit: PAGE });
				return { rows: page.repairs, cursor: page.next_before };
			}),
			fillFeed(d.investigations, async () => {
				const page = await api.listInvestigations({ problem_id: id, limit: PAGE });
				return { rows: page.investigations, cursor: page.next_before };
			})
		]);
	}

	async function refreshDetail(id: number): Promise<void> {
		const d = details[id];
		if (d === undefined) return;
		await Promise.all([
			refreshFeed(d.actions, () => actionsFrom(null, id), actionKey),
			refreshFeed(
				d.findings,
				async () => {
					const page = await api.listFindings({ problem_id: id, limit: PAGE });
					return { rows: page.findings, cursor: page.next_before };
				},
				(f) => f.id
			),
			refreshFeed(
				d.repairs,
				async () => {
					const page = await api.listRepairs({ problem_id: id, limit: PAGE });
					return { rows: page.repairs, cursor: page.next_before };
				},
				(r) => r.id
			),
			refreshFeed(
				d.investigations,
				async () => {
					const page = await api.listInvestigations({ problem_id: id, limit: PAGE });
					return { rows: page.investigations, cursor: page.next_before };
				},
				(i) => i.id
			)
		]);
	}

	function applyAction(updated: WrightAction): void {
		const swap = (feed: Feed<WrightAction>): void => {
			feed.rows = feed.rows.map((a) => (a.id === updated.id ? updated : a));
			feed.held = feed.held.map((a) => (a.id === updated.id ? updated : a));
		};
		swap(actions);
		for (const d of Object.values(details)) swap(d.actions);
	}

	async function undoAction(id: number): Promise<void> {
		const key = `action-${id}`;
		undo[key] = { state: 'undoing', reason: null };
		try {
			const updated = await api.undoAction(id);
			applyAction(updated);
			delete undo[key];
		} catch (err) {
			undo[key] = {
				state: 'failed',
				reason:
					err instanceof api.WrightApiError && err.reason !== null ? err.reason : errorMessage(err)
			};
			if (err instanceof api.WrightApiError && err.isConflict) {
				void refreshFeed(actions, () => actionsFrom(null), actionKey);
			}
		}
	}

	async function undoSteer(message: WrightMessage): Promise<void> {
		const steer = message.steer;
		if (steer === null) return;
		const key = `steer-${steer.id}`;
		undo[key] = { state: 'undoing', reason: null };
		try {
			await api.undoSteer(steer.id, steer.version ?? 0);
			delete undo[key];
			await loadMessages();
			void refreshFeed(remedies, remedyPage, remedyKey);
		} catch (err) {
			undo[key] = {
				state: 'failed',
				reason:
					err instanceof api.WrightApiError && err.reason !== null ? err.reason : errorMessage(err)
			};
		}
	}

	/** Posts the operator's message. `ok` is false when it was not stored; `id` is its row when known. */
	async function send(body: string): Promise<{ ok: boolean; id: number | null }> {
		const text = body.trim();
		if (text.length === 0 || sending) return { ok: false, id: null };
		sending = true;
		sendError = null;
		try {
			const stored = await api.postMessage({ client_msg_id: newClientMessageId(), body: text });
			if (stored !== null && !messages.rows.some((m) => m.id === stored.id)) {
				messages.rows = [...messages.rows, stored];
			}
			pendingReply = true;
			void refreshMessages();
			return { ok: true, id: stored?.id ?? null };
		} catch (err) {
			sendError = api.isUnauthorised(err)
				? 'The operator token was refused. Pair this browser again.'
				: errorMessage(err);
			return { ok: false, id: null };
		} finally {
			sending = false;
		}
	}

	/** Polls while the tab is visible; faster while Wright's reply is pending. Returns its stop. */
	function startLive(): () => void {
		let timer: ReturnType<typeof setTimeout> | null = null;
		let stopped = false;
		const tick = (): void => {
			timer = setTimeout(
				() => {
					if (stopped) return;
					const visible = typeof document === 'undefined' || document.visibilityState === 'visible';
					void (visible ? refresh() : Promise.resolve()).finally(() => {
						if (!stopped) tick();
					});
				},
				pendingReply ? POLL_PENDING_MS : POLL_MS
			);
		};
		tick();
		return () => {
			stopped = true;
			if (timer !== null) clearTimeout(timer);
		};
	}

	return {
		get open() {
			return open;
		},
		get earlier() {
			return earlier;
		},
		get actions() {
			return actions;
		},
		get remedies() {
			return remedies;
		},
		get messages() {
			return messages;
		},
		get pendingReply() {
			return pendingReply;
		},
		get watchEnabled() {
			return watchEnabled;
		},
		get undo() {
			return undo;
		},
		get details() {
			return details;
		},
		get sending() {
			return sending;
		},
		get sendError() {
			return sendError;
		},
		loadAll,
		refresh,
		loadDetail,
		undoAction,
		undoSteer,
		send,
		moreOpen: () => moreFeed(open, (offset) => problemsFrom('open', offset), problemKey),
		moreEarlier: () => moreFeed(earlier, (offset) => problemsFrom('healthy', offset), problemKey),
		moreActions: () => moreFeed(actions, (before) => actionsFrom(before), actionKey),
		moreDetail: (id: number, which: DetailFeed): Promise<void> => {
			const d = detailFor(id);
			switch (which) {
				case 'actions':
					return moreFeed(d.actions, (before) => actionsFrom(before, id), actionKey);
				case 'episodes':
					return moreFeed(
						d.episodes,
						async (offset) => {
							const res = await api.getProblem(id, { episode_limit: PAGE, episode_offset: offset });
							return { rows: res.episodes, cursor: res.next_episode_offset };
						},
						(e) => e.id
					);
				case 'findings':
					return moreFeed(
						d.findings,
						async (before) => {
							const page = await api.listFindings({ problem_id: id, limit: PAGE, before });
							return { rows: page.findings, cursor: page.next_before };
						},
						(f) => f.id
					);
				case 'repairs':
					return moreFeed(
						d.repairs,
						async (before) => {
							const page = await api.listRepairs({ problem_id: id, limit: PAGE, before });
							return { rows: page.repairs, cursor: page.next_before };
						},
						(r) => r.id
					);
				case 'investigations':
					return moreFeed(
						d.investigations,
						async (before) => {
							const page = await api.listInvestigations({ problem_id: id, limit: PAGE, before });
							return { rows: page.investigations, cursor: page.next_before };
						},
						(i) => i.id
					);
			}
		},
		showHeldOpen: () => showHeld(open, byOpenedDesc),
		showHeldEarlier: () => showHeld(earlier, byOpenedDesc),
		showHeldActions: () => showHeld(actions, byCreatedDesc),
		startLive
	};
}

export type WrightStore = ReturnType<typeof createWrightStore>;
