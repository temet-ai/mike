import * as api from '$lib/api/wright';
import type {
	FollowupLimits,
	FollowupState,
	MessagesPage,
	ProblemsPage,
	ProblemState,
	WrightAction,
	WrightCause,
	WrightEpisode,
	WrightFinding,
	WrightFollowup,
	WrightGroup,
	WrightInvestigation,
	WrightMessage,
	WrightProblem,
	WrightRemedy,
	WrightRepair
} from '$lib/api/wright';
import { byPriorityDesc } from './causes';
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
const FOLLOWUP_PAGE = 25;
const CAUSE_LIMIT = 50;
/** Until the first answer says otherwise; the server's `waited_hours` replaces it. */
const DEFAULT_WAITED_HOURS = 6;
const POLL_MS = 15_000;
const POLL_PENDING_MS = 3_000;

export type UndoStatus = { state: 'undoing' | 'failed'; reason: string | null };

/** What the Follow-ups Show control offers: every state, or one. */
export type FollowupFilter = 'all' | FollowupState;

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

const causeKey = (c: WrightCause): number => c.cause_id;

const followupKey = (f: WrightFollowup): number => f.id;
const byIdDesc = (a: WrightFollowup, b: WrightFollowup): number => b.id - a.id;

async function followupsFrom(
	filter: FollowupFilter,
	before: number | null,
	limit: number = FOLLOWUP_PAGE
): Promise<FeedPage<WrightFollowup>> {
	const page = await api.listFollowups({
		state: filter === 'all' ? undefined : filter,
		limit,
		before
	});
	return { rows: page.followups, cursor: page.next_before };
}

/** Newest first from the top until `minRows` are covered, so every row on screen refreshes. */
async function followupsCovering(
	filter: FollowupFilter,
	minRows: number
): Promise<FeedPage<WrightFollowup>> {
	const rows: WrightFollowup[] = [];
	let cursor: number | null = null;
	let first = true;
	while (first || (cursor !== null && rows.length < minRows)) {
		first = false;
		const page: FeedPage<WrightFollowup> = await followupsFrom(filter, cursor);
		rows.push(...page.rows);
		cursor = page.cursor;
	}
	return { rows, cursor };
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

/** What the operator reads when an action on the queue was refused. */
function triageErrorText(err: unknown): string {
	if (api.isUnauthorised(err)) return 'The operator token was refused. Pair this browser again.';
	if (err instanceof api.WrightApiError && err.reason !== null) return err.reason;
	if (api.isNotDeployed(err)) {
		return 'That cause is gone, or this part of Wright is not in the running mike yet.';
	}
	return errorMessage(err);
}

/** The new group a cause list is put in: one that exists, or a name to create first. */
export type GroupTarget = { id: number } | { name: string };

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
	const followups = $state<Feed<WrightFollowup>>(emptyFeed());
	const causes = $state<Feed<WrightCause>>(emptyFeed());
	let waitedHours = $state(DEFAULT_WAITED_HOURS);
	let groups = $state<WrightGroup[]>([]);
	let maxAttempts = $state<number | null>(null);
	/** The open problems of each cause the operator opened, from `GET /causes/{id}/problems`. */
	const causeProblems = $state<Record<number, Feed<WrightProblem>>>({});
	let triageBusy = $state(false);
	let triageError = $state<string | null>(null);
	/** Counts the operator's actions, so a poll that began before one does not undo it. */
	let actionSeq = 0;
	/** Problems fetched for a detail that are not in the open or healthy lists on screen. */
	const fetchedProblems = $state<Record<number, WrightProblem>>({});
	let followupFilter = $state<FollowupFilter>('all');
	let followupLimits = $state<FollowupLimits | null>(null);
	let filterSeq = 0;
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

	/** The limits only sharpen the copy ("attempt 2 of 3"); without them the copy still reads. */
	async function loadLimits(): Promise<void> {
		try {
			followupLimits = (await api.getFollowupLimits()).limits;
		} catch {
			// Keep what is shown: the page says "attempt N" until the limits arrive.
		}
	}

	/** The ranked causes, in the server's order; the page keeps the rows it shows until asked. */
	async function causePage(): Promise<FeedPage<WrightCause>> {
		const res = await api.listCauses({ limit: CAUSE_LIMIT });
		waitedHours = res.waited_hours;
		groups = res.groups;
		maxAttempts = res.max_attempts;
		return { rows: res.causes, cursor: null, total: res.causes.length };
	}

	/** A poll's answer, dropped when an operator action happened while it was on its way. */
	async function passiveCausePage(): Promise<FeedPage<WrightCause>> {
		const seq = actionSeq;
		const res = await api.listCauses({ limit: CAUSE_LIMIT });
		if (seq !== actionSeq) return { rows: causes.rows, cursor: null, total: causes.rows.length };
		waitedHours = res.waited_hours;
		groups = res.groups;
		maxAttempts = res.max_attempts;
		return { rows: res.causes, cursor: null, total: res.causes.length };
	}

	/** After an operator action the queue is read again and shown in the server's order at once. */
	async function reloadCauses(): Promise<void> {
		const seq = actionSeq;
		try {
			const res = await api.listCauses({ limit: CAUSE_LIMIT });
			// A later action reads the queue again itself; this answer is already out of date.
			if (seq !== actionSeq) return;
			waitedHours = res.waited_hours;
			groups = res.groups;
			maxAttempts = res.max_attempts;
			causes.rows = [...res.causes].sort(byPriorityDesc);
			causes.held = [];
			causes.cursor = null;
			causes.total = res.causes.length;
			causes.status = 'ready';
			causes.error = null;
		} catch {
			// The row the action returned is already shown; the next poll reads the rest.
		}
		await Promise.all(
			Object.keys(causeProblems).map((id) => refreshCauseProblems(Number(id), true))
		);
	}

	/** The page asks for the last refusal to go, for example when it opens a dialog of its own. */
	function clearTriageError(): void {
		triageError = null;
	}

	/** Runs one operator action: one at a time, its refusal shown on the page. True when it worked. */
	async function operate(work: () => Promise<void>): Promise<boolean> {
		if (triageBusy) return false;
		triageBusy = true;
		triageError = null;
		actionSeq++;
		try {
			await work();
			return true;
		} catch (err) {
			triageError = triageErrorText(err);
			return false;
		} finally {
			triageBusy = false;
		}
	}

	/** The row the server returned replaces the shown one, and the list is in order again. */
	function applyCause(updated: WrightCause): void {
		const swap = (row: WrightCause): WrightCause =>
			row.cause_id === updated.cause_id ? updated : row;
		causes.rows = causes.rows.map(swap).sort(byPriorityDesc);
		causes.held = causes.held.map(swap);
	}

	async function patchCauses(ids: number[], patch: api.CausePatch): Promise<void> {
		for (const id of ids) applyCause(await api.patchCause(id, patch));
	}

	const pinCause = (id: number, pinned: boolean): Promise<boolean> =>
		operate(async () => {
			await patchCauses([id], { pinned });
			void reloadCauses();
		});

	/** `priority` null clears the operator's value; the formula orders the cause again. */
	const moveCause = (id: number, priority: number | null): Promise<boolean> =>
		operate(async () => {
			await patchCauses([id], { operator_priority: priority });
			void reloadCauses();
		});

	/** Puts the causes in a group, or takes them out of it (`group` null). */
	const groupCauses = (ids: number[], group: GroupTarget | null): Promise<boolean> =>
		operate(async () => {
			let groupId: number | null = null;
			if (group !== null) {
				groupId = 'id' in group ? group.id : (await api.createCauseGroup(group.name)).id;
			}
			await patchCauses(ids, { group_id: groupId });
			await reloadCauses();
		});

	const renameGroup = (id: number, name: string): Promise<boolean> =>
		operate(async () => {
			await api.renameCauseGroup(id, name);
			await reloadCauses();
		});

	const deleteGroup = (id: number): Promise<boolean> =>
		operate(async () => {
			await api.deleteCauseGroup(id);
			await reloadCauses();
		});

	/** Absorbs `ids` into cause `rootId`. */
	const mergeCauses = (rootId: number, ids: number[]): Promise<boolean> =>
		operate(async () => {
			await api.mergeCauses(rootId, ids);
			await reloadCauses();
		});

	/** Undoes one merge the operator made; the server refuses an engineer's or a repeated one. */
	const separateMerge = (mergeId: number): Promise<boolean> =>
		operate(async () => {
			await api.separateMerge(mergeId);
			await reloadCauses();
		});

	function causeProblemFeed(id: number): Feed<WrightProblem> {
		const existing = causeProblems[id];
		if (existing !== undefined) return existing;
		causeProblems[id] = emptyFeed();
		return causeProblems[id];
	}

	const causeProblemsPage = async (id: number): Promise<FeedPage<WrightProblem>> => {
		const rows = await api.listCauseProblems(id);
		return { rows, cursor: null, total: rows.length };
	};

	/** The cause's open problems, read when its row is opened. */
	async function loadCauseProblems(id: number): Promise<void> {
		await fillFeed(causeProblemFeed(id), () => causeProblemsPage(id));
	}

	/** `replace`: after a merge the membership changed on purpose, so the rows are replaced. */
	async function refreshCauseProblems(id: number, replace = false): Promise<void> {
		const feed = causeProblems[id];
		if (feed === undefined) return;
		if (replace) {
			try {
				const page = await causeProblemsPage(id);
				feed.rows = page.rows;
				feed.held = [];
				feed.total = page.total ?? null;
				feed.status = 'ready';
			} catch {
				// Keep what is shown.
			}
			return;
		}
		await refreshFeed(feed, () => causeProblemsPage(id), problemKey);
	}

	function showHeldCauseProblems(id: number): void {
		const feed = causeProblems[id];
		if (feed === undefined || feed.held.length === 0) return;
		feed.rows = [...feed.rows, ...feed.held];
		feed.held = [];
	}

	async function loadAll(): Promise<void> {
		await Promise.all([
			fillFeed(causes, causePage),
			fillFeed(followups, () => followupsCovering(followupFilter, 0)),
			loadLimits(),
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
		const filter = followupFilter;
		await Promise.all([
			refreshFeed(causes, passiveCausePage, causeKey),
			...Object.keys(causeProblems).map((id) => refreshCauseProblems(Number(id))),
			refreshFeed(
				followups,
				async () =>
					// A filter changed mid-flight: this answer is for the old one, so it adds nothing.
					filter === followupFilter
						? followupsCovering(filter, followups.rows.length)
						: { rows: [], cursor: null },
				followupKey
			),
			loadLimits(),
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
				fetchedProblems[id] = res.problem;
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

	/** The operator's choice: the list is read again for that state and replaces what was shown. */
	async function setFollowupFilter(filter: FollowupFilter): Promise<void> {
		followupFilter = filter;
		const mine = ++filterSeq;
		const next = emptyFeed<WrightFollowup>();
		await fillFeed(next, () => followupsCovering(filter, 0));
		if (mine !== filterSeq) return;
		followups.rows = next.rows;
		followups.held = [];
		followups.cursor = next.cursor;
		followups.error = next.error;
		followups.status = next.status;
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
		get followups() {
			return followups;
		},
		get causes() {
			return causes;
		},
		get waitedHours() {
			return waitedHours;
		},
		get groups() {
			return groups;
		},
		get maxAttempts() {
			return maxAttempts;
		},
		get causeProblems() {
			return causeProblems;
		},
		get triageBusy() {
			return triageBusy;
		},
		get triageError() {
			return triageError;
		},
		get fetchedProblems() {
			return fetchedProblems;
		},
		get followupFilter() {
			return followupFilter;
		},
		get followupLimits() {
			return followupLimits;
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
		loadCauseProblems,
		showHeldCauseProblems,
		clearTriageError,
		pinCause,
		moveCause,
		groupCauses,
		renameGroup,
		deleteGroup,
		mergeCauses,
		separateMerge,
		undoAction,
		undoSteer,
		send,
		moreOpen: () => moreFeed(open, (offset) => problemsFrom('open', offset), problemKey),
		moreEarlier: () => moreFeed(earlier, (offset) => problemsFrom('healthy', offset), problemKey),
		moreFollowups: () =>
			moreFeed(followups, (before) => followupsFrom(followupFilter, before), followupKey),
		setFollowupFilter,
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
		showHeldCauses: () => showHeld(causes, byPriorityDesc),
		showHeldFollowups: () => showHeld(followups, byIdDesc),
		showHeldActions: () => showHeld(actions, byCreatedDesc),
		startLive
	};
}

export type WrightStore = ReturnType<typeof createWrightStore>;
