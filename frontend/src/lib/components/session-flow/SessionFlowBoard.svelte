<script lang="ts">
	import { SvelteMap, SvelteSet } from 'svelte/reactivity';
	import { routeBoard, type BoardRoutes } from '$lib/board/router';
	import {
		TRACE_KINDS,
		type IdentityFamily,
		type PadMark,
		type Point,
		type Route,
		type Trace
	} from '$lib/board/types';
	import BoardColumn from '$lib/components/shared/board/BoardColumn.svelte';
	import BoardSection from '$lib/components/shared/board/BoardSection.svelte';
	import NodeChip from '$lib/components/shared/board/NodeChip.svelte';
	import TraceLayer from '$lib/components/shared/board/TraceLayer.svelte';
	import {
		buildActorKeyMaps,
		buildBoardLayout,
		deriveConnections,
		MIKE_OUT_PAIRS,
		type ActorKeyMaps,
		type LayoutChip
	} from '$lib/session-flow/boardLayout';
	import { bornAtMs } from '$lib/session-flow/born';
	import { buildAgentIdentities } from '$lib/session-flow/grouping';
	import { connectedSet } from '$lib/session-flow/focus';
	import { chipFor, chipForLiveEvent, type MikeChip } from '$lib/session-flow/serviceRoles';
	import { buildTraceModel, type FlowTraceEntry } from '$lib/session-flow/traceModel';
	import type { FlowLiveMark } from '$lib/session-flow/sessionFlowStore.svelte';
	import type {
		FlowDataset,
		FlowLinkEndpoint,
		FlowScene,
		FlowSelection
	} from '$lib/session-flow/types';

	interface Props {
		dataset: FlowDataset;
		scene: FlowScene;
		atMs: number;
		selection: FlowSelection;
		onSelect: (selection: FlowSelection) => void;
		legend: Set<string>;
		collapsed?: boolean;
		stageW?: number;
		onToggleCollapsed?: () => void;
		overlay?: readonly FlowLiveMark[];
		naturalHeight?: number;
	}

	let {
		dataset,
		scene,
		atMs,
		selection,
		onSelect,
		legend,
		collapsed = $bindable(),
		stageW = 1372,
		onToggleCollapsed,
		overlay = [],
		naturalHeight = $bindable(0)
	}: Props = $props();

	const analyzersObserved = $derived(
		overlay.some((mark) => chipForLiveEvent(mark.family) === 'analyzers')
	);

	const agentIdentities = $derived(buildAgentIdentities(dataset));
	const isCollapsed = $derived(collapsed ?? agentIdentities.length >= 10);

	function toggleCollapsed(): void {
		collapsed = !isCollapsed;
		onToggleCollapsed?.();
	}

	const maps = $derived(buildActorKeyMaps(dataset));
	const selectedActorId = $derived(selection?.kind === 'actor' ? selection.id : null);

	function boardKey(chipKey: string | undefined): string | undefined {
		if (chipKey === undefined) return undefined;
		return isCollapsed && /^w\d+$/.test(chipKey) ? 'agents' : chipKey;
	}

	const callerOfToolActivity = $derived.by((): ReadonlyMap<string, string> => {
		const callers = new SvelteMap<string, string>();
		for (const rel of dataset.relationships.values()) {
			if (rel.kind !== 'request_tool' && rel.kind !== 'dispatch_tool') continue;
			if (rel.from_id === null || rel.to_id === null) continue;
			const caller = dataset.activities.get(rel.from_id);
			if (caller) callers.set(rel.to_id, caller.actor_id);
		}
		return callers;
	});

	const connections = $derived(deriveConnections(dataset, atMs, maps));

	const layout = $derived(
		buildBoardLayout(dataset, {
			stageW,
			collapsed: isCollapsed,
			atMs,
			selected: selectedActorId,
			calls: connections.calls,
			events: connections.events,
			mikeOut: MIKE_OUT_PAIRS
		})
	);
	const routes = $derived(routeBoard(layout.input));

	$effect(() => {
		naturalHeight = layout.H;
	});

	const visibleChipKeys = $derived.by((): ReadonlySet<string> => {
		const keys = new SvelteSet<string>();
		for (const chip of layout.chips)
			if (chip.since === null || chip.since <= atMs) keys.add(chip.key);
		return keys;
	});
	const visibleChips = $derived(layout.chips.filter((chip) => visibleChipKeys.has(chip.key)));
	function routeChipKeys(key: string): readonly string[] {
		if (/^w\d+$/.test(key) || ['agents', 'coord', 'peer', 'side', 'browser', 'xmcp'].includes(key))
			return [key];
		if (key.startsWith('tool:')) return key.split(':').slice(1);
		if (key.startsWith('event:')) return [key.split(':')[2] ?? ''];
		if (key === 'dispatch')
			return ['coord', layout.input.collapsed ? 'agents' : (layout.input.selected ?? '')];
		if (key === 'mail') return ['peer'];
		return [];
	}
	function routeVisible(key: string): boolean {
		return routeChipKeys(key).every((chipKey) => visibleChipKeys.has(chipKey));
	}
	const silk = $derived<readonly Route[]>(
		[...Object.values(routes.top), ...Object.values(routes.bottom)].filter((route) =>
			routeVisible(route.key)
		)
	);

	const connectedChipKeys = $derived.by((): ReadonlySet<string> | null => {
		if (selectedActorId === null) return null;
		const focus = connectedSet(dataset, selectedActorId);
		const keys = new SvelteSet<string>();
		for (const id of focus.keys) {
			if (id === 'upstream') keys.add('model');
			else if (id === 'terminal') keys.add('term');
			else {
				const key = maps.keyOfActor.get(id);
				if (key !== undefined) keys.add(key);
			}
		}
		for (const chip of focus.chipKeys) keys.add(chip);
		const ownKey = maps.keyOfActor.get(selectedActorId);
		if (ownKey !== undefined) keys.add(ownKey);
		if (isCollapsed) for (const key of [...keys]) if (/^w\d+$/.test(key)) keys.add('agents');
		return keys;
	});

	function chipMuted(key: string): boolean {
		if (connectedChipKeys === null) return false;
		return !connectedChipKeys.has(key);
	}

	function routeConnected(routeKey: string): boolean {
		if (connectedChipKeys === null) return true;
		if (routeKey === 'dispatch') return connectedChipKeys.has('coord');
		if (routeKey === 'mail') return connectedChipKeys.has('coord') || connectedChipKeys.has('peer');
		return routeKey.split(':').some((segment) => connectedChipKeys!.has(segment));
	}

	const REQUEST_TAPS = ['r_capture', 'filters', 'composer', 'inject', 'compress'] as const;
	const RESPONSE_TAPS = ['s_capture', 'intercept', 'analyzers', 'extract', 'persist'] as const;
	const MAX_PACKET_TRACES = 128;

	interface RawTrace {
		routeKey: string;
		kind: Trace['kind'];
		colorToken?: string;
		reverse: boolean;
		opacity: number;
		packets: boolean;
	}

	function toolRouteKeysByFamily(routes: BoardRoutes): Map<string, string[]> {
		const byFamily = new SvelteMap<string, string[]>();
		for (const key of Object.keys(routes.top)) {
			if (!key.startsWith('tool:')) continue;
			const parts = key.split(':');
			const toolKey = parts[2];
			if (toolKey === undefined) continue;
			(byFamily.get(toolKey) ?? byFamily.set(toolKey, []).get(toolKey)!).push(key);
		}
		return byFamily;
	}

	function activityIdOf(entryId: string): string {
		const i = entryId.indexOf(':');
		return i === -1 ? entryId : entryId.slice(i + 1);
	}

	function serviceChipOf(dataset: FlowDataset, entry: FlowTraceEntry): MikeChip | null {
		const activity = dataset.activities.get(activityIdOf(entry.id));
		if (!activity) return null;
		const actor = dataset.actors.get(activity.actor_id);
		if (!actor) return null;
		return chipFor(actor, activity);
	}

	function pushRaw(map: Map<string, RawTrace>, next: RawTrace, counts?: Map<string, number>): void {
		const dedupeKey = `${next.kind}\u0000${next.routeKey}`;
		if (counts) counts.set(dedupeKey, (counts.get(dedupeKey) ?? 0) + 1);
		const existing = map.get(dedupeKey);
		if (!existing || next.opacity > existing.opacity) map.set(dedupeKey, next);
	}

	function buildRawTraces(
		dataset: FlowDataset,
		entries: readonly FlowTraceEntry[],
		maps: ActorKeyMaps,
		toolFamilies: Map<string, string[]>,
		dispatchTargetKey: string | undefined,
		counts: Map<string, number>
	): RawTrace[] {
		const out = new Map<string, RawTrace>();
		const push = (next: RawTrace): void => pushRaw(out, next, counts);
		let anyCapture = false;
		let anyResponse = false;
		for (const entry of entries) {
			if (entry.kind === 'capture') {
				anyCapture = true;
				const chipKey = boardKey(maps.keyOfActor.get(entry.from));
				if (chipKey !== undefined)
					push({
						routeKey: chipKey,
						kind: 'capture',
						colorToken: `--ident-${entry.identity}-fg`,
						reverse: false,
						opacity: entry.opacity,
						packets: true
					});
			} else if (entry.kind === 'response') {
				anyResponse = true;
				const chipKey = boardKey(maps.keyOfActor.get(entry.to));
				if (chipKey !== undefined)
					push({
						routeKey: chipKey,
						kind: 'response',
						colorToken: '--vendor-anthropic',
						reverse: true,
						opacity: entry.opacity,
						packets: true
					});
			} else if (entry.kind === 'toolcall' || entry.kind === 'return') {
				const toolKey = maps.keyOfActor.get(entry.from);
				const callerActor = callerOfToolActivity.get(activityIdOf(entry.id));
				const callerKey =
					callerActor === undefined ? undefined : boardKey(maps.keyOfActor.get(callerActor));
				const ownRoute =
					toolKey !== undefined && callerKey !== undefined
						? `tool:${callerKey}:${toolKey}`
						: undefined;
				const routeKeys =
					ownRoute !== undefined && ownRoute in routes.top
						? [ownRoute]
						: toolKey === undefined
							? []
							: (toolFamilies.get(toolKey) ?? []);
				for (const routeKey of routeKeys)
					push({
						routeKey,
						kind: entry.kind,
						reverse: entry.kind === 'return',
						opacity: entry.opacity,
						packets: true
					});
			} else if (entry.kind === 'internet') {
				const toolKey = maps.keyOfActor.get(entry.from);
				if (toolKey === 'browser' || toolKey === 'xmcp') {
					push({
						routeKey: toolKey,
						kind: 'internet',
						reverse: false,
						opacity: entry.opacity,
						packets: true
					});
					push({
						routeKey: 'inet_trunk',
						kind: 'internet',
						reverse: false,
						opacity: entry.opacity,
						packets: true
					});
				}
			} else if (entry.kind === 'mikeout') {
				const chip = serviceChipOf(dataset, entry);
				if (chip !== null)
					push({
						routeKey: `mike:${chip}`,
						kind: 'mikeout',
						reverse: false,
						opacity: entry.opacity,
						packets: true
					});
			} else if (entry.kind === 'audit') {
				const chip = serviceChipOf(dataset, entry);
				const targetKey = boardKey(maps.keyOfActor.get(entry.to));
				if (chip !== null && targetKey !== undefined)
					push({
						routeKey: `event:${chip}:${targetKey}`,
						kind: 'audit',
						reverse: false,
						opacity: entry.opacity,
						packets: true
					});
			} else if (entry.kind === 'dispatch') {
				const targetKey = boardKey(maps.keyOfActor.get(entry.to));
				if (targetKey !== undefined && targetKey === dispatchTargetKey)
					push({
						routeKey: 'dispatch',
						kind: 'dispatch',
						reverse: false,
						opacity: entry.opacity,
						packets: false
					});
			} else if (entry.kind === 'message') {
				const routeKey = entry.id === 'message:terminal-ask' ? 'term' : 'mail';
				push({ routeKey, kind: 'message', reverse: false, opacity: entry.opacity, packets: true });
			}
		}
		if (anyCapture) {
			pushRaw(out, {
				routeKey: 'req_channel',
				kind: 'upstream',
				reverse: false,
				opacity: 0.55,
				packets: false
			});
			for (const tap of REQUEST_TAPS)
				pushRaw(out, {
					routeKey: `tap:${tap}`,
					kind: 'upstream',
					reverse: false,
					opacity: 0.55,
					packets: false
				});
		}
		if (anyResponse) {
			pushRaw(out, {
				routeKey: 'res_channel',
				kind: 'response',
				reverse: true,
				opacity: 0.55,
				packets: false
			});
			for (const tap of RESPONSE_TAPS)
				pushRaw(out, {
					routeKey: `tap:${tap}`,
					kind: 'response',
					reverse: true,
					opacity: 0.55,
					packets: false
				});
		}
		return [...out.values()];
	}

	const traceEntries = $derived(buildTraceModel(dataset, atMs, selectedActorId));

	const rawTraces = $derived.by((): { raw: RawTrace[]; counts: ReadonlyMap<string, number> } => {
		const toolFamilies = toolRouteKeysByFamily(routes);
		const counts = new SvelteMap<string, number>();
		const raw = buildRawTraces(
			dataset,
			traceEntries,
			maps,
			toolFamilies,
			layout.input.selected,
			counts
		);
		return { raw, counts };
	});
	function routeActivityCount(kind: Trace['kind'], routeKey: string): number | null {
		return rawTraces.counts.get(`${kind}\u0000${routeKey}`) ?? null;
	}

	const traces = $derived.by((): Trace[] => {
		const { raw } = rawTraces;
		const litFilter = legend;
		const lit: Trace[] = [];
		let packetBudget = MAX_PACKET_TRACES;
		for (const [i, r] of raw.entries()) {
			if (!litFilter.has(r.kind)) continue;
			const route = routes.top[r.routeKey] ?? routes.bottom[r.routeKey];
			if (!route || !routeVisible(r.routeKey)) continue;
			const connected = routeConnected(r.routeKey);
			const opacity = connected ? r.opacity : Math.min(r.opacity, 0.18);
			let packets = r.packets && TRACE_KINDS[r.kind].packets;
			if (packets) {
				if (packetBudget <= 0) packets = false;
				else packetBudget -= 1;
			}
			lit.push({
				key: `${r.kind}:${r.routeKey}:${i}`,
				routeKey: r.routeKey,
				kind: r.kind,
				points: route.points,
				colorToken: r.colorToken,
				hops: routes.hops[r.routeKey],
				reverse: r.reverse,
				opacity,
				packets
			});
		}
		return lit;
	});

	const pads = $derived.by((): PadMark[] => {
		const colorByPoint = new SvelteMap<string, string>();
		for (const t of traces) {
			const layerName = TRACE_KINDS[t.kind].layer;
			const color = t.colorToken ?? TRACE_KINDS[t.kind].colorToken;
			const endpoints: Point[] = [t.points[0]!, t.points[t.points.length - 1]!];
			for (const [x, y] of endpoints)
				colorByPoint.set(`${x.toFixed(1)},${y.toFixed(1)},${layerName}`, color);
		}
		return routes.padAnchors
			.filter((p) => visibleChipKeys.has(p.owner))
			.map((p) => ({
				x: p.x,
				y: p.y,
				layer: p.layer,
				n: p.n,
				vertical: p.vertical,
				colorToken:
					colorByPoint.get(`${p.x.toFixed(1)},${p.y.toFixed(1)},${p.layer}`) ?? '--border-strong'
			}));
	});

	const WHO_FOR_KIND: Readonly<Record<string, IdentityFamily>> = {
		'who-session': 'session',
		'who-agent': 'agent',
		'who-tool': 'tool',
		'who-mike': 'mike',
		'who-anthropic': 'outside'
	};
	function identityOfChip(kind: LayoutChip['kind']): IdentityFamily {
		if (kind === 'session' || kind === 'peer') return 'session';
		if (kind === 'agent' || kind === 'group') return 'agent';
		if (kind === 'tool') return 'tool';
		if (kind === 'service') return 'mike';
		if (kind === 'outside') return 'outside';
		return 'terminal';
	}
	function whoMuted(chip: LayoutChip): boolean {
		const family = identityOfChip(chip.kind);
		for (const [legendKey, target] of Object.entries(WHO_FOR_KIND)) {
			if (target === family) return !legend.has(legendKey);
		}
		return false;
	}
	function errorIdleMuted(chip: LayoutChip): boolean {
		if (chip.state === 'error') return !legend.has('error');
		if (chip.state === 'quiet') return !legend.has('idle');
		return false;
	}

	function handleSelect(chip: LayoutChip): void {
		if (chip.key === 'agents') {
			onSelect(null);
			toggleCollapsed();
			return;
		}
		const actorId = maps.actorOfKey.get(chip.key);
		if (actorId === undefined) return;
		onSelect(selectedActorId === actorId ? null : { kind: 'actor', id: actorId });
	}

	function chipLabel(key: string): string {
		return layout.chips.find((chip) => chip.key === key)?.name ?? key;
	}
	function linkEndpoint(key: string): FlowLinkEndpoint {
		return { key, label: chipLabel(key) };
	}

	function linkEndpointsOf(routeKey: string, kind: Trace['kind']): FlowLinkEndpoint[] {
		if (routeKey.startsWith('tool:')) return routeKey.split(':').slice(1).map(linkEndpoint);
		if (routeKey.startsWith('event:')) {
			const [, chipKey, targetKey] = routeKey.split(':');
			return chipKey && targetKey ? [linkEndpoint(chipKey), linkEndpoint(targetKey)] : [];
		}
		if (routeKey.startsWith('mike:')) {
			const chipKey = routeKey.slice('mike:'.length);
			const pair = MIKE_OUT_PAIRS.find(([from]) => from === chipKey);
			return pair ? [linkEndpoint(chipKey), linkEndpoint(pair[1])] : [linkEndpoint(chipKey)];
		}
		if (routeKey === 'dispatch') {
			const targetKey = layout.input.collapsed ? 'agents' : (layout.input.selected ?? '');
			return targetKey ? [linkEndpoint('coord'), linkEndpoint(targetKey)] : [linkEndpoint('coord')];
		}
		if (routeKey === 'mail') return [linkEndpoint('peer'), linkEndpoint('coord')];
		if (routeKey === 'term') return [linkEndpoint('term'), linkEndpoint('coord')];
		const isSingleChip =
			/^w\d+$/.test(routeKey) || ['agents', 'coord', 'peer', 'side'].includes(routeKey);
		if (isSingleChip && (kind === 'capture' || kind === 'response')) {
			return [linkEndpoint(routeKey), linkEndpoint('model')];
		}
		if (kind === 'internet' && (routeKey === 'browser' || routeKey === 'xmcp')) {
			return [linkEndpoint(routeKey), linkEndpoint('internet')];
		}
		return [];
	}

	const selectedLink = $derived(selection?.kind === 'link' ? selection : null);

	function handleSelectLink(route: { routeKey: string; kind: Trace['kind'] }): void {
		const already =
			selectedLink?.routeKey === route.routeKey && selectedLink?.traceKind === route.kind;
		if (already) {
			onSelect(null);
			return;
		}
		onSelect({
			kind: 'link',
			routeKey: route.routeKey,
			traceKind: route.kind,
			endpoints: linkEndpointsOf(route.routeKey, route.kind),
			activityCount: routeActivityCount(route.kind, route.routeKey)
		});
	}

	let prevAtMs = $state<number | null>(null);
	const enteredKeys = new SvelteSet<string>();
	$effect(() => {
		const current = atMs;
		if (prevAtMs !== null && current > prevAtMs) {
			for (const [i, identity] of agentIdentities.entries()) {
				const key = `w${i}`;
				if (enteredKeys.has(key)) continue;
				const rep = identity.dispatchActorId ?? identity.workerActorIds[0];
				if (rep === undefined) continue;
				const born = bornAtMs(dataset, rep);
				if (born !== null && born > prevAtMs && born <= current) enteredKeys.add(key);
			}
		}
		prevAtMs = current;
	});
</script>

{#if scene.counts.actors === 0}
	<p class="session-flow-board__empty">No flow data yet.</p>
{:else}
	<div class="session-flow-board" style="width: {stageW}px; height: {layout.H}px;">
		<div class="session-flow-board__dots" aria-hidden="true"></div>

		{#each layout.columns as column (column.key)}
			<div class="session-flow-board__column-slot" style="left: {column.x}px; width: {column.w}px;">
				<BoardColumn
					label={column.label}
					count={column.count}
					summary={column.summary}
					state={column.state}
					control={column.control ?? 'none'}
					onToggle={column.key === 'agents' ? toggleCollapsed : undefined}
					height={layout.H}
				/>
			</div>
		{/each}
		{#each layout.sections as section (section.key)}
			{@const owner = layout.columns.find((c) => c.key === section.columnKey)}
			{#if owner}
				<div
					class="session-flow-board__section-slot"
					style="left: {owner.x}px; width: {owner.w}px;"
				>
					<BoardSection
						label={section.label}
						count={section.count}
						summary={section.summary}
						state={section.state}
						y={section.y}
					/>
				</div>
			{/if}
		{/each}

		<TraceLayer
			{traces}
			{pads}
			{silk}
			width={stageW}
			height={layout.H}
			layer="bottom"
			onSelectLink={handleSelectLink}
			selectedRouteKey={selectedLink?.routeKey ?? null}
			selectedKind={selectedLink?.traceKind ?? null}
		/>
		{#each visibleChips as chip (chip.key)}
			<div
				class="session-flow-board__chip-slot"
				style="left: {chip.x}px; top: {chip.y}px; width: {chip.w}px; height: {chip.h}px;"
				data-motion={enteredKeys.has(chip.key) ? 'enter' : undefined}
				data-chip-key={chip.key}
				data-chip-kind={chip.kind}
			>
				<NodeChip
					kind={chip.kind}
					name={chip.name}
					sub={chip.sub}
					state={analyzersObserved && chip.key === 'analyzers' ? 'recent' : chip.state}
					icon={chip.icon}
					muted={chipMuted(chip.key) || whoMuted(chip) || errorIdleMuted(chip)}
					w={chip.w}
					h={chip.h}
					x={0}
					y={0}
					onSelect={() => handleSelect(chip)}
				/>
			</div>
		{/each}
		<TraceLayer
			{traces}
			{pads}
			{silk}
			width={stageW}
			height={layout.H}
			layer="top"
			onSelectLink={handleSelectLink}
			selectedRouteKey={selectedLink?.routeKey ?? null}
			selectedKind={selectedLink?.traceKind ?? null}
		/>
	</div>
{/if}

<style>
	.session-flow-board {
		position: relative;
		background: var(--surface-card);
	}
	.session-flow-board__dots {
		position: absolute;
		inset: 0;
		pointer-events: none;
		background-image: radial-gradient(var(--border-default) 0.6px, transparent 0.7px);
		background-size: 24px 24px;
	}
	.session-flow-board__column-slot,
	.session-flow-board__section-slot {
		position: absolute;
		top: 0;
	}
	.session-flow-board__chip-slot {
		position: absolute;
	}
	.session-flow-board__empty {
		padding: var(--space-4, 16px);
		color: var(--text-2);
		font-family: var(--font-mono);
		font-size: 12px;
	}
</style>
