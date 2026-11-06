<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, userEvent, within } from 'storybook/test';
	import type { PadMark, Route, Trace, TraceKind } from '$lib/board/types';
	import TraceLayer from './TraceLayer.svelte';
	import NodeChip from './NodeChip.svelte';

	const { Story } = defineMeta({
		title: 'Shared/Board/TraceLayer',
		component: TraceLayer,
		tags: ['autodocs']
	});

	const silk: Route[] = [
		{
			key: 'silk-1',
			points: [
				[170, 60],
				[195, 60],
				[195, 120],
				[220, 120]
			]
		}
	];

	const traces: Trace[] = [
		{
			key: 'toolcall-1',
			routeKey: 'tool:rust-developer:shell',
			kind: 'toolcall',
			points: [
				[170, 60],
				[195, 60],
				[195, 120],
				[220, 120]
			],
			hops: [[195, 90]]
		},
		{
			key: 'dispatch-1',
			routeKey: 'dispatch',
			kind: 'dispatch',
			points: [
				[150, 24],
				[170, 24],
				[170, 60],
				[20, 60]
			]
		}
	];

	const pads: PadMark[] = [
		{ x: 220, y: 120, layer: 'top', colorToken: '--ident-agent-fg', n: 2 },
		{ x: 20, y: 60, layer: 'bottom', colorToken: '--ident-session-fg' }
	];

	const linkSelections: Array<{ routeKey: string; kind: TraceKind }> = [];
</script>

<script lang="ts">
	let selected = $state<{ routeKey: string; kind: TraceKind } | null>(null);
	function onSelectLink(route: { routeKey: string; kind: TraceKind }): void {
		linkSelections.push(route);
		selected = route;
	}
</script>

<Story
	name="TracesAndPads"
	play={async ({ canvasElement }) => {
		const silkGroup = canvasElement.querySelector('.trace-layer__silk');
		const bottomLit = canvasElement.querySelector('.trace-layer__lit--bottom');
		const topLit = canvasElement.querySelector('.trace-layer__lit--top');
		const padsGroup = canvasElement.querySelector('.trace-layer__pads');
		await expect(silkGroup).toBeTruthy();
		await expect(bottomLit).toBeTruthy();
		await expect(topLit).toBeTruthy();
		await expect(padsGroup).toBeTruthy();

		const packets = canvasElement.querySelectorAll('.trace-packet');
		await expect(packets).toHaveLength(1);

		const litPaths = canvasElement.querySelectorAll('.trace-layer__lit--top path');
		await expect(litPaths[0]?.getAttribute('d')).toContain('A 4 4');

		const rect = canvasElement.querySelector('.trace-layer__pads rect');
		await expect(rect?.getAttribute('width')).toBe('9');

		const canvas = within(canvasElement);
		await expect(
			canvasElement.querySelectorAll('.trace-layer__silk path[role="button"]')
		).toHaveLength(0);
		linkSelections.length = 0;
		const toolcallHit = canvas.getByRole('button', { name: /Select toolcall route/ });
		await userEvent.click(toolcallHit);
		await expect(linkSelections).toEqual([
			{ routeKey: 'tool:rust-developer:shell', kind: 'toolcall' }
		]);
		await expect(toolcallHit).toHaveAttribute('aria-pressed', 'true');

		const dispatchHit = canvas.getByRole('button', { name: /Select dispatch route/ });
		dispatchHit.focus();
		await userEvent.keyboard('{Enter}');
		await expect(linkSelections).toEqual([
			{ routeKey: 'tool:rust-developer:shell', kind: 'toolcall' },
			{ routeKey: 'dispatch', kind: 'dispatch' }
		]);
	}}
>
	{#snippet template()}
		<div
			data-design="funk"
			style="position: relative; width: 400px; height: 160px; background: var(--surface-bg);"
		>
			<TraceLayer
				{traces}
				{pads}
				{silk}
				width={400}
				height={160}
				layer="bottom"
				{onSelectLink}
				selectedRouteKey={selected?.routeKey ?? null}
				selectedKind={selected?.kind ?? null}
			/>
			<NodeChip
				kind="session"
				name="main session"
				sub="busy"
				state="busy"
				icon="session"
				x={0}
				y={0}
			/>
			<NodeChip
				kind="agent"
				name="rust-developer"
				sub="busy"
				state="busy"
				icon="agent"
				x={20}
				y={40}
			/>
			<NodeChip
				kind="tool"
				name="Bash"
				sub="busy"
				state="busy"
				icon="tool"
				w={160}
				x={220}
				y={100}
			/>
			<TraceLayer
				{traces}
				{pads}
				{silk}
				width={400}
				height={160}
				layer="top"
				{onSelectLink}
				selectedRouteKey={selected?.routeKey ?? null}
				selectedKind={selected?.kind ?? null}
			/>
		</div>
	{/snippet}
</Story>
