import { describe, it, expect } from 'vitest';
import type { OperatorTurnDto, RequestSummary, ToolUseDto } from '$lib/types';
import type { MailboxMessage } from '$lib/api/mailbox';
import type { SessionSteeringRow } from '$lib/api/steering';
import {
	AUTO_LOAD_FOLD_MARGIN_PX,
	buildTimelineRows,
	categoryOf,
	computeTurnToolStats,
	dispatchCountOf,
	filesTouchedOf,
	findTurnForRequestId,
	highlightSegments,
	ledgerCellState,
	mergeMailboxRows,
	mergeSteerRows,
	shouldAutoLoadOlder,
	splitFirstLine,
	timelineBodyState,
	timelineRowKey,
	toolTarget,
	toolCallMatchesQuery,
	turnVisible,
	type TimelineRow,
	type TurnRow
} from './timelineTurnView';

function band(overrides: Partial<RequestSummary>): RequestSummary {
	return {
		id: 1,
		method: 'POST',
		path: '/v1/messages',
		model: 'claude-opus-4-6',
		status: 'Complete',
		is_streaming: false,
		message_count: 2,
		max_tokens: null,
		created_at: '2026-07-02T14:00:00Z',
		completed_at: '2026-07-02T14:00:03Z',
		session_id: 's1',
		project: null,
		human_prompt: '',
		turn_type: 'user_prompt',
		turn_origin: 'primary',
		subagent_persona: null,
		response_tool_count: 0,
		response_tools_used: [],
		tool_names: [],
		request_number: 1,
		...overrides
	};
}

function turn(overrides: Partial<OperatorTurnDto>): OperatorTurnDto {
	return {
		turn_id: 1,
		session_id: 's1',
		project_id: 1,
		start_request_id: 1,
		end_request_id: 1,
		start_request_number: 1,
		end_request_number: 1,
		user_prompt: '',
		final_assistant_text: '',
		tool_call_count: 0,
		tool_error_count: 0,
		files_touched: [],
		tools_used: [],
		agents_dispatched: 0,
		weasel_findings_count: 0,
		total_input_tokens: 0,
		total_output_tokens: 0,
		duration_ms: 0,
		outcome: 'unknown',
		outcome_evidence: null,
		classified_at: null,
		created_at: '2026-07-02T14:00:00Z',
		updated_at: '2026-07-02T14:00:00Z',
		...overrides
	};
}

function toolUse(overrides: Partial<ToolUseDto>): ToolUseDto {
	return {
		request_id: 1,
		session_id: 's1',
		created_at: '2026-07-02T14:00:00Z',
		tool_id: 't1',
		tool_name: 'Read',
		input: {},
		file_path: null,
		ord: 0,
		...overrides
	};
}

function mailboxMsg(overrides: Partial<MailboxMessage> & { message_uuid: string }): MailboxMessage {
	return {
		from_session_uuid: null,
		to_session_uuid: 's1',
		body: 'test message body',
		queued_at: '2026-07-02T14:00:00Z',
		claimed_at: null,
		sent_request_id: null,
		sent_at: null,
		consumed_at: null,
		dead_lettered_at: null,
		delivery_attempts: 0,
		status: 'pending',
		...overrides
	};
}

function steerRow(
	overrides: Partial<SessionSteeringRow> & { intervention_id: number }
): SessionSteeringRow {
	return {
		session_id: 's1',
		session_uuid: 's1',
		project_path: null,
		steerer_name: 'verifier',
		steerer_display_name: 'Verifier',
		verdict: 'fired',
		authority: 'command',
		mode: 'drive',
		trigger_request_id: null,
		sent_request_id: null,
		sent_response_id: null,
		operator_turn_id: null,
		gate_reason: null,
		off_thread_topic: null,
		confidence: null,
		anchor_text: null,
		verdict_json: null,
		payload_text: 'You claimed the tests pass, but no test command ran this turn.',
		steerer_model: null,
		sent_at: '2026-07-02T14:05:00Z',
		processed_at: null,
		expires_at: null,
		reverted_at: null,
		revert_reason: null,
		created_at: '2026-07-02T14:05:00Z',
		...overrides
	};
}

describe('findTurnForRequestId', () => {
	const t1 = turn({ turn_id: 1, start_request_id: 100, end_request_id: 120 });
	const t2 = turn({ turn_id: 2, start_request_id: 200, end_request_id: 205 });
	const t3 = turn({ turn_id: 3, start_request_id: 300, end_request_id: 310 });
	const turnsAsc = [t1, t2, t3];

	it('attributes a request inside a turn range to that turn', () => {
		expect(findTurnForRequestId(turnsAsc, 110)?.turn_id).toBe(1);
		expect(findTurnForRequestId(turnsAsc, 203)?.turn_id).toBe(2);
	});

	it('attributes the boundary request ids to the turn that starts there', () => {
		expect(findTurnForRequestId(turnsAsc, 100)?.turn_id).toBe(1);
		expect(findTurnForRequestId(turnsAsc, 200)?.turn_id).toBe(2);
	});

	it('keeps a request past a turn end but before the next start in that turn', () => {
		expect(findTurnForRequestId(turnsAsc, 121)?.turn_id).toBe(1);
		expect(findTurnForRequestId(turnsAsc, 199)?.turn_id).toBe(1);
		expect(findTurnForRequestId(turnsAsc, 206)?.turn_id).toBe(2);
		expect(findTurnForRequestId(turnsAsc, 299)?.turn_id).toBe(2);
	});

	it('attributes in-flight requests past the newest turn end to that turn', () => {
		expect(findTurnForRequestId(turnsAsc, 311)?.turn_id).toBe(3);
		expect(findTurnForRequestId(turnsAsc, 99_999)?.turn_id).toBe(3);
	});

	it('returns null only for requests preceding the first turn', () => {
		expect(findTurnForRequestId(turnsAsc, 99)).toBeNull();
		expect(findTurnForRequestId(turnsAsc, 1)).toBeNull();
	});

	it('returns null when there are no turns', () => {
		expect(findTurnForRequestId([], 100)).toBeNull();
	});

	it('resolves overlapping turn ranges to the later-starting turn', () => {
		const a = turn({ turn_id: 1, start_request_id: 100, end_request_id: 250 });
		const b = turn({ turn_id: 2, start_request_id: 200, end_request_id: 210 });
		expect(findTurnForRequestId([a, b], 220)?.turn_id).toBe(2);
	});

	it('sends every request of a gapped session to a turn, not housekeeping', () => {
		const ids = [100, 121, 150, 199, 200, 206, 299, 300, 311, 400];
		const orphans = ids.filter((id) => findTurnForRequestId(turnsAsc, id) === null);
		expect(orphans).toEqual([]);
	});
});

describe('buildTimelineRows', () => {
	it('places every band in exactly one row (inspection-fidelity invariant)', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 2, end_request_id: 3 });
		const bands: RequestSummary[] = [
			band({ id: 1, turn_origin: 'quota_probe', message_count: 0 }),
			band({ id: 2, turn_origin: 'primary_first' }),
			band({ id: 3, turn_origin: 'primary', turn_type: 'tool_loop' }),
			band({ id: 4, turn_origin: 'title_sidecar', message_count: 0 })
		];
		const findTurn = (id: number): OperatorTurnDto | null => (id >= 2 && id <= 3 ? t1 : null);
		const { rows, anchorForRequestId } = buildTimelineRows(bands, findTurn);

		const covered = new Set<number>();
		for (const row of rows) {
			if (row.kind === 'turn') {
				covered.add(row.turn.start_request_id);
			} else if (row.kind === 'housekeeping') {
				for (const item of row.items) covered.add(item.id);
			}
		}
		for (const b of bands) {
			const turnOwner = findTurn(b.id);
			covered.has(turnOwner ? turnOwner.start_request_id : b.id);
			expect(covered.has(turnOwner ? turnOwner.start_request_id : b.id)).toBe(true);
		}

		expect(anchorForRequestId).toEqual(
			new Map([
				[1, 1],
				[2, 2],
				[3, 2],
				[4, 4]
			])
		);

		expect(rows).toEqual([
			{
				kind: 'housekeeping',
				anchorId: 1,
				items: [expect.objectContaining({ id: 1, label: 'quota probe' })]
			},
			{
				kind: 'turn',
				turn: t1,
				live: false,
				firstRequestId: 2,
				firstRequestNumber: 1,
				lastRequestNumber: 1
			},
			{
				kind: 'housekeeping',
				anchorId: 4,
				items: [expect.objectContaining({ id: 4, label: 'title sidecar' })]
			}
		]);
	});

	it('marks a turn live when any of its bands is Pending/Streaming', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 1, end_request_id: 2 });
		const bands: RequestSummary[] = [
			band({ id: 1, turn_origin: 'primary_first', status: 'Complete' }),
			band({
				id: 2,
				turn_origin: 'primary',
				turn_type: 'tool_loop',
				status: 'Streaming',
				is_streaming: true
			})
		];
		const findTurn = (): OperatorTurnDto => t1;
		const { rows } = buildTimelineRows(bands, findTurn);
		expect(rows).toEqual([
			{
				kind: 'turn',
				turn: t1,
				live: true,
				firstRequestId: 1,
				firstRequestNumber: 1,
				lastRequestNumber: 1
			}
		]);
	});

	it('emits no housekeeping row between two adjacent turns with no out-of-turn traffic', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 1, end_request_id: 1 });
		const t2 = turn({ turn_id: 2, start_request_id: 2, end_request_id: 2 });
		const bands: RequestSummary[] = [
			band({ id: 1, turn_origin: 'primary_first' }),
			band({ id: 2, turn_origin: 'primary_first' })
		];
		const findTurn = (id: number): OperatorTurnDto | null => (id === 1 ? t1 : id === 2 ? t2 : null);
		const { rows } = buildTimelineRows(bands, findTurn);

		expect(rows).toEqual([
			{
				kind: 'turn',
				turn: t1,
				live: false,
				firstRequestId: 1,
				firstRequestNumber: 1,
				lastRequestNumber: 1
			},
			{
				kind: 'turn',
				turn: t2,
				live: false,
				firstRequestId: 2,
				firstRequestNumber: 1,
				lastRequestNumber: 1
			}
		]);
		expect(rows.some((r) => r.kind === 'housekeeping')).toBe(false);
	});

	it('merges every band of a turn into ONE TurnRow at its first band when out-of-turn traffic interleaves mid-turn — the interleaved traffic renders after it as its own housekeeping row', () => {
		const t1 = turn({ turn_id: 7, start_request_id: 1, end_request_id: 3 });
		const bands: RequestSummary[] = [
			band({ id: 1, turn_origin: 'primary_first' }),
			band({ id: 2, turn_origin: 'title_sidecar', message_count: 0 }),
			band({ id: 3, turn_origin: 'primary', turn_type: 'tool_loop' })
		];
		const findTurn = (id: number): OperatorTurnDto | null => (id === 1 || id === 3 ? t1 : null);
		const { rows } = buildTimelineRows(bands, findTurn);

		expect(rows).toEqual([
			{
				kind: 'turn',
				turn: t1,
				live: false,
				firstRequestId: 1,
				firstRequestNumber: 1,
				lastRequestNumber: 1
			},
			{
				kind: 'housekeeping',
				anchorId: 2,
				items: [expect.objectContaining({ id: 2, label: 'title sidecar' })]
			}
		]);

		const keys = rows.map(timelineRowKey);
		expect(new Set(keys).size).toBe(keys.length);
	});
});

describe('TurnRow request-number range', () => {
	it('covers every band the turn absorbed, not just its stamped end', () => {
		const t1 = turn({
			turn_id: 1,
			start_request_id: 10,
			end_request_id: 11,
			start_request_number: 1,
			end_request_number: 2
		});
		const bands = [
			band({ id: 10, request_number: 1 }),
			band({ id: 11, request_number: 2 }),
			band({ id: 12, request_number: 3 }),
			band({ id: 13, request_number: 4 })
		];
		const { rows } = buildTimelineRows(bands, (id) => findTurnForRequestId([t1], id));
		const turnRows = rows.filter((r): r is TurnRow => r.kind === 'turn');
		expect(turnRows).toHaveLength(1);
		expect(turnRows[0]!.firstRequestNumber).toBe(1);
		expect(turnRows[0]!.lastRequestNumber).toBe(4);
	});
});

describe('timelineRowKey', () => {
	it('derives the turn key from turn_id + firstRequestId — a stable per-row identity, not (post-merge) a collision guard', () => {
		const t1 = turn({ turn_id: 7, start_request_id: 1, end_request_id: 3 });
		const row: TurnRow = {
			kind: 'turn',
			turn: t1,
			live: false,
			firstRequestId: 1,
			firstRequestNumber: 1,
			lastRequestNumber: 1
		};
		expect(timelineRowKey(row)).toBe('t7:1');
	});

	it('keys housekeeping, mailbox, and steer rows by their own identity fields', () => {
		const hk: TimelineRow = { kind: 'housekeeping', anchorId: 5, items: [] };
		const mb: TimelineRow = {
			kind: 'mailbox',
			message: mailboxMsg({ message_uuid: 'm-9' }),
			variant: 'queued',
			turn: null
		};
		const st: TimelineRow = {
			kind: 'steer',
			intervention: steerRow({ intervention_id: 3 }),
			anchorId: 5
		};
		expect(timelineRowKey(hk)).toBe('hk5');
		expect(timelineRowKey(mb)).toBe('mm-9');
		expect(timelineRowKey(st)).toBe('s3');
	});
});

describe('categoryOf', () => {
	it('classifies tools into the four timeline categories', () => {
		expect(categoryOf('Bash')).toBe('exec');
		expect(categoryOf('Task')).toBe('dispatch');
		expect(categoryOf('Edit')).toBe('edit');
		expect(categoryOf('Read')).toBe('read');
		expect(categoryOf('mcp__proxy__list_sessions')).toBe('read');
	});
});

describe('computeTurnToolStats', () => {
	it('counts categories and errors', () => {
		const calls = [
			toolUse({ tool_name: 'Read' }),
			toolUse({
				tool_name: 'Bash',
				result: {
					tool_use_id: 't',
					is_error: true,
					content: '',
					content_sha256: '',
					content_bytes: 1
				}
			}),
			toolUse({ tool_name: 'Task' })
		];
		const stats = computeTurnToolStats(calls);
		expect(stats.catCounts).toEqual({ read: 1, edit: 0, exec: 1, dispatch: 1 });
		expect(stats.errorCount).toBe(1);
	});
});

describe('toolTarget', () => {
	it('prefers file_path, then a documented per-tool field', () => {
		expect(toolTarget(toolUse({ tool_name: 'Read', file_path: 'src/a.ts' }))).toBe('src/a.ts');
		expect(toolTarget(toolUse({ tool_name: 'Bash', input: { command: 'ls -la' } }))).toBe('ls -la');
		expect(toolTarget(toolUse({ tool_name: 'Grep', input: { pattern: 'foo' } }))).toBe('foo');
	});
});

describe('toolCallMatchesQuery', () => {
	it('matches on tool name, target, or #request_id', () => {
		const t = toolUse({ tool_name: 'Edit', file_path: 'src/lib/foo.ts', request_id: 42 });
		expect(toolCallMatchesQuery(t, 'foo.ts')).toBe(true);
		expect(toolCallMatchesQuery(t, '#42')).toBe(true);
		expect(toolCallMatchesQuery(t, 'bash')).toBe(false);
	});
});

describe('splitFirstLine', () => {
	it('splits at the first newline without slicing by character count', () => {
		expect(splitFirstLine('one line only')).toEqual({
			first: 'one line only',
			rest: '',
			hasMore: false
		});
		expect(splitFirstLine('first\nsecond\nthird')).toEqual({
			first: 'first',
			rest: 'second\nthird',
			hasMore: true
		});
	});
});

describe('filesTouchedOf / dispatchCountOf', () => {
	it('reads the JSONB array fields defensively', () => {
		expect(filesTouchedOf(turn({ files_touched: ['a.ts', 'b.ts'] }))).toEqual(['a.ts', 'b.ts']);
		expect(filesTouchedOf(turn({ files_touched: null }))).toEqual([]);
		expect(dispatchCountOf(turn({ agents_dispatched: 3 }))).toBe(3);
		expect(dispatchCountOf(turn({ agents_dispatched: ['a', 'b'] }))).toBe(2);
		expect(dispatchCountOf(turn({ agents_dispatched: null }))).toBe(0);
	});
});

describe('turnVisible', () => {
	it('role=errors requires tool_error_count > 0', () => {
		const withErrors = turn({ tool_error_count: 1 });
		const noErrors = turn({ tool_error_count: 0 });
		expect(turnVisible(withErrors, [], 'errors', 'all', '')).toBe(true);
		expect(turnVisible(noErrors, [], 'errors', 'all', '')).toBe(false);
	});

	it('role=dispatch requires agents_dispatched > 0', () => {
		const dispatched = turn({ agents_dispatched: 1 });
		const notDispatched = turn({ agents_dispatched: 0 });
		expect(turnVisible(dispatched, [], 'dispatch', 'all', '')).toBe(true);
		expect(turnVisible(notDispatched, [], 'dispatch', 'all', '')).toBe(false);
	});

	it('an active tool filter requires a matching tool call in the turn', () => {
		const t = turn({});
		const calls = [toolUse({ tool_name: 'Bash' })];
		expect(turnVisible(t, calls, 'all', 'Bash', '')).toBe(true);
		expect(turnVisible(t, calls, 'all', 'Edit', '')).toBe(false);
	});

	it("a numeric query in the turn's request-number range matches without needing text overlap", () => {
		const t = turn({
			start_request_number: 5,
			end_request_number: 8,
			user_prompt: 'unrelated text'
		});
		expect(turnVisible(t, [], 'all', 'all', '6')).toBe(true);
		expect(turnVisible(t, [], 'all', 'all', '#6')).toBe(true);
	});

	it('a numeric query outside the range falls back to a plain text search', () => {
		const matches = turn({
			start_request_number: 5,
			end_request_number: 8,
			user_prompt: 'contains 42 in prose'
		});
		const noMatch = turn({
			start_request_number: 5,
			end_request_number: 8,
			user_prompt: 'nothing here'
		});
		expect(turnVisible(matches, [], 'all', 'all', '42')).toBe(true);
		expect(turnVisible(noMatch, [], 'all', 'all', '42')).toBe(false);
	});

	it('an empty query passes every turn through regardless of content', () => {
		const t = turn({ user_prompt: '', final_assistant_text: '' });
		expect(turnVisible(t, [], 'all', 'all', '')).toBe(true);
	});
});

describe('mergeMailboxRows', () => {
	it('treats a channel-pushed message as delivered and anchors it to the first request at or after the push', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 2, end_request_id: 3 });
		const t2 = turn({ turn_id: 2, start_request_id: 4, end_request_id: 5 });
		const bands: RequestSummary[] = [
			band({ id: 2, created_at: '2026-07-02T14:00:00Z' }),
			band({ id: 3, created_at: '2026-07-02T14:01:00Z' }),
			band({ id: 4, created_at: '2026-07-02T14:05:00Z' }),
			band({ id: 5, created_at: '2026-07-02T14:06:00Z' })
		];
		const findTurn = (id: number): OperatorTurnDto => (id <= 3 ? t1 : t2);
		const { rows } = buildTimelineRows(bands, findTurn);

		const msg = mailboxMsg({
			message_uuid: 'm-push',
			status: 'consumed',
			channel_delivered_at: '2026-07-02T14:04:59Z',
			consumed_at: '2026-07-02T14:05:30Z'
		});
		const { rows: merged, earlierUnloadedCount } = mergeMailboxRows(rows, [msg], findTurn, bands);

		expect(earlierUnloadedCount).toBe(0);
		expect(merged.map((r) => r.kind)).toEqual(['turn', 'mailbox', 'turn']);
		expect(merged[1]).toEqual({ kind: 'mailbox', message: msg, variant: 'delivered', turn: t2 });
	});

	it('shows a pushed message newer than every loaded request as delivered, not queued', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 2, end_request_id: 2 });
		const bands: RequestSummary[] = [band({ id: 2, created_at: '2026-07-02T14:00:00Z' })];
		const findTurn = (): OperatorTurnDto => t1;
		const { rows } = buildTimelineRows(bands, findTurn);

		const msg = mailboxMsg({
			message_uuid: 'm-late',
			status: 'delivered',
			channel_delivered_at: '2026-07-02T14:10:00Z'
		});
		const { rows: merged } = mergeMailboxRows(rows, [msg], findTurn, bands);

		expect(merged.at(-1)).toEqual({
			kind: 'mailbox',
			message: msg,
			variant: 'delivered',
			turn: null
		});
	});

	it('counts a pushed message older than the loaded window as earlier-unloaded', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 2, end_request_id: 2 });
		const bands: RequestSummary[] = [band({ id: 2, created_at: '2026-07-02T14:00:00Z' })];
		const findTurn = (): OperatorTurnDto => t1;
		const { rows } = buildTimelineRows(bands, findTurn);

		const msg = mailboxMsg({
			message_uuid: 'm-old',
			status: 'consumed',
			channel_delivered_at: '2026-07-02T13:00:00Z',
			consumed_at: '2026-07-02T13:01:00Z'
		});
		const { rows: merged, earlierUnloadedCount } = mergeMailboxRows(rows, [msg], findTurn, bands);

		expect(earlierUnloadedCount).toBe(1);
		expect(merged.some((r) => r.kind === 'mailbox')).toBe(false);
	});

	it('attaches a mid-turn delivered message directly above the turn row (turn-level precision, not a mid-turn splice)', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 2, end_request_id: 5 });
		const bands: RequestSummary[] = [2, 3, 4, 5].map((id) => band({ id }));
		const findTurn = (): OperatorTurnDto => t1;
		const { rows } = buildTimelineRows(bands, findTurn);

		const msg = mailboxMsg({
			message_uuid: 'm-1',
			status: 'delivered',
			sent_request_id: 4,
			sent_at: '2026-07-02T14:05:00Z'
		});
		const { rows: merged, earlierUnloadedCount } = mergeMailboxRows(rows, [msg], findTurn);

		expect(earlierUnloadedCount).toBe(0);
		expect(merged).toEqual([
			{ kind: 'mailbox', message: msg, variant: 'delivered', turn: t1 },
			{
				kind: 'turn',
				turn: t1,
				live: false,
				firstRequestId: 2,
				firstRequestNumber: 1,
				lastRequestNumber: 1
			}
		]);
	});

	it('renders two rows, keyed by message_uuid, when two messages share one sent_request_id', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 2, end_request_id: 3 });
		const bands: RequestSummary[] = [2, 3].map((id) => band({ id }));
		const findTurn = (): OperatorTurnDto => t1;
		const { rows } = buildTimelineRows(bands, findTurn);

		const msgA = mailboxMsg({
			message_uuid: 'm-a',
			status: 'delivered',
			sent_request_id: 3,
			queued_at: '2026-07-02T13:59:00Z'
		});
		const msgB = mailboxMsg({
			message_uuid: 'm-b',
			status: 'delivered',
			sent_request_id: 3,
			queued_at: '2026-07-02T13:58:00Z'
		});
		const { rows: merged } = mergeMailboxRows(rows, [msgA, msgB], findTurn);

		const mailboxRows = merged.filter((r) => r.kind === 'mailbox');
		expect(mailboxRows).toHaveLength(2);
		expect(mailboxRows.map((r) => r.message.message_uuid)).toEqual(['m-a', 'm-b']);
		expect(merged[2]).toEqual({
			kind: 'turn',
			turn: t1,
			live: false,
			firstRequestId: 2,
			firstRequestNumber: 1,
			lastRequestNumber: 1
		});
	});

	it('attaches consumed and dead_lettered messages with a sent_request_id to their turn — terminal status never demotes a delivered message to queued (issue #186)', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 2, end_request_id: 5 });
		const bands: RequestSummary[] = [2, 3, 4, 5].map((id) => band({ id }));
		const findTurn = (): OperatorTurnDto => t1;
		const { rows } = buildTimelineRows(bands, findTurn);

		const consumed = mailboxMsg({
			message_uuid: 'm-consumed',
			status: 'consumed',
			sent_request_id: 3
		});
		const deadLettered = mailboxMsg({
			message_uuid: 'm-dead',
			status: 'dead_lettered',
			sent_request_id: 4
		});
		const { rows: merged, earlierUnloadedCount } = mergeMailboxRows(
			rows,
			[consumed, deadLettered],
			findTurn
		);

		expect(earlierUnloadedCount).toBe(0);
		expect(merged).toEqual([
			{ kind: 'mailbox', message: consumed, variant: 'delivered', turn: t1 },
			{ kind: 'mailbox', message: deadLettered, variant: 'delivered', turn: t1 },
			{
				kind: 'turn',
				turn: t1,
				live: false,
				firstRequestId: 2,
				firstRequestNumber: 1,
				lastRequestNumber: 1
			}
		]);
		expect(merged.filter((r) => r.kind === 'mailbox' && r.variant === 'queued')).toEqual([]);
	});

	it('renders a pending message as a queued row at the timeline end — visible before any turn claims it', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 2, end_request_id: 3 });
		const bands: RequestSummary[] = [2, 3].map((id) => band({ id }));
		const findTurn = (): OperatorTurnDto => t1;
		const { rows } = buildTimelineRows(bands, findTurn);

		const pending = mailboxMsg({ message_uuid: 'm-pending', status: 'pending' });
		const { rows: merged } = mergeMailboxRows(rows, [pending], findTurn);

		expect(merged).toEqual([
			{
				kind: 'turn',
				turn: t1,
				live: false,
				firstRequestId: 2,
				firstRequestNumber: 1,
				lastRequestNumber: 1
			},
			{ kind: 'mailbox', message: pending, variant: 'queued', turn: null }
		]);
	});

	it('renders a claimed (not yet delivered) message as queued too — presence, not `delivered`, gates visibility', () => {
		const claimed = mailboxMsg({
			message_uuid: 'm-claimed',
			status: 'claimed',
			claimed_at: '2026-07-02T14:00:00Z'
		});
		const { rows: merged, earlierUnloadedCount } = mergeMailboxRows([], [claimed], () => null);
		expect(earlierUnloadedCount).toBe(0);
		expect(merged).toEqual([{ kind: 'mailbox', message: claimed, variant: 'queued', turn: null }]);
	});

	it('counts (never inline-renders) a delivered message whose host turn has no row in the loaded page', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 50, end_request_id: 52 });
		const { rows } = buildTimelineRows([], () => null);
		expect(rows).toEqual([]);

		const msg = mailboxMsg({ message_uuid: 'm-earlier', status: 'delivered', sent_request_id: 51 });
		const { rows: merged, earlierUnloadedCount } = mergeMailboxRows(rows, [msg], () => t1);

		expect(merged).toEqual([]);
		expect(earlierUnloadedCount).toBe(1);
	});

	it("leaves buildTimelineRows' anchorForRequestId untouched (anchor-map integrity unaffected by mailbox rows)", () => {
		const t1 = turn({ turn_id: 1, start_request_id: 2, end_request_id: 3 });
		const bands: RequestSummary[] = [
			band({ id: 1, turn_origin: 'quota_probe', message_count: 0 }),
			band({ id: 2 }),
			band({ id: 3, turn_type: 'tool_loop' })
		];
		const findTurn = (id: number): OperatorTurnDto | null => (id >= 2 && id <= 3 ? t1 : null);
		const { rows, anchorForRequestId } = buildTimelineRows(bands, findTurn);
		const anchorSnapshot = new Map(anchorForRequestId);

		const msg = mailboxMsg({ message_uuid: 'm-1', status: 'delivered', sent_request_id: 3 });
		mergeMailboxRows(rows, [msg], findTurn);

		expect(anchorForRequestId).toEqual(anchorSnapshot);
	});
});

describe('mergeSteerRows', () => {
	it('anchors a steer via trigger_request_id, splicing it immediately after the host housekeeping fold — even though neither the trigger nor the delivered request is a turn member', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 3, end_request_id: 4 });
		const bands: RequestSummary[] = [1, 2, 3, 4].map((id) => band({ id }));
		const findTurn = (id: number): OperatorTurnDto | null => (id >= 3 && id <= 4 ? t1 : null);
		const { rows, anchorForRequestId } = buildTimelineRows(bands, findTurn);
		expect(rows).toEqual([
			{ kind: 'housekeeping', anchorId: 1, items: expect.any(Array) },
			{
				kind: 'turn',
				turn: t1,
				live: false,
				firstRequestId: 3,
				firstRequestNumber: 1,
				lastRequestNumber: 1
			}
		]);

		const s = steerRow({ intervention_id: 1, trigger_request_id: 2, sent_request_id: 10 });
		const { rows: merged, earlierUnloadedCount } = mergeSteerRows(rows, [s], anchorForRequestId);

		expect(earlierUnloadedCount).toBe(0);
		expect(merged).toEqual([rows[0], { kind: 'steer', intervention: s, anchorId: 1 }, rows[1]]);
	});

	it('falls back to sent_request_id when trigger_request_id is null, anchoring after the host turn row', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 2, end_request_id: 3 });
		const bands: RequestSummary[] = [2, 3].map((id) => band({ id }));
		const findTurn = (): OperatorTurnDto => t1;
		const { rows, anchorForRequestId } = buildTimelineRows(bands, findTurn);

		const s = steerRow({ intervention_id: 2, trigger_request_id: null, sent_request_id: 3 });
		const { rows: merged, earlierUnloadedCount } = mergeSteerRows(rows, [s], anchorForRequestId);

		expect(earlierUnloadedCount).toBe(0);
		expect(merged).toEqual([
			{
				kind: 'turn',
				turn: t1,
				live: false,
				firstRequestId: 2,
				firstRequestNumber: 1,
				lastRequestNumber: 1
			},
			{ kind: 'steer', intervention: s, anchorId: 2 }
		]);
	});

	it('stacks multiple steers anchored to the same host row in input order', () => {
		const t1 = turn({ turn_id: 1, start_request_id: 2, end_request_id: 3 });
		const bands: RequestSummary[] = [2, 3].map((id) => band({ id }));
		const findTurn = (): OperatorTurnDto => t1;
		const { rows, anchorForRequestId } = buildTimelineRows(bands, findTurn);

		const sA = steerRow({ intervention_id: 10, sent_request_id: 3 });
		const sB = steerRow({ intervention_id: 11, sent_request_id: 2 });
		const { rows: merged } = mergeSteerRows(rows, [sA, sB], anchorForRequestId);

		expect(merged).toEqual([
			{
				kind: 'turn',
				turn: t1,
				live: false,
				firstRequestId: 2,
				firstRequestNumber: 1,
				lastRequestNumber: 1
			},
			{ kind: 'steer', intervention: sA, anchorId: 2 },
			{ kind: 'steer', intervention: sB, anchorId: 2 }
		]);
	});

	it("counts (never drops) a fired steer whose anchor request is not reachable in anchorForRequestId — the page holding it hasn't loaded", () => {
		const s = steerRow({ intervention_id: 20, sent_request_id: 99 });
		const { rows: merged, earlierUnloadedCount } = mergeSteerRows([], [s], new Map());

		expect(merged).toEqual([]);
		expect(earlierUnloadedCount).toBe(1);
	});

	it('counts (never drops) a fired steer with no request id at all — defensive, should not occur for a real fired row', () => {
		const s = steerRow({ intervention_id: 21, trigger_request_id: null, sent_request_id: null });
		const { rows: merged, earlierUnloadedCount } = mergeSteerRows([], [s], new Map([[1, 1]]));

		expect(merged).toEqual([]);
		expect(earlierUnloadedCount).toBe(1);
	});
});

describe('timelineBodyState', () => {
	it('prioritises not-found over every other state', () => {
		expect(
			timelineBodyState({ notFound: true, loading: true, hasDetail: false, bandCount: 0 })
		).toBe('not-found');
	});

	it('shows the skeleton while loading and no detail has arrived yet, even if bandCount is 0', () => {
		expect(
			timelineBodyState({ notFound: false, loading: true, hasDetail: false, bandCount: 0 })
		).toBe('skeleton');
	});

	it('does not show the skeleton once detail has arrived, even mid-refresh', () => {
		expect(
			timelineBodyState({ notFound: false, loading: true, hasDetail: true, bandCount: 0 })
		).toBe('empty');
	});

	it('shows "empty" only once genuinely loaded with zero requests', () => {
		expect(
			timelineBodyState({ notFound: false, loading: false, hasDetail: true, bandCount: 0 })
		).toBe('empty');
	});

	it('shows content once there is at least one band', () => {
		expect(
			timelineBodyState({ notFound: false, loading: false, hasDetail: true, bandCount: 3 })
		).toBe('content');
	});
});

describe('highlightSegments', () => {
	it('returns the text unmarked for an empty query', () => {
		expect(highlightSegments('hello world', '')).toEqual([{ text: 'hello world', mark: false }]);
	});

	it('returns the text unmarked when the query does not match', () => {
		expect(highlightSegments('hello world', 'xyz')).toEqual([{ text: 'hello world', mark: false }]);
	});

	it('marks a single interior match, keeping the surrounding text', () => {
		expect(highlightSegments('hello world', 'lo w')).toEqual([
			{ text: 'hel', mark: false },
			{ text: 'lo w', mark: true },
			{ text: 'orld', mark: false }
		]);
	});

	it('marks every occurrence of a repeated match', () => {
		expect(highlightSegments('ababa', 'a')).toEqual([
			{ text: 'a', mark: true },
			{ text: 'b', mark: false },
			{ text: 'a', mark: true },
			{ text: 'b', mark: false },
			{ text: 'a', mark: true }
		]);
	});

	it('marks a match at the very start with no leading segment', () => {
		expect(highlightSegments('hello', 'he')).toEqual([
			{ text: 'he', mark: true },
			{ text: 'llo', mark: false }
		]);
	});

	it('marks a match at the very end with no trailing segment', () => {
		expect(highlightSegments('hello', 'lo')).toEqual([
			{ text: 'hel', mark: false },
			{ text: 'lo', mark: true }
		]);
	});

	it('matches case-insensitively while preserving the original case in output', () => {
		expect(highlightSegments('Hello WORLD', 'hello')).toEqual([
			{ text: 'Hello', mark: true },
			{ text: ' WORLD', mark: false }
		]);
	});
});

describe('ledgerCellState', () => {
	it('reads "unavailable" when the ledger failed to load, regardless of count', () => {
		expect(ledgerCellState(true, 0, 0)).toBe('unavailable');
		expect(ledgerCellState(true, 3, 3)).toBe('unavailable');
	});

	it('reads "empty" when the ledger loaded and this turn genuinely made no calls', () => {
		expect(ledgerCellState(false, 0, 0)).toBe('empty');
	});

	it('reads "loaded" when the ledger loaded and this turn has calls', () => {
		expect(ledgerCellState(false, 5, 5)).toBe('loaded');
	});

	it('reads "pending" when the turn aggregate is ahead of the mapped ledger slice', () => {
		expect(ledgerCellState(false, 0, 5)).toBe('pending');
	});
});

describe('shouldAutoLoadOlder', () => {
	const FOLD = 44 + 28;
	const VIEWPORT = 900;
	const HINT_HEIGHT = 27;
	const HINT_DOC_TOP = 1686;

	function atScroll(scrollTop: number) {
		const top = HINT_DOC_TOP - scrollTop;
		return {
			scrollTop,
			topThreshold: 80,
			foldOffset: FOLD,
			viewportHeight: VIEWPORT,
			sentinelRect: { top, bottom: top + HINT_HEIGHT }
		};
	}

	it('fires at the top of the document, with the hint still far below the fold', () => {
		const geometry = atScroll(0);
		expect(geometry.sentinelRect.top).toBeGreaterThan(VIEWPORT);
		expect(shouldAutoLoadOlder(geometry)).toBe(true);
	});

	it('fires anywhere within the top threshold', () => {
		expect(shouldAutoLoadOlder(atScroll(80))).toBe(true);
	});

	it('fires with the hint parked at the fold line, deep down the document', () => {
		const geometry = atScroll(HINT_DOC_TOP - FOLD);
		expect(geometry.scrollTop).toBeGreaterThan(1000);
		expect(geometry.sentinelRect.top).toBe(FOLD);
		expect(shouldAutoLoadOlder(geometry)).toBe(true);
	});

	it('fires with the hint a hair below the fold, within the margin', () => {
		const geometry = atScroll(HINT_DOC_TOP - FOLD - AUTO_LOAD_FOLD_MARGIN_PX);
		expect(geometry.sentinelRect.top).toBe(FOLD + AUTO_LOAD_FOLD_MARGIN_PX);
		expect(shouldAutoLoadOlder(geometry)).toBe(true);
	});

	it('does not fire with the hint below the fold by more than the margin', () => {
		const geometry = atScroll(HINT_DOC_TOP - FOLD - AUTO_LOAD_FOLD_MARGIN_PX - 1);
		expect(shouldAutoLoadOlder(geometry)).toBe(false);
	});

	it('does not fire while the hint is still below the viewport', () => {
		const geometry = atScroll(500);
		expect(geometry.sentinelRect.top).toBeGreaterThan(VIEWPORT);
		expect(shouldAutoLoadOlder(geometry)).toBe(false);
	});

	it('does not fire once the hint has left the viewport upwards', () => {
		const geometry = atScroll(HINT_DOC_TOP + HINT_HEIGHT);
		expect(geometry.sentinelRect.bottom).toBe(0);
		expect(shouldAutoLoadOlder(geometry)).toBe(false);
		expect(shouldAutoLoadOlder(atScroll(HINT_DOC_TOP + 400))).toBe(false);
	});

	it('does not fire when there is no hint, even at the top of the document', () => {
		expect(
			shouldAutoLoadOlder({
				scrollTop: 0,
				topThreshold: 80,
				foldOffset: FOLD,
				viewportHeight: VIEWPORT,
				sentinelRect: undefined
			})
		).toBe(false);
	});

	it('fires on a page whose header leaves no room to reach the fold', () => {
		expect(
			shouldAutoLoadOlder({
				scrollTop: 12,
				topThreshold: 80,
				foldOffset: FOLD,
				viewportHeight: VIEWPORT,
				sentinelRect: { top: 640, bottom: 667 }
			})
		).toBe(true);
	});
});
