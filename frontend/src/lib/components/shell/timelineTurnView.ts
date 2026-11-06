import type { RequestSummary, ToolUseDto, OperatorTurnDto } from '$lib/types';
import { isSidecarOrigin, SIDECAR_BAND_LABELS } from '$lib/turn-origins';
import type { MailboxMessage } from '$lib/api/mailbox';
import type { SessionSteeringRow } from '$lib/api/steering';

export type ToolCategory = 'read' | 'edit' | 'exec' | 'dispatch';

const READ_LIKE = new Set([
	'Read',
	'ReadFile',
	'View',
	'Grep',
	'Glob',
	'LS',
	'WebFetch',
	'WebSearch'
]);
const EDIT_LIKE = new Set([
	'Edit',
	'MultiEdit',
	'Write',
	'NotebookEdit',
	'NotebookEditCell',
	'TodoWrite'
]);
const EXEC_LIKE = new Set(['Bash', 'BashOutput', 'KillShell']);
const DISPATCH_LIKE = new Set(['Task', 'Agent', 'Workflow']);

export function categoryOf(toolName: string): ToolCategory {
	if (EXEC_LIKE.has(toolName)) return 'exec';
	if (DISPATCH_LIKE.has(toolName)) return 'dispatch';
	if (EDIT_LIKE.has(toolName)) return 'edit';
	if (READ_LIKE.has(toolName)) return 'read';
	return 'read';
}

export function categoryLabel(cat: ToolCategory): string {
	return cat === 'dispatch' ? 'agent' : cat;
}

export function categoryColorVar(cat: ToolCategory): string {
	switch (cat) {
		case 'read':
			return '--status-info-fg';
		case 'edit':
			return '--role-tool-fg';
		case 'exec':
			return '--status-success-fg';
		case 'dispatch':
			return '--data-teal-fg';
	}
}

export type TurnOutcome = OperatorTurnDto['outcome'];

export function outcomeColorVar(outcome: TurnOutcome, streaming: boolean): string {
	if (streaming) return '--status-warning-fg';
	switch (outcome) {
		case 'accepted':
			return '--status-success-fg';
		case 'corrected':
			return '--status-danger-fg';
		case 'shipped':
			return '--status-info-fg';
		case 'abandoned':
			return '--status-danger-fg';
		default:
			return '--text-3';
	}
}

export function outcomeLabel(outcome: TurnOutcome, streaming: boolean): string {
	if (streaming) return 'streaming';
	if (outcome === 'unknown' || outcome === '') return 'in review';
	return outcome;
}

export function formatTokenCount(n: number): string {
	if (n >= 10000) return `${Math.round(n / 1000)}k`;
	if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
	return String(n);
}

function asStringArray(value: unknown): string[] {
	if (!Array.isArray(value)) return [];
	return value.filter((v): v is string => typeof v === 'string');
}

export function filesTouchedOf(turn: OperatorTurnDto): string[] {
	return asStringArray(turn.files_touched);
}

export function dispatchCountOf(turn: OperatorTurnDto): number {
	const v = turn.agents_dispatched;
	if (typeof v === 'number') return v;
	if (Array.isArray(v)) return v.length;
	return 0;
}

export function turnCollapseKey(turn: Pick<OperatorTurnDto, 'session_id' | 'turn_id'>): string {
	return `timeline-turn:${turn.session_id}:${turn.turn_id}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function strField(input: unknown, key: string): string | null {
	if (!isRecord(input)) return null;
	const v = input[key];
	return typeof v === 'string' ? v : null;
}

export function toolTarget(tool: ToolUseDto): string {
	if (tool.file_path) return tool.file_path;
	const input = tool.input;
	const byName: Record<string, string | null> = {
		Bash: strField(input, 'command'),
		BashOutput: strField(input, 'bash_id'),
		Grep: strField(input, 'pattern'),
		Glob: strField(input, 'pattern'),
		WebFetch: strField(input, 'url'),
		WebSearch: strField(input, 'query'),
		Task: strField(input, 'description') ?? strField(input, 'subagent_type'),
		TodoWrite: 'todo list'
	};
	const named = byName[tool.tool_name];
	if (named) return named;
	if (isRecord(input)) {
		for (const v of Object.values(input)) {
			if (typeof v === 'string' && v.length > 0) return v;
		}
	}
	return '';
}

export function toolInputText(tool: ToolUseDto): string {
	if (tool.input === undefined || tool.input === null) return '';
	if (typeof tool.input === 'string') return tool.input;
	try {
		return JSON.stringify(tool.input, null, 2);
	} catch {
		return String(tool.input);
	}
}

export function toolResultText(tool: ToolUseDto): string | null {
	const result = tool.result;
	if (!result) return null;
	const content = result.content;
	if (content === undefined || content === null) return '';
	if (typeof content === 'string') return content;
	try {
		return JSON.stringify(content, null, 2);
	} catch {
		return String(content);
	}
}

export interface TurnToolStats {
	catCounts: Record<ToolCategory, number>;
	errorCount: number;
}

export function computeTurnToolStats(toolCalls: readonly ToolUseDto[]): TurnToolStats {
	const catCounts: Record<ToolCategory, number> = { read: 0, edit: 0, exec: 0, dispatch: 0 };
	let errorCount = 0;
	for (const t of toolCalls) {
		catCounts[categoryOf(t.tool_name)]++;
		if (t.result?.is_error) errorCount++;
	}
	return { catCounts, errorCount };
}

export function toolCallMatchesQuery(tool: ToolUseDto, query: string): boolean {
	if (query.length === 0) return true;
	const q = query.toLowerCase();
	const hay = `${tool.tool_name} ${toolTarget(tool)} #${tool.request_id}`.toLowerCase();
	return hay.includes(q);
}

export function findTurnForRequestId(
	turnsAsc: readonly OperatorTurnDto[],
	requestId: number
): OperatorTurnDto | null {
	let found: OperatorTurnDto | null = null;
	for (const t of turnsAsc) {
		if (t.start_request_id > requestId) break;
		found = t;
	}
	return found;
}

export interface HousekeepingItemView {
	id: number;
	requestNumber: number | null;
	label: string;
	toolName: string | null;
}

export interface HousekeepingRow {
	kind: 'housekeeping';
	anchorId: number;
	items: HousekeepingItemView[];
}

export interface TurnRow {
	kind: 'turn';
	turn: OperatorTurnDto;
	live: boolean;
	firstRequestId: number;
	firstRequestNumber: number | null;
	lastRequestNumber: number | null;
}

export interface MailboxRow {
	kind: 'mailbox';
	message: MailboxMessage;
	variant: 'delivered' | 'queued';
	turn: OperatorTurnDto | null;
}

export interface SteerRow {
	kind: 'steer';
	intervention: SessionSteeringRow;
	anchorId: number;
}

export type TimelineRow = TurnRow | HousekeepingRow | MailboxRow | SteerRow;

export function assertNeverTimelineRow(row: never): never {
	throw new Error(`Unhandled TimelineRow kind: ${JSON.stringify(row)}`);
}

export function timelineRowKey(row: TimelineRow): string {
	switch (row.kind) {
		case 'turn':
			return `t${row.turn.turn_id}:${row.firstRequestId}`;
		case 'housekeeping':
			return `hk${row.anchorId}`;
		case 'mailbox':
			return `m${row.message.message_uuid}`;
		case 'steer':
			return `s${row.intervention.intervention_id}`;
		default:
			return assertNeverTimelineRow(row);
	}
}

function isLiveStatus(status: string): boolean {
	return status === 'Pending' || status === 'Streaming';
}

function housekeepingLabel(b: RequestSummary): string {
	const origin = b.turn_origin;
	if (origin && isSidecarOrigin(origin)) return SIDECAR_BAND_LABELS[origin];
	const tool = b.tool_names[0];
	if (tool) return tool;
	return `${b.method} ${b.path}`;
}

export function buildTimelineRows(
	bandsAsc: readonly RequestSummary[],
	findTurnForId: (id: number) => OperatorTurnDto | null
): { rows: TimelineRow[]; anchorForRequestId: Map<number, number> } {
	const rows: TimelineRow[] = [];
	const anchorForRequestId = new Map<number, number>();
	let currentHk: HousekeepingItemView[] = [];
	const turnRowIndex = new Map<number, number>();

	const flushHk = (): void => {
		if (currentHk.length === 0) return;
		const anchorId = currentHk[0]!.id;
		for (const it of currentHk) anchorForRequestId.set(it.id, anchorId);
		rows.push({ kind: 'housekeeping', anchorId, items: currentHk });
		currentHk = [];
	};

	for (const b of bandsAsc) {
		const turn = findTurnForId(b.id);
		if (turn === null) {
			currentHk.push({
				id: b.id,
				requestNumber: b.request_number ?? null,
				label: housekeepingLabel(b),
				toolName: b.tool_names[0] ?? null
			});
			continue;
		}
		flushHk();
		anchorForRequestId.set(b.id, turn.start_request_id);
		const live = isLiveStatus(b.status);
		const n = b.request_number ?? null;
		const existingIdx = turnRowIndex.get(turn.turn_id);
		if (existingIdx === undefined) {
			rows.push({
				kind: 'turn',
				turn,
				live,
				firstRequestId: b.id,
				firstRequestNumber: n,
				lastRequestNumber: n
			});
			turnRowIndex.set(turn.turn_id, rows.length - 1);
		} else {
			const row = rows[existingIdx];
			if (row && row.kind === 'turn') {
				const next = { ...row };
				if (live) next.live = true;
				if (n !== null) {
					if (next.firstRequestNumber === null || n < next.firstRequestNumber)
						next.firstRequestNumber = n;
					if (next.lastRequestNumber === null || n > next.lastRequestNumber)
						next.lastRequestNumber = n;
				}
				rows[existingIdx] = next;
			}
		}
	}
	flushHk();
	return { rows, anchorForRequestId };
}

export interface MailboxMergeResult {
	rows: TimelineRow[];
	earlierUnloadedCount: number;
}

function firstRequestIdAtOrAfter(
	bandsAsc: readonly RequestSummary[],
	iso: string
): number | null | 'before-loaded' {
	const at = new Date(iso).getTime();
	const first = bandsAsc[0];
	if (first && new Date(first.created_at).getTime() > at) {
		return 'before-loaded';
	}
	for (const b of bandsAsc) {
		if (new Date(b.created_at).getTime() >= at) return b.id;
	}
	return null;
}

function deliveryRequestId(
	message: MailboxMessage,
	bandsAsc: readonly RequestSummary[]
): number | null | 'before-loaded' | 'not-delivered' {
	if (message.sent_request_id !== null) return message.sent_request_id;
	const pushedAt = message.channel_delivered_at ?? message.consumed_at;
	if (!pushedAt) return 'not-delivered';
	return firstRequestIdAtOrAfter(bandsAsc, pushedAt);
}

export function mergeMailboxRows(
	rows: readonly TimelineRow[],
	inboxMessages: readonly MailboxMessage[],
	findTurnForId: (id: number) => OperatorTurnDto | null,
	bandsAsc: readonly RequestSummary[] = []
): MailboxMergeResult {
	const turnRowIndex = new Map<number, number>();
	rows.forEach((r, i) => {
		if (r.kind === 'turn') turnRowIndex.set(r.turn.turn_id, i);
	});

	const attachByTurnId = new Map<number, MailboxRow[]>();
	const queued: MailboxRow[] = [];
	const deliveredAfterLoaded: MailboxRow[] = [];
	let earlierUnloadedCount = 0;

	for (const message of inboxMessages) {
		const requestId = deliveryRequestId(message, bandsAsc);
		if (requestId === 'not-delivered') {
			queued.push({ kind: 'mailbox', message, variant: 'queued', turn: null });
			continue;
		}
		if (requestId === 'before-loaded') {
			earlierUnloadedCount++;
			continue;
		}
		if (requestId === null) {
			deliveredAfterLoaded.push({ kind: 'mailbox', message, variant: 'delivered', turn: null });
			continue;
		}
		const turn = findTurnForId(requestId);
		if (turn === null || !turnRowIndex.has(turn.turn_id)) {
			earlierUnloadedCount++;
			continue;
		}
		const row: MailboxRow = { kind: 'mailbox', message, variant: 'delivered', turn };
		const existing = attachByTurnId.get(turn.turn_id);
		if (existing) existing.push(row);
		else attachByTurnId.set(turn.turn_id, [row]);
	}

	const out: TimelineRow[] = [];
	for (const row of rows) {
		if (row.kind === 'turn') {
			const attached = attachByTurnId.get(row.turn.turn_id);
			if (attached) out.push(...attached);
		}
		out.push(row);
	}
	out.push(...deliveredAfterLoaded);
	out.push(...queued);

	return { rows: out, earlierUnloadedCount };
}

export interface SteerMergeResult {
	rows: TimelineRow[];
	earlierUnloadedCount: number;
}

export function mergeSteerRows(
	rows: readonly TimelineRow[],
	interventions: readonly SessionSteeringRow[],
	anchorForRequestId: ReadonlyMap<number, number>
): SteerMergeResult {
	const rowIndexByAnchor = new Map<number, number>();
	rows.forEach((row, i) => {
		if (row.kind === 'turn') rowIndexByAnchor.set(row.turn.start_request_id, i);
		else if (row.kind === 'housekeeping') rowIndexByAnchor.set(row.anchorId, i);
	});

	const attachByRowIndex = new Map<number, SteerRow[]>();
	let earlierUnloadedCount = 0;

	for (const intervention of interventions) {
		const anchorRequestId = intervention.trigger_request_id ?? intervention.sent_request_id;
		const hostAnchor =
			anchorRequestId === null ? undefined : anchorForRequestId.get(anchorRequestId);
		const rowIdx = hostAnchor === undefined ? undefined : rowIndexByAnchor.get(hostAnchor);
		if (hostAnchor === undefined || rowIdx === undefined) {
			earlierUnloadedCount++;
			continue;
		}
		const steerRow: SteerRow = { kind: 'steer', intervention, anchorId: hostAnchor };
		const existing = attachByRowIndex.get(rowIdx);
		if (existing) existing.push(steerRow);
		else attachByRowIndex.set(rowIdx, [steerRow]);
	}

	const out: TimelineRow[] = [];
	rows.forEach((row, i) => {
		out.push(row);
		const attached = attachByRowIndex.get(i);
		if (attached) out.push(...attached);
	});

	return { rows: out, earlierUnloadedCount };
}

export type RoleFilter = 'all' | 'prompts' | 'tools' | 'errors' | 'dispatch';

export function turnVisible(
	turn: OperatorTurnDto,
	toolCalls: readonly ToolUseDto[],
	role: RoleFilter,
	tool: string,
	query: string
): boolean {
	if (role === 'errors' && turn.tool_error_count === 0) return false;
	if (role === 'dispatch' && dispatchCountOf(turn) === 0) return false;
	if (tool !== 'all' && !toolCalls.some((t) => t.tool_name === tool)) return false;
	if (query.length > 0) {
		const q = query.toLowerCase();
		const numMatch = /^#?\d+$/.test(q);
		if (numMatch) {
			const n = Number(q.replace('#', ''));
			if (n >= turn.start_request_number && n <= turn.end_request_number) return true;
		}
		const hay = `${turn.user_prompt} ${turn.stop_response ?? turn.final_assistant_text} ${toolCalls
			.map((t) => `${t.tool_name} ${toolTarget(t)}`)
			.join(' ')}`.toLowerCase();
		if (!hay.includes(q)) return false;
	}
	return true;
}

export function splitFirstLine(text: string): { first: string; rest: string; hasMore: boolean } {
	const nl = text.indexOf('\n');
	if (nl === -1) return { first: text, rest: '', hasMore: false };
	const rest = text.slice(nl + 1).replace(/^\n+/, '');
	return { first: text.slice(0, nl), rest, hasMore: rest.trim().length > 0 };
}

export function highlightSegments(
	text: string,
	query: string
): Array<{ text: string; mark: boolean }> {
	if (query.length === 0) return [{ text, mark: false }];
	const lc = text.toLowerCase();
	const ql = query.toLowerCase();
	if (!lc.includes(ql)) return [{ text, mark: false }];
	const out: Array<{ text: string; mark: boolean }> = [];
	let i = 0;
	for (;;) {
		const idx = lc.indexOf(ql, i);
		if (idx < 0) {
			if (i < text.length) out.push({ text: text.slice(i), mark: false });
			break;
		}
		if (idx > i) out.push({ text: text.slice(i, idx), mark: false });
		out.push({ text: text.slice(idx, idx + ql.length), mark: true });
		i = idx + ql.length;
	}
	return out;
}

export type TimelineBodyState = 'not-found' | 'skeleton' | 'empty' | 'content';

export function timelineBodyState(state: {
	notFound: boolean;
	loading: boolean;
	hasDetail: boolean;
	bandCount: number;
}): TimelineBodyState {
	if (state.notFound) return 'not-found';
	if (state.loading && !state.hasDetail) return 'skeleton';
	if (state.bandCount === 0) return 'empty';
	return 'content';
}

export interface SentinelRect {
	top: number;
	bottom: number;
}

export const AUTO_LOAD_FOLD_MARGIN_PX = 24;

export function shouldAutoLoadOlder(geometry: {
	scrollTop: number;
	topThreshold: number;
	foldOffset: number;
	viewportHeight: number;
	sentinelRect: SentinelRect | undefined;
}): boolean {
	const { scrollTop, topThreshold, foldOffset, viewportHeight, sentinelRect } = geometry;
	if (sentinelRect === undefined) return false;
	if (scrollTop <= topThreshold) return true;
	if (sentinelRect.bottom <= 0) return false;
	if (sentinelRect.top >= viewportHeight) return false;
	return sentinelRect.top <= foldOffset + AUTO_LOAD_FOLD_MARGIN_PX;
}

export type LedgerCellState = 'unavailable' | 'empty' | 'pending' | 'loaded';

export function ledgerCellState(
	ledgerUnavailable: boolean,
	mappedCount: number,
	expectedCount: number
): LedgerCellState {
	if (ledgerUnavailable) return 'unavailable';
	if (mappedCount === 0) return expectedCount > 0 ? 'pending' : 'empty';
	return 'loaded';
}
