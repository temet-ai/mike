<script lang="ts">
	import type { SystemBlocksProjection } from '$lib/types';
	import { resolve } from '$app/paths';
	import Tooltip from '$lib/components/shared/Tooltip.svelte';

	interface Props {
		systemBlocks: SystemBlocksProjection | null | undefined;
		requestId: string;
	}

	let { systemBlocks, requestId }: Props = $props();

	type SignalState =
		| { kind: 'none' }
		| { kind: 'composed'; promptSha256: string; label: string }
		| { kind: 'unknown-class' }
		| { kind: 'drifted'; promptSha256: string; count: number };

	const signal = $derived.by((): SignalState => {
		if (!systemBlocks) return { kind: 'none' };

		if (systemBlocks.prompt_sha256 === null) {
			return { kind: 'unknown-class' };
		}

		const drifted = systemBlocks.blocks.filter(
			(b) => b.composition_match !== 'matched' && b.composition_match !== null
		);

		if (drifted.length > 0) {
			return {
				kind: 'drifted',
				promptSha256: systemBlocks.prompt_sha256,
				count: drifted.length
			};
		}

		return {
			kind: 'composed',
			promptSha256: systemBlocks.prompt_sha256,
			label: systemBlocks.prompt_sha256
		};
	});

	const newCompositionHref = $derived(
		`/prompts/new?from=request:${encodeURIComponent(requestId)}` as `/prompts/new?from=request:${string}`
	);
	function diffHref(_promptSha256: string): '/prompts' {
		return '/prompts';
	}
	function classHref(_promptSha256: string): '/prompts' {
		return '/prompts';
	}
</script>

{#if signal.kind === 'composed'}
	<Tooltip text="Composed — system prompt matches a registered composition.">
		{#snippet child({ props })}
			<a
				{...props}
				href={resolve(classHref(signal.promptSha256))}
				class="badge-nx badge-active"
				data-slot="system-prompt-visibility-signal"
				data-state="composed"
			>
				{signal.label}
			</a>
		{/snippet}
	</Tooltip>
{:else if signal.kind === 'unknown-class'}
	<Tooltip
		text="Unknown class — system prompt structure is brand-new. Click to author a composition."
	>
		{#snippet child({ props })}
			<a
				{...props}
				href={resolve(newCompositionHref)}
				class="badge-nx badge-warning"
				data-slot="system-prompt-visibility-signal"
				data-state="unknown-class"
			>
				unknown class
			</a>
		{/snippet}
	</Tooltip>
{:else if signal.kind === 'drifted'}
	<Tooltip
		text={`${signal.count} block${signal.count === 1 ? '' : 's'} drifted from this composition — drift queue picks them up.`}
	>
		{#snippet child({ props })}
			<a
				{...props}
				href={resolve(diffHref(signal.promptSha256))}
				class="badge-nx badge-warning"
				data-slot="system-prompt-visibility-signal"
				data-state="drifted"
			>
				{signal.count} block{signal.count === 1 ? '' : 's'} drifted
			</a>
		{/snippet}
	</Tooltip>
{/if}
