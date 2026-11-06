<script lang="ts">
	import { cn } from '$lib/utils';
	import {
		IDENTITY_OF,
		type IdentityFamily,
		type NodeKind,
		type NodeState
	} from '$lib/board/types';
	import { resolveIcon, type IconKey } from './icons';

	interface Props {
		kind: NodeKind;
		name: string;
		sub: string;
		state?: NodeState;
		icon: IconKey;
		muted?: boolean;
		w?: number;
		h?: number;
		x: number;
		y: number;
		onSelect?: () => void;
	}

	let {
		kind,
		name,
		sub,
		state = 'quiet',
		icon,
		muted = false,
		w,
		h,
		x,
		y,
		onSelect
	}: Props = $props();

	interface Shape {
		readonly cuts: 2 | 4;
		readonly edge: string;
		readonly w: number;
		readonly h: number;
	}

	const SHAPE: Readonly<Record<NodeKind, Shape>> = {
		session: { cuts: 4, edge: 'var(--edge-md)', w: 176, h: 56 },
		peer: { cuts: 4, edge: 'var(--edge-md)', w: 144, h: 48 },
		agent: { cuts: 4, edge: 'var(--edge-sm)', w: 150, h: 40 },
		group: { cuts: 2, edge: 'var(--edge-sm)', w: 150, h: 40 },
		tool: { cuts: 2, edge: 'var(--edge-sm)', w: 160, h: 40 },
		service: { cuts: 2, edge: '4px', w: 92, h: 28 },
		outside: { cuts: 4, edge: 'var(--edge-lg)', w: 176, h: 48 },
		terminal: { cuts: 2, edge: 'var(--edge-sm)', w: 184, h: 36 }
	};

	const RING: Partial<Record<NodeState, string>> = {
		busy: 'var(--signal-active)',
		selected: 'var(--signal-active)',
		recent: 'var(--signal-complete)',
		hot: 'var(--signal-complete)',
		error: 'var(--signal-error)',
		response: 'var(--vendor-anthropic)'
	};
	const SUB: Partial<Record<NodeState, string>> = {
		busy: 'var(--signal-active)',
		selected: 'var(--signal-active)',
		recent: 'var(--signal-complete)',
		hot: 'var(--signal-complete)',
		error: 'var(--signal-error)',
		response: 'var(--vendor-anthropic)',
		pending: 'var(--text-3)'
	};
	const halo = (signal: 'cyan' | 'green' | 'red'): string =>
		`drop-shadow(0 0 2px var(--${signal}-border)) drop-shadow(0 0 12px var(--${signal}-dim))`;
	const HALO: Partial<Record<NodeState, string>> = {
		busy: halo('cyan'),
		selected: halo('cyan'),
		recent: halo('green'),
		hot: halo('green'),
		error: halo('red')
	};

	const shape = $derived(SHAPE[kind]);
	const width = $derived(w ?? shape.w);
	const height = $derived(h ?? shape.h);
	const family: IdentityFamily = $derived(IDENTITY_OF[kind]);
	const isService = $derived(kind === 'service');
	const plateWidth = $derived(isService ? 24 : 30);
	const nameSize = $derived(
		isService ? 9 : kind === 'agent' || kind === 'tool' || kind === 'group' ? 10 : 11
	);
	const selected = $derived(state === 'selected');

	const ringToken = $derived(RING[state] ?? `var(--ident-${family}-line)`);
	const subToken = $derived(SUB[state] ?? `var(--ident-${family}-fg)`);
	const haloShadow = $derived(HALO[state]);
	const fillToken = $derived(`var(--ident-${family}-fill)`);
	const plateToken = $derived(`var(--ident-${family}-plate)`);
	const nameColor = $derived(state === 'pending' ? 'var(--text-3)' : 'var(--text-1)');
	const iconEntry = $derived(resolveIcon(icon));
	const iconColor = $derived(icon === 'model' ? 'var(--vendor-anthropic)' : 'var(--text-1)');

	function select(): void {
		onSelect?.();
	}
</script>

<div
	class={cn('node-chip', muted && 'node-chip--muted')}
	data-motion="select"
	style="left: {x}px; top: {y}px; width: {width}px; height: {height}px;{haloShadow
		? ` filter: ${haloShadow};`
		: ''}"
>
	{#if selected}
		<div
			class={shape.cuts === 4 ? 'edge-4-select' : 'edge-select'}
			style="--edge: {shape.edge};"
			aria-hidden="true"
		></div>
	{/if}
	<button
		type="button"
		class={cn('node-chip__shape', shape.cuts === 4 ? 'edge edge-4' : 'edge')}
		style="position: absolute; inset: 0; width: 100%; height: 100%; --edge: {shape.edge}; --edge-pad: 0px; --edge-line: {ringToken}; --edge-surface: {fillToken};"
		aria-label="Inspect {name} · {state}"
		aria-pressed={selected}
		onclick={select}
	>
		<span
			class="node-chip__plate"
			style="width: {plateWidth}px; background: {plateToken}; color: {iconColor};"
			aria-hidden="true"
		>
			{#if iconEntry.component}
				{@const Icon = iconEntry.component}
				<Icon size={14} />
			{:else if iconEntry.svg}
				<!-- eslint-disable-next-line svelte/no-at-html-tags -- icons.ts's RAW entries are hand-authored, not user input. -->
				{@html iconEntry.svg}
			{/if}
		</span>
		<span class="node-chip__text" style="padding-left: {plateWidth + 9}px;">
			<span
				class={isService ? 'machine node-chip__name' : 'node-chip__name'}
				style="font-size: {nameSize}px; color: {nameColor};"
			>
				{name}
			</span>
			<span class="machine node-chip__sub" style="color: {subToken};">{sub}</span>
		</span>
	</button>
</div>

<style>
	.node-chip {
		position: absolute;
	}
	.node-chip--muted {
		opacity: 0.32;
	}
	.node-chip__shape {
		display: block;
		border: 0;
		margin: 0;
		font: inherit;
		text-align: left;
		color: inherit;
		cursor: pointer;
	}
	.node-chip__plate {
		position: absolute;
		left: 1.5px;
		top: 1.5px;
		bottom: 1.5px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
	}
	.node-chip__text {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 1px;
		min-width: 0;
		padding-right: 8px;
		box-sizing: border-box;
	}
	.node-chip__name {
		font-family: var(--font-mono);
		line-height: 1.2;
		overflow-wrap: break-word;
		white-space: normal;
	}
	.node-chip__name.machine {
		white-space: nowrap;
		letter-spacing: 0.04em;
	}
	.node-chip__sub {
		font-size: 9px;
		letter-spacing: 0.06em;
		white-space: nowrap;
	}
</style>
