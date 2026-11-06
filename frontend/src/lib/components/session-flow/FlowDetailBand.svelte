<script lang="ts">
	import { untrack } from 'svelte';
	import type { Component } from 'svelte';
	import { Bot, Monitor, Wrench, Server, Globe, Terminal, ArrowUpRight, Maximize2 } from '@lucide/svelte';
	import CopyIconButton from '$lib/components/shared/CopyIconButton.svelte';
	import FlowPromptModal from './FlowPromptModal.svelte';
	import { loadSessionToolUses } from '$lib/stores/cache/sessionToolUses.svelte';
	import { getAccumulatedText } from '$lib/api';
	import { toolInputText, toolResultText, toolTarget } from '$lib/components/shell/timelineTurnView';
	import {
		buildAgentIdentities,
		buildGroups,
		dispatchActivityOf,
		type FlowAgentIdentity
	} from '$lib/session-flow/grouping';
	import { requestIdsForActorIds, mainSessionActorId } from '$lib/session-flow/requestFilter';
	import {
		agentCostUsd,
		agentModels,
		agentRequestSpan,
		toolCallCountsByName,
		type AgentLiveStatus
	} from '$lib/session-flow/agentDetail';
	import { activitiesOfActor } from '$lib/session-flow/actorActivities';
	import { fetchRequestDetails } from '$lib/session-flow/agentUsageFetch';
	import { rolesOf, identityOf, nodeKindOf, type FlowRole } from '$lib/session-flow/roles';
	import { formatClockAbsolute, formatCount, formatDuration } from '$lib/time';
	import { shortSessionId } from '$lib/format';
	import type { ToolUseDto } from '$lib/types';
	import type { FlowDataset, FlowSelection } from '$lib/session-flow/types';
	import type { IdentityFamily, NodeKind } from '$lib/board/types';

	let {
		dataset,
		selection,
		title = null,
		followId = null,
		onSelect,
		onFollow,
		onOpenDrawer
	}: {
		dataset: FlowDataset;
		selection: FlowSelection;
		title?: string | null;
		followId?: string | null;
		onSelect: (selection: FlowSelection) => void;
		onFollow?: (id: string | null) => void;
		onOpenDrawer: (requestId: string, kind: 'request' | 'response') => void;
	} = $props();

	const ROLE_DESCRIPTIONS: Partial<Record<string, string>> = {
		peer: 'An independent peer session, connected only through the mailbox.',
		sidecars:
			'A background sidecar request (title generation, quota checks, monitoring) — not part of the visible conversation.',
		'local-tools': 'A tool this session called directly on its own machine.',
		'mike-mcp': "Mike's own MCP server — the tools this session used to inspect and steer itself.",
		upstream: 'The Anthropic Model API — every captured request is ultimately sent here.',
		terminal: 'The terminal or launcher surface used to open or observe this session.'
	};
	function roleDescription(role: FlowRole): string {
		const known = ROLE_DESCRIPTIONS[role];
		if (known) return known;
		if (role.startsWith('other-mcp:'))
			return `An MCP server ("${role.slice('other-mcp:'.length)}") this session connected to for tools.`;
		if (role.startsWith('service:'))
			return `Part of mike's own request pipeline (${role.slice('service:'.length).replaceAll('-', ' ')}).`;
		return 'Part of the session infrastructure.';
	}

	const ICON_OF: Readonly<Record<IdentityFamily, Component<{ size?: number | string }>>> = {
		agent: Bot,
		session: Monitor,
		tool: Wrench,
		mike: Server,
		outside: Globe,
		terminal: Terminal
	};

	const STATUS_LABEL: Readonly<Record<AgentLiveStatus, string>> = {
		active: 'Active',
		finished: 'Finished',
		idle: 'Idle',
		failed: 'Failed',
		not_matched: 'Not matched'
	};

	const roles = $derived(rolesOf(dataset.actors.values(), dataset.sessionId));
	const coordinatorActorId = $derived(mainSessionActorId(dataset));
	const identities = $derived(buildAgentIdentities(dataset));
	const groups = $derived(buildGroups(dataset));

	const selectedActor = $derived(
		selection?.kind === 'actor' ? dataset.actors.get(selection.id) : undefined
	);
	const isCoordinator = $derived(
		selectedActor !== undefined && selectedActor.id === coordinatorActorId
	);
	const selectedIdentity = $derived.by((): FlowAgentIdentity | undefined => {
		if (!selectedActor || isCoordinator) return undefined;
		return identities.find((entry) => entry.actorIds.includes(selectedActor.id));
	});
	const selectedRole = $derived(selectedActor ? roles.get(selectedActor.id) : undefined);
	const selectedGroup = $derived.by(() => {
		if (!selectedActor || isCoordinator || selectedIdentity) return undefined;
		return groups.find((group) => group.memberIds.includes(selectedActor.id));
	});

	const selectedActivity = $derived(
		selection?.kind === 'activity' ? dataset.activities.get(selection.id) : undefined
	);
	const selectedLink = $derived(selection?.kind === 'link' ? selection : undefined);

	const ACTIVITY_KIND_LABEL: Partial<Record<string, string>> = {
		dispatches: 'Agent dispatch',
		results: 'Tool result',
		messages: 'Mailbox message',
		worker_status: 'Worker status',
		manifest_stages: 'Pipeline stage',
		lesson_injections: 'Lesson injection',
		verifier_evaluations: 'Answer check',
		judgments: "Mike's judgement",
		llm_calls: 'Mike LLM call',
		embeddings: 'Memory embedding',
		consolidation: 'Memory consolidation',
		interventions: 'Steering message',
		requests: 'Request',
		tools: 'Tool call'
	};

	let activityToolRow = $state<ToolUseDto | null>(null);
	let activityToolKey = $state<string | null>(null);
	$effect(() => {
		const activity = selectedActivity;
		const sid = dataset.sessionId;
		untrack(() => {
			if (!activity || activity.kind !== 'tools' || activity.metadata.wire_tool_use_id === null) {
				activityToolRow = null;
				activityToolKey = null;
				return;
			}
			const key = activity.id;
			if (activityToolKey === key) return;
			activityToolKey = key;
			const wireId = activity.metadata.wire_tool_use_id;
			loadSessionToolUses(sid)
				.then((rows) => {
					if (activityToolKey !== key) return;
					activityToolRow = rows.find((r) => r.tool_id === wireId) ?? null;
				})
				.catch(() => {
					if (activityToolKey !== key) return;
					activityToolRow = null;
				});
		});
	});

	const relevantActorIds = $derived.by((): readonly string[] => {
		if (isCoordinator && selectedActor) return [selectedActor.id];
		if (selectedIdentity) return selectedIdentity.workerActorIds;
		if (selectedGroup) return selectedGroup.memberIds;
		return [];
	});
	const models = $derived(agentModels(dataset, relevantActorIds));
	const span = $derived(agentRequestSpan(dataset, relevantActorIds));
	const requestIds = $derived(requestIdsForActorIds(dataset, relevantActorIds));
	const costUsd = $derived(agentCostUsd(dataset, relevantActorIds));

	type DispatchInfo =
		| { status: 'idle' }
		| { status: 'loading' }
		| {
				status: 'loaded';
				prompt: string | null;
				description: string | null;
				subagentType: string | null;
				resultRow: ToolUseDto | null;
				runInBackground: boolean;
		  }
		| { status: 'error'; message: string };

	let dispatchInfo = $state<DispatchInfo>({ status: 'idle' });
	let dispatchInfoKey = $state<string | null>(null);

	$effect(() => {
		const identity = selectedIdentity;
		const ds = dataset;
		const sid = dataset.sessionId;
		untrack(() => {
			if (!identity) {
				dispatchInfo = { status: 'idle' };
				dispatchInfoKey = null;
				return;
			}
			if (dispatchInfoKey === identity.key) return;
			dispatchInfoKey = identity.key;
			const dispatchActivity = identity.dispatchActorId
				? dispatchActivityOf(ds, identity.dispatchActorId)
				: undefined;
			const wireId = dispatchActivity?.metadata.wire_tool_use_id ?? null;
			if (wireId === null) {
				dispatchInfo = {
					status: 'loaded',
					prompt: null,
					description: identity.description,
					subagentType: identity.agentName,
					resultRow: null,
					runInBackground: false
				};
				return;
			}
			dispatchInfo = { status: 'loading' };
			loadSessionToolUses(sid)
				.then((rows) => {
					if (dispatchInfoKey !== identity.key) return;
					const row = rows.find((r) => r.tool_id === wireId) ?? null;
					dispatchInfo = {
						status: 'loaded',
						prompt: row ? strField(row.input, 'prompt') : null,
						description: (row ? strField(row.input, 'description') : null) ?? identity.description,
						subagentType: (row ? strField(row.input, 'subagent_type') : null) ?? identity.agentName,
						resultRow: row,
						runInBackground: row ? boolField(row.input, 'run_in_background') : false
					};
				})
				.catch((err: unknown) => {
					if (dispatchInfoKey !== identity.key) return;
					dispatchInfo = {
						status: 'error',
						message: err instanceof Error ? err.message : String(err)
					};
				});
		});
	});

	function strField(input: unknown, key: string): string | null {
		if (!input || typeof input !== 'object') return null;
		const v = (input as Record<string, unknown>)[key];
		return typeof v === 'string' ? v : null;
	}

	function boolField(input: unknown, key: string): boolean {
		if (!input || typeof input !== 'object') return false;
		return (input as Record<string, unknown>)[key] === true;
	}

	const ASYNC_LAUNCH_PREFIX = 'Async agent launched';

	function extractTextBlocks(content: unknown): string | null {
		if (content === undefined || content === null) return null;
		if (typeof content === 'string') return content;
		if (Array.isArray(content)) {
			const texts: string[] = [];
			for (const block of content) {
				if (block && typeof block === 'object') {
					const text = (block as Record<string, unknown>).text;
					if (typeof text === 'string') texts.push(text);
				}
			}
			return texts.length > 0 ? texts.join('\n\n') : null;
		}
		return null;
	}

	function isAsyncLaunchNotice(text: string | null, runInBackground: boolean): boolean {
		if (runInBackground) return true;
		return text !== null && text.trimStart().startsWith(ASYNC_LAUNCH_PREFIX);
	}

	interface UsageInfo {
		status: 'idle' | 'loading' | 'loaded' | 'error';
		input: number;
		output: number;
		cacheCreate: number;
		cacheRead: number;
		lastRequestId: number | null;
		lastAccumulatedText: string | null;
		lastHumanPrompt: string | null;
	}
	const USAGE_IDLE: UsageInfo = {
		status: 'idle',
		input: 0,
		output: 0,
		cacheCreate: 0,
		cacheRead: 0,
		lastRequestId: null,
		lastAccumulatedText: null,
		lastHumanPrompt: null
	};

	let usageInfo = $state<UsageInfo>(USAGE_IDLE);
	let usageKey = $state<string | null>(null);

	$effect(() => {
		const ids = requestIds;
		const active = selectedIdentity ?? selectedGroup ?? (isCoordinator ? selectedActor : undefined);
		untrack(() => {
			if (!active) {
				usageInfo = USAGE_IDLE;
				usageKey = null;
				return;
			}
			const key = [...ids].sort((a, b) => a - b).join(',');
			if (usageKey === key) return;
			usageKey = key;
			if (ids.size === 0) {
				usageInfo = { ...USAGE_IDLE, status: 'loaded' };
				return;
			}
			usageInfo = { ...USAGE_IDLE, status: 'loading' };
			fetchRequestDetails([...ids], () => usageKey !== key)
				.then((details) => {
					if (usageKey !== key) return;
					let input = 0;
					let output = 0;
					let cacheCreate = 0;
					let cacheRead = 0;
					let lastRequestId: number | null = null;
					let lastAccumulatedText: string | null = null;
					let lastHumanPrompt: string | null = null;
					let answerRequestId: number | null = null;
					for (const detail of details) {
						const usage = detail.token_usage;
						if (usage) {
							input += usage.input_tokens;
							output += usage.output_tokens;
							cacheCreate += usage.cache_creation_input_tokens ?? 0;
							cacheRead += usage.cache_read_input_tokens ?? 0;
						}
						if (lastRequestId === null || detail.id > lastRequestId) {
							lastRequestId = detail.id;
							lastAccumulatedText = detail.accumulated_text ?? null;
							lastHumanPrompt = detail.human_prompt ?? null;
						}
						if (detail.stop_reason === 'end_turn' && (answerRequestId === null || detail.id > answerRequestId))
							answerRequestId = detail.id;
					}
					usageInfo = {
						status: 'loaded',
						input,
						output,
						cacheCreate,
						cacheRead,
						lastRequestId,
						lastAccumulatedText,
						lastHumanPrompt
					};
					const answerId = answerRequestId ?? lastRequestId;
					if (lastAccumulatedText === null && answerId !== null) {
						getAccumulatedText(answerId)
							.then((text) => {
								if (usageKey !== key || usageInfo.status !== 'loaded' || !text) return;
								usageInfo = { ...usageInfo, lastRequestId: answerId, lastAccumulatedText: text };
							})
							.catch(() => {});
					}
				})
				.catch(() => {
					if (usageKey !== key) return;
					usageInfo = { ...USAGE_IDLE, status: 'error' };
				});
		});
	});

	let sessionToolRows = $state<ToolUseDto[]>([]);
	let sessionToolRowsSid = $state<string | null>(null);
	$effect(() => {
		const sid = dataset.sessionId;
		untrack(() => {
			if (sessionToolRowsSid === sid) return;
			sessionToolRowsSid = sid;
			loadSessionToolUses(sid)
				.then((rows) => {
					if (sessionToolRowsSid !== sid) return;
					sessionToolRows = rows;
				})
				.catch(() => {
					if (sessionToolRowsSid !== sid) return;
					sessionToolRows = [];
				});
		});
	});
	const toolCallCounts = $derived(toolCallCountsByName(sessionToolRows, requestIds));

	const latestGroupToolRow = $derived.by((): ToolUseDto | null => {
		if (!selectedGroup) return null;
		let best: ToolUseDto | null = null;
		for (const row of sessionToolRows) {
			if (!requestIds.has(row.request_id)) continue;
			if (best === null || row.request_id > best.request_id) best = row;
		}
		return best;
	});

	const liveStatus = $derived.by((): AgentLiveStatus | null => {
		if (!selectedIdentity) return null;
		if (selectedIdentity.pending) return 'not_matched';
		if (dispatchInfo.status === 'loaded' && dispatchInfo.resultRow?.result?.is_error) return 'failed';
		if (span.open) return 'active';
		if (dispatchInfo.status === 'loaded' && dispatchInfo.resultRow?.result) return 'finished';
		return 'idle';
	});

	function clockOf(ms: number): string {
		const d = new Date(ms);
		return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}:${String(d.getSeconds()).padStart(2, '0')}`;
	}
	function statusNoteFor(status: AgentLiveStatus | null): string {
		if (status === null) return '';
		if (status === 'not_matched')
			return 'The dispatch was captured but none of its requests were matched to it.';
		if (span.startMs === null) return '';
		if (status === 'active') return `running ${formatDuration(Date.now() - span.startMs)} · started ${clockOf(span.startMs)}`;
		if (span.endMs === null) return `started ${clockOf(span.startMs)}`;
		return `took ${formatDuration(span.endMs - span.startMs)} · ${clockOf(span.startMs)} → ${clockOf(span.endMs)}`;
	}

	interface UsageRow {
		key: string;
		label: string;
		value: string;
		detail: string;
		valueClass?: string;
		meterPct?: number;
		span?: number;
	}

	const USAGE_GRID_COLUMNS = 3;

	function fillTrailingSpan(rows: UsageRow[]): UsageRow[] {
		if (rows.length === 0) return rows;
		let used = 0;
		for (let i = 0; i < rows.length - 1; i++) used += rows[i]!.span ?? 1;
		const remainder = used % USAGE_GRID_COLUMNS;
		const roomLeft = remainder === 0 ? USAGE_GRID_COLUMNS : USAGE_GRID_COLUMNS - remainder;
		const last = rows[rows.length - 1]!;
		if ((last.span ?? 1) === roomLeft) return rows;
		return [...rows.slice(0, -1), { ...last, span: roomLeft }];
	}

	const cacheSharePct = $derived.by((): number | null => {
		if (usageInfo.status !== 'loaded') return null;
		const denom = usageInfo.input + usageInfo.cacheRead + usageInfo.cacheCreate;
		if (denom <= 0) return null;
		return Math.round(((usageInfo.cacheRead + usageInfo.cacheCreate) / denom) * 100);
	});

	const agentUsageRows = $derived.by((): UsageRow[] => {
		const rows: UsageRow[] = [
			{
				key: 'requests',
				label: 'Requests',
				value: formatCount(requestIds.size),
				detail: span.open ? '1 in flight' : requestIds.size > 0 ? 'all completed' : 'none yet',
				valueClass: 'fdb-v--cyan'
			}
		];
		if (usageInfo.status === 'loaded') {
			const total = usageInfo.input + usageInfo.output;
			rows.push({
				key: 'tokens',
				label: 'Tokens',
				value: formatCount(total),
				detail: `${formatCount(usageInfo.input)} in · ${formatCount(usageInfo.output)} out`,
				valueClass: 'fdb-v--magenta'
			});
			if (cacheSharePct !== null) {
				rows.push({
					key: 'cache',
					label: 'Cache reuse',
					value: `${cacheSharePct}%`,
					detail: `${formatCount(usageInfo.cacheRead)} read · ${formatCount(usageInfo.cacheCreate)} written`,
					meterPct: cacheSharePct
				});
			}
		} else if (usageInfo.status === 'loading') {
			rows.push({ key: 'tokens', label: 'Tokens', value: '', detail: 'loading tokens' });
		}
		if (toolCallCounts.length > 0) {
			const total = toolCallCounts.reduce((n, t) => n + t.count, 0);
			rows.push({
				key: 'tools',
				label: 'Tool calls',
				value: formatCount(total),
				detail: toolCallCounts.map((t) => `${t.count} ${t.name}`).join(' · '),
				span: 2
			});
		}
		if (costUsd !== null) {
			rows.push({
				key: 'cost',
				label: 'Cost',
				value: `$${costUsd.toFixed(2)}`,
				detail: 'at list price'
			});
		}
		return fillTrailingSpan(rows);
	});

	const mainUsageRows = $derived.by((): UsageRow[] => {
		const rows: UsageRow[] = [
			{
				key: 'requests',
				label: 'Requests',
				value: formatCount(requestIds.size),
				detail: 'subagents excluded',
				valueClass: 'fdb-v--cyan'
			}
		];
		if (usageInfo.status === 'loaded') {
			const total = usageInfo.input + usageInfo.output;
			rows.push({
				key: 'tokens',
				label: 'Tokens',
				value: formatCount(total),
				detail: `${formatCount(usageInfo.input)} in · ${formatCount(usageInfo.output)} out`,
				valueClass: 'fdb-v--magenta'
			});
		} else if (usageInfo.status === 'loading') {
			rows.push({ key: 'tokens', label: 'Tokens', value: '', detail: 'loading tokens' });
		}
		if (costUsd !== null)
			rows.push({ key: 'cost', label: 'Cost', value: `$${costUsd.toFixed(2)}`, detail: 'at list price' });
		return fillTrailingSpan(rows);
	});

	const groupUsageRows = $derived.by((): UsageRow[] => {
		let errors = 0;
		let open = 0;
		for (const actorId of relevantActorIds) {
			for (const activity of activitiesOfActor(dataset, actorId)) {
				if (activity.kind !== 'tools') continue;
				if (activity.outcome === 'error') errors += 1;
				if (activity.lifecycle === 'started' && activity.endedAtMs === null) open += 1;
			}
		}
		const calls = selectedGroup ? [...selectedGroup.counts.values()].reduce((a, b) => a + b, 0) : 0;
		const rows: UsageRow[] = [
			{
				key: 'calls',
				label: 'Calls',
				value: formatCount(calls),
				detail: open > 0 ? `${open} in flight` : 'across this session',
				valueClass: 'fdb-v--cyan'
			}
		];
		if (errors > 0)
			rows.push({ key: 'errors', label: 'Errors', value: formatCount(errors), detail: 'non-zero exit or refused' });
		return fillTrailingSpan(rows);
	});

	const dispatchResultText = $derived.by((): string | null => {
		if (dispatchInfo.status !== 'loaded' || !dispatchInfo.resultRow?.result) return null;
		const extracted = extractTextBlocks(dispatchInfo.resultRow.result.content);
		if (isAsyncLaunchNotice(extracted, dispatchInfo.runInBackground)) return null;
		return extracted;
	});
	const finalAnswerText = $derived.by((): string | null => {
		if (dispatchResultText !== null) return dispatchResultText;
		if (liveStatus === 'finished' && usageInfo.status === 'loaded' && usageInfo.lastAccumulatedText) {
			return usageInfo.lastAccumulatedText;
		}
		return null;
	});
	const finalAnswerIsFallback = $derived(dispatchResultText === null && finalAnswerText !== null);

	interface PanelIdentity {
		family: IdentityFamily;
		icon: Component<{ size?: number | string }>;
		kindLabel: string;
		name: string;
		desc: string;
	}

	const panelIdentity = $derived.by((): PanelIdentity | null => {
		if (selectedIdentity) {
			const type = dispatchInfo.status === 'loaded' ? dispatchInfo.subagentType : null;
			return {
				family: 'agent',
				icon: Bot,
				kindLabel: type ? `Subagent · ${type.toUpperCase()}` : 'Subagent',
				name: selectedIdentity.label,
				desc:
					(dispatchInfo.status === 'loaded' ? dispatchInfo.description : null) ??
					selectedIdentity.description ??
					''
			};
		}
		if (isCoordinator && selectedActor) {
			return {
				family: 'session',
				icon: Monitor,
				kindLabel: 'Main session',
				name: title ?? selectedActor.label,
				desc: `Session ${shortSessionId(dataset.sessionId)}`
			};
		}
		if (selectedGroup && selectedActor) {
			const kind: NodeKind = nodeKindOf(selectedGroup.role);
			return {
				family: identityOf(selectedGroup.role),
				icon: ICON_OF[identityOf(selectedGroup.role)] ?? Wrench,
				kindLabel: kind === 'tool' ? 'Local tool' : 'Tool',
				name: selectedGroup.label,
				desc: roleDescription(selectedGroup.role)
			};
		}
		if (selectedActor && selectedRole) {
			const family = identityOf(selectedRole);
			const kind = nodeKindOf(selectedRole);
			return {
				family,
				icon: ICON_OF[family] ?? Globe,
				kindLabel: kind === 'service' ? 'Mike proxy' : kind === 'outside' ? 'Outside service' : 'Local tool',
				name: selectedActor.label,
				desc: roleDescription(selectedRole)
			};
		}
		return null;
	});

	const panelStatus = $derived.by((): AgentLiveStatus | null => {
		if (selectedIdentity) return liveStatus;
		if (isCoordinator) return span.open ? 'active' : 'idle';
		if (selectedActor) {
			for (const actorId of relevantActorIds.length ? relevantActorIds : [selectedActor.id]) {
				for (const activity of activitiesOfActor(dataset, actorId)) {
					if (activity.lifecycle === 'started' && activity.endedAtMs === null) return 'active';
				}
			}
			return 'idle';
		}
		return null;
	});

	const PanelIcon = $derived(panelIdentity?.icon ?? null);

	const panelModel = $derived(models.length > 0 ? models.join(', ') : null);

	const panelStatusNote = $derived.by((): string => {
		if (selectedIdentity) return statusNoteFor(panelStatus);
		if (isCoordinator) {
			if (span.open) return 'a main-thread request is in flight';
			if (usageInfo.status === 'loaded' && usageInfo.lastRequestId !== null)
				return `last request #${usageInfo.lastRequestId}`;
			return '';
		}
		return '';
	});

	const promptChars = $derived(dispatchInfo.status === 'loaded' ? (dispatchInfo.prompt?.length ?? 0) : 0);
	const dispatchActivity = $derived(
		selectedIdentity?.dispatchActorId ? dispatchActivityOf(dataset, selectedIdentity.dispatchActorId) : undefined
	);
	const dispatchRequestId = $derived(dispatchActivity?.source.request_id ?? null);

	interface TextPanel {
		title: string;
		note: string;
		text: string;
	}

	const promptPanel = $derived.by((): TextPanel | null => {
		if (selectedIdentity) {
			if (dispatchInfo.status !== 'loaded' || !dispatchInfo.prompt) return null;
			return {
				title: 'Prompt',
				note: dispatchRequestId
					? `${formatCount(promptChars)} chars · from dispatch #${dispatchRequestId}`
					: `${formatCount(promptChars)} chars`,
				text: dispatchInfo.prompt
			};
		}
		if (isCoordinator) {
			if (usageInfo.status !== 'loaded' || !usageInfo.lastHumanPrompt) return null;
			return {
				title: 'Latest ask',
				note: usageInfo.lastRequestId ? `request #${usageInfo.lastRequestId}` : '',
				text: usageInfo.lastHumanPrompt
			};
		}
		if (selectedGroup) {
			if (!latestGroupToolRow) return null;
			return {
				title: 'Latest command',
				note: `#${latestGroupToolRow.request_id}`,
				text: toolInputText(latestGroupToolRow)
			};
		}
		return null;
	});

	const answerPanel = $derived.by((): TextPanel | null => {
		if (!selectedIdentity || finalAnswerText === null) return null;
		return {
			title: 'Final answer',
			note: finalAnswerIsFallback
				? `${formatCount(finalAnswerText.length)} chars${usageInfo.status === 'loaded' && usageInfo.lastRequestId !== null ? ` · #${usageInfo.lastRequestId}` : ''}`
				: `${formatCount(finalAnswerText.length)} chars${dispatchInfo.status === 'loaded' && dispatchInfo.resultRow ? ` · #${dispatchInfo.resultRow.request_id}` : ''}`,
			text: finalAnswerText
		};
	});

	const openTarget = $derived.by((): { label: string; requestId: number | null } => {
		if (selectedIdentity) {
			return {
				label: dispatchRequestId ? `dispatch #${dispatchRequestId}` : 'dispatch',
				requestId: dispatchRequestId !== null ? Number(dispatchRequestId) : null
			};
		}
		if (isCoordinator) {
			return {
				label: usageInfo.lastRequestId ? `request #${usageInfo.lastRequestId}` : 'latest request',
				requestId: usageInfo.lastRequestId
			};
		}
		if (selectedGroup) {
			return {
				label: latestGroupToolRow ? `call #${latestGroupToolRow.request_id}` : 'latest call',
				requestId: latestGroupToolRow?.request_id ?? null
			};
		}
		if (selectedActor) {
			const activities = activitiesOfActor(dataset, selectedActor.id);
			const withRequest = activities.filter((a) => a.source.request_id !== null);
			const latest = withRequest.at(-1);
			const rid = latest?.source.request_id ? Number(latest.source.request_id) : null;
			return { label: rid !== null ? `request #${rid}` : 'latest', requestId: rid };
		}
		return { label: '', requestId: null };
	});

	function openTargetDrawer(): void {
		if (openTarget.requestId !== null) onOpenDrawer(String(openTarget.requestId), 'request');
	}

	const canFollow = $derived(
		!!onFollow && !!selectedActor && !(selectedIdentity?.pending ?? false)
	);

	let modalOpen = $state(false);
	let modalSource = $state<'prompt' | 'answer'>('prompt');
	function openModal(source: 'prompt' | 'answer'): void {
		modalSource = source;
		modalOpen = true;
	}
	const modalPanel = $derived(modalSource === 'answer' ? answerPanel : promptPanel);
</script>

<section class="fdb" data-slot="flow-detail-band" aria-label="Selected">
	<div class="fdb-head">
		<span class="panel-hd">Selected</span>
		{#if selection !== null}
			<button type="button" class="fdb-btn" onclick={() => onSelect(null)}>Clear</button>
		{/if}
	</div>

	{#if selection === null}
		<div class="fdb-empty">
			<span class="fdb-empty__title">Pick anything on the board</span>
			<span class="fdb-empty__body"
				>An agent shows its prompt, model, status and usage. A tool shows its calls. The requests
				below narrow to whatever you pick.</span
			>
		</div>
	{:else if selectedActivity}
		<div class="fdb-event">
			<span class="fdb-event__kind"
				>{ACTIVITY_KIND_LABEL[selectedActivity.kind] ?? selectedActivity.kind.replaceAll('_', ' ')}</span
			>
			<span class="fdb-name"
				>{#if selectedActivity.kind === 'tools'}{selectedActivity.metadata.tool_name ??
						'Tool call'}{:else if selectedActivity.kind === 'requests'}Request to {selectedActivity
						.metadata.model ?? 'the model'}{:else}{dataset.actors.get(selectedActivity.actor_id)
						?.label ?? selectedActivity.kind.replaceAll('_', ' ')}{/if}</span
			>
			{#if selectedActivity.kind === 'tools' && activityToolRow}
				<span class="fdb-desc">{toolTarget(activityToolRow)}</span>
			{/if}
			<div class="fdb-statusrow">
				<span class="fdb-note"
					>{selectedActivity.occurred_at
						? formatClockAbsolute(selectedActivity.occurred_at)
						: formatClockAbsolute(selectedActivity.recorded_at)}</span
				>
				<span class="fdb-note"
					>{selectedActivity.outcome ?? selectedActivity.lifecycle.replaceAll('_', ' ')}</span
				>
				{#if selectedActivity.occurredAtMs !== null && selectedActivity.endedAtMs !== null}
					<span class="fdb-note"
						>{formatDuration(selectedActivity.endedAtMs - selectedActivity.occurredAtMs)}</span
					>
				{/if}
			</div>
			{#if selectedActivity.kind === 'tools' && activityToolRow}
				<div class="fdb-text">
					<div class="fdb-text__head"><span class="fdb-text__title">Input</span></div>
					<pre class="fdb-event__pre">{toolInputText(activityToolRow)}</pre>
				</div>
				{#if activityToolRow.result}
					<div class="fdb-text">
						<div class="fdb-text__head"
							><span class="fdb-text__title"
								>{activityToolRow.result.is_error ? 'Result (error)' : 'Result'}</span
							></div
						>
						<pre class="fdb-event__pre">{toolResultText(activityToolRow)}</pre>
					</div>
				{/if}
			{/if}
			{#if selectedActivity.source.request_id}
				<div class="fdb-actions">
					<button
						type="button"
						class="fdb-btn"
						onclick={() => onOpenDrawer(selectedActivity!.source.request_id!, 'request')}
						>Open request <ArrowUpRight size={11} aria-hidden="true" /></button
					>
				</div>
			{/if}
		</div>
	{:else if selectedLink}
		<div class="fdb-event">
			<span class="fdb-event__kind">Connection</span>
			<span class="fdb-name"
				>{selectedLink.traceKind.replaceAll('_', ' ')}{#if selectedLink.endpoints.length === 2}
					· {selectedLink.endpoints[0]!.label} → {selectedLink.endpoints[1]!.label}{/if}</span
			>
			<span class="fdb-desc"
				>{selectedLink.activityCount === null
					? 'Activity at the playhead is not tracked for this connection.'
					: `${selectedLink.activityCount} activities lit this route at the playhead.`}</span
			>
		</div>
	{:else if selection.kind === 'source'}
		<div class="fdb-event">
			<span class="fdb-event__kind">Captured row</span>
			<span class="fdb-name">{selection.source.table}:{selection.source.row_id}</span>
			{#if selection.source.request_id}
				<div class="fdb-actions">
					<button
						type="button"
						class="fdb-btn"
						onclick={() => onOpenDrawer(selection.source.request_id!, 'request')}
						>Open request <ArrowUpRight size={11} aria-hidden="true" /></button
					>
				</div>
			{/if}
		</div>
	{:else if !panelIdentity}
		<div class="fdb-empty">
			<span class="fdb-empty__title">Pick anything on the board</span>
			<span class="fdb-empty__body"
				>An agent shows its prompt, model, status and usage. A tool shows its calls. The requests
				below narrow to whatever you pick.</span
			>
		</div>
	{:else}
		<div class="fdb-identity">
			<span
				class="fdb-plate"
				style="background: var(--ident-{panelIdentity.family}-fill); color: var(--ident-{panelIdentity.family}-fg); box-shadow: inset 0 0 0 1px var(--ident-{panelIdentity.family}-line);"
			>
				{#if PanelIcon}<PanelIcon size={20} />{/if}
			</span>
			<div class="fdb-identity__text">
				<span class="fdb-kindline" style="color: var(--ident-{panelIdentity.family}-fg);"
					>{panelIdentity.kindLabel}</span
				>
				<span class="fdb-name">{panelIdentity.name}</span>
				{#if panelIdentity.desc}<span class="fdb-desc">{panelIdentity.desc}</span>{/if}
			</div>
		</div>

		<div class="fdb-statusrow">
			{#if panelStatus}
				<span class="fdb-chip fdb-chip--{panelStatus}" data-augmented-ui="tl-clip br-clip border">
					{#if panelStatus === 'active'}
						<span class="fdb-chip__dots" aria-hidden="true"
							><span data-motion="wait wait-march">·</span><span data-motion="wait wait-march"
								>·</span
							><span data-motion="wait wait-march">·</span></span
						>
					{/if}
					{STATUS_LABEL[panelStatus]}
				</span>
			{/if}
			{#if panelModel}
				<span class="fdb-modelchip">{panelModel}</span>
			{/if}
			{#if panelStatusNote}<span class="fdb-note">{panelStatusNote}</span>{/if}
		</div>

		<div class="fdb-usage">
			<span class="fdb-usage__title"
				>{selectedIdentity
					? panelStatus === 'active'
						? 'Usage so far'
						: 'Usage'
					: isCoordinator
						? 'Main thread only'
						: 'Calls'}</span
			>
			<div class="fdb-usage__grid">
				{#each (selectedIdentity ? agentUsageRows : isCoordinator ? mainUsageRows : groupUsageRows) as row (row.key)}
					<div class="fdb-usage__cell" style={row.span ? `grid-column: span ${row.span};` : undefined}>
						<span class="fdb-usage__k">{row.label}</span>
						<span class="fdb-usage__v {row.valueClass ?? ''}">{row.value}</span>
						<span class="fdb-usage__d">{row.detail}</span>
						{#if row.meterPct !== undefined}
							<span class="fdb-usage__meter"
								><span style="width: {row.meterPct}%;"></span></span
							>
						{/if}
					</div>
				{/each}
			</div>
		</div>

		{#if promptPanel}
			<div class="fdb-text">
				<div class="fdb-text__head">
					<span class="fdb-text__title">{promptPanel.title}</span>
					{#if promptPanel.note}<span class="fdb-text__note">{promptPanel.note}</span>{/if}
					<span class="fdb-spacer"></span>
					<CopyIconButton getText={() => promptPanel!.text} label="Copy {promptPanel.title.toLowerCase()}" />
				</div>
				<button
					type="button"
					class="fdb-text__preview"
					onclick={() => openModal('prompt')}
					aria-label="Open the full {promptPanel.title}"
				>
					<pre class="fdb-text__pre">{promptPanel.text}</pre>
					<span class="fdb-text__readfull"><Maximize2 size={10} aria-hidden="true" />Read full</span>
				</button>
			</div>
		{/if}

		{#if answerPanel}
			<div class="fdb-answer">
				<div class="fdb-text__head">
					<span class="fdb-text__title">{answerPanel.title}</span>
					<span class="fdb-spacer"></span>
					{#if answerPanel.note}<span class="fdb-text__note">{answerPanel.note}</span>{/if}
				</div>
				<button type="button" class="fdb-answer__body" onclick={() => openModal('answer')}
					>{answerPanel.text}</button
				>
			</div>
		{:else if panelStatus === 'active'}
			<div class="fdb-running">
				<span class="fdb-running__dot" aria-hidden="true"></span>
				Still working · its answer lands here when it finishes
			</div>
		{/if}

		<div class="fdb-actions">
			{#if openTarget.requestId !== null}
				<button type="button" class="fdb-btn" onclick={openTargetDrawer}
					>Open {openTarget.label} <ArrowUpRight size={11} aria-hidden="true" /></button
				>
			{/if}
			{#if canFollow}
				<button
					type="button"
					class="fdb-btn"
					aria-pressed={followId === selectedActor!.id}
					onclick={() => onFollow?.(followId === selectedActor!.id ? null : selectedActor!.id)}
					>{followId === selectedActor!.id ? 'Stop following' : 'Follow on timeline'}</button
				>
			{/if}
		</div>
	{/if}
</section>

{#if panelIdentity && modalPanel}
	<FlowPromptModal
		bind:open={modalOpen}
		identityFamily={panelIdentity.family}
		icon={panelIdentity.icon}
		titleLabel={modalPanel.title}
		name={panelIdentity.name}
		heading={panelIdentity.desc || panelIdentity.name}
		note={modalPanel.note}
		text={modalPanel.text}
		footerNote={selectedIdentity
			? 'Sent by the main session when it dispatched this agent'
			: isCoordinator
				? 'The most recent prompt on the main thread'
				: 'The most recent call captured for this tool'}
		openLabel={openTarget.requestId !== null ? openTarget.label : null}
		onOpenDispatch={openTarget.requestId !== null ? openTargetDrawer : null}
	/>
{/if}

<style>
	.fdb {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		min-width: 0;
	}
	.fdb-head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}
	.fdb-btn {
		--edge: 4px;
		display: inline-flex;
		align-items: center;
		gap: 5px;
		margin-left: auto;
		height: 24px;
		padding: 0 8px;
		border: 0;
		background: var(--surface-2);
		color: var(--text-2);
		font-family: var(--font-mono);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		cursor: pointer;
	}
	.fdb-btn:hover {
		background: var(--interactive-hover-surface);
		color: var(--text-strong);
	}
	.fdb-btn:focus-visible {
		outline: 2px solid var(--interactive-focus-ring);
		outline-offset: 2px;
	}

	.fdb-empty {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		padding: var(--space-6, 36px) var(--space-2);
	}
	.fdb-empty__title {
		font-family: var(--font-display);
		font-size: 18px;
		font-weight: 700;
		color: var(--text-strong);
	}
	.fdb-empty__body {
		font-size: 11.5px;
		line-height: 1.6;
		color: var(--text-2);
		max-width: 320px;
	}

	.fdb-event {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		min-width: 0;
	}
	.fdb-event__kind {
		font-family: var(--font-mono);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}
	.fdb-event__pre {
		margin: 0;
		max-height: 220px;
		overflow-y: auto;
		padding: 8px 10px;
		background: var(--surface-code);
		border: var(--hairline) solid var(--border-subtle);
		font-family: var(--font-mono);
		font-size: 11.5px;
		line-height: 1.55;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}

	.fdb-identity {
		display: flex;
		gap: var(--space-3);
		align-items: flex-start;
		min-width: 0;
	}
	.fdb-plate {
		--edge: 6px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		flex: 0 0 auto;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 40px;
		height: 40px;
	}
	.fdb-identity__text {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
		flex: 1 1 auto;
	}
	.fdb-kindline {
		font-family: var(--font-mono);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.fdb-name {
		font-family: var(--font-display);
		font-size: 19px;
		font-weight: 700;
		line-height: 1.15;
		color: var(--text-strong);
		overflow-wrap: anywhere;
	}
	.fdb-desc {
		font-size: 11.5px;
		line-height: 1.45;
		color: var(--text-2);
		overflow-wrap: anywhere;
	}

	.fdb-statusrow {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		align-items: center;
	}
	.fdb-chip {
		--edge: 4px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		display: inline-flex;
		align-items: center;
		gap: 5px;
		height: 22px;
		padding: 0 8px;
		font-family: var(--font-mono);
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.fdb-chip--active {
		background:
			linear-gradient(0deg, var(--status-info-bg), var(--status-info-bg)), var(--surface-card);
		color: var(--status-info-fg);
		--aug-border-bg: var(--status-info-border);
		box-shadow:
			0 0 2px var(--status-info-border),
			0 0 10px var(--status-info-border);
	}
	.fdb-chip--finished {
		background:
			linear-gradient(0deg, var(--status-success-bg), var(--status-success-bg)), var(--surface-card);
		color: var(--status-success-fg);
		--aug-border-bg: var(--status-success-border);
	}
	.fdb-chip--idle {
		background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-card);
		color: var(--text-3);
		--aug-border-bg: var(--border-default);
	}
	.fdb-chip--failed {
		background:
			linear-gradient(0deg, var(--status-danger-bg), var(--status-danger-bg)), var(--surface-card);
		color: var(--status-danger-fg);
		--aug-border-bg: var(--status-danger-border);
	}
	.fdb-chip--not_matched {
		background: linear-gradient(0deg, var(--surface-2), var(--surface-2)), var(--surface-card);
		color: var(--text-3);
		font-style: italic;
		--aug-border-bg: var(--border-default);
	}
	.fdb-chip__dots {
		display: inline-flex;
		gap: 2px;
	}
	.fdb-modelchip {
		display: inline-flex;
		align-items: center;
		height: 22px;
		padding: 0 8px;
		background: var(--surface-2);
		color: var(--text-strong);
		box-shadow: inset 0 0 0 1px var(--border-default);
		font-family: var(--font-mono);
		font-size: 10.5px;
	}
	.fdb-note {
		font-family: var(--font-mono);
		font-size: 10.5px;
		color: var(--text-3);
	}

	.fdb-usage {
		display: flex;
		flex-direction: column;
		gap: 5px;
	}
	.fdb-usage__title {
		font-family: var(--font-mono);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}
	.fdb-usage__grid {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--hairline) var(--hairline);
		background: var(--border-default);
		padding: var(--hairline);
	}
	.fdb-usage__cell {
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
		padding: 6px 8px 7px;
		background: var(--surface-2);
	}
	.fdb-usage__k {
		font-family: var(--font-mono);
		font-size: 9px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}
	.fdb-usage__v {
		font-family: var(--font-display);
		font-size: 16px;
		font-weight: 700;
		line-height: 1.2;
		color: var(--text-strong);
		font-variant-numeric: tabular-nums;
	}
	.fdb-v--cyan {
		color: var(--funk-cyan);
	}
	.fdb-v--magenta {
		color: var(--funk-magenta);
	}
	.fdb-usage__d {
		font-family: var(--font-mono);
		font-size: 9.5px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		color: var(--text-faint);
	}
	.fdb-usage__meter {
		display: block;
		height: 2px;
		background: var(--border-default);
		margin-top: 3px;
	}
	.fdb-usage__meter > span {
		display: block;
		height: 100%;
		background: var(--funk-cyan);
	}

	.fdb-text,
	.fdb-answer {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.fdb-text__head {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}
	.fdb-text__title {
		font-family: var(--font-mono);
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-3);
	}
	.fdb-text__note {
		font-family: var(--font-mono);
		font-size: 10px;
		color: var(--text-faint);
	}
	.fdb-spacer {
		flex: 1 1 auto;
	}
	.fdb-text__preview {
		display: block;
		width: 100%;
		padding: 0;
		border: 0;
		background: var(--border-default);
		cursor: zoom-in;
		text-align: left;
		position: relative;
	}
	.fdb-text__pre {
		margin: 0;
		max-height: 150px;
		overflow: hidden;
		-webkit-mask-image: linear-gradient(#000 62%, transparent);
		mask-image: linear-gradient(#000 62%, transparent);
		padding: 10px 12px;
		background: var(--surface-code);
		font-family: var(--font-mono);
		font-size: 11px;
		line-height: 1.55;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	.fdb-text__readfull {
		position: absolute;
		right: 8px;
		bottom: 8px;
		display: inline-flex;
		align-items: center;
		gap: 5px;
		height: 22px;
		padding: 0 8px;
		background: var(--data-teal-bg);
		color: var(--data-teal-fg);
		font-family: var(--font-mono);
		font-size: 9.5px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		box-shadow: inset 0 0 0 1px var(--data-teal-border);
	}

	.fdb-answer__body {
		display: block;
		width: 100%;
		padding: 10px 12px;
		border: 0;
		border-top: 2px solid var(--status-success-fg);
		background: var(--surface-2);
		font-family: var(--font-mono);
		font-size: 11.5px;
		line-height: 1.6;
		color: var(--text-1);
		white-space: pre-wrap;
		overflow-wrap: anywhere;
		text-align: left;
		cursor: zoom-in;
	}

	.fdb-running {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 9px 12px;
		background: var(--status-info-bg);
		color: var(--status-info-fg);
		font-family: var(--font-mono);
		font-size: 11px;
	}
	.fdb-running__dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: currentColor;
		box-shadow: 0 0 8px currentColor;
	}

	.fdb-actions {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
		margin-top: auto;
	}
</style>
