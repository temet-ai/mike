<script lang="ts">
	/**
	 * A collapsed column's 36px labelled rail (Outline / Your edits / Prompt assistant). Click
	 * anywhere on the rail to expand — no caret, no "Show" label (Interaction Conventions:
	 * "No expand/collapse chrome"). `streaming` renders the same staggered-opacity marching-dots
	 * treatment already used for the assistant's in-flight reply (`WorkbenchAssistantPanel`'s
	 * `wb-msg__phase-dots`), oriented vertically — directional/progressive motion, never a pulse.
	 */
	interface Props {
		label: string;
		onExpand: () => void;
		streaming?: boolean;
	}

	let { label, onExpand, streaming = false }: Props = $props();
</script>

<button
	type="button"
	class="wb-rail"
	aria-label={`Expand ${label}`}
	aria-expanded="false"
	onclick={onExpand}
	data-slot="workbench-rail"
>
	<span class="wb-rail__label">{label}</span>
	{#if streaming}
		<span class="wb-rail__indicator" aria-hidden="true">
			<span></span><span></span><span></span>
		</span>
	{/if}
</button>

<style>
	.wb-rail {
		flex-shrink: 0;
		width: 36px;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: flex-start;
		gap: var(--space-3, 12px);
		padding: var(--space-4, 16px) 0;
		background: var(--surface-sidebar);
		border-left: 1px solid var(--border-default);
		border-right: 1px solid var(--border-default);
		color: var(--text-2);
		cursor: pointer;
	}
	.wb-rail:hover {
		color: var(--text-strong);
	}
	.wb-rail:focus-visible {
		outline: none;
		box-shadow: inset 0 0 0 2px var(--interactive-focus-ring);
	}

	.wb-rail__label {
		writing-mode: vertical-rl;
		transform: rotate(180deg);
		font-family: var(--font-display, 'Syne Variable', sans-serif);
		font-weight: 700;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		white-space: nowrap;
	}

	.wb-rail__indicator {
		display: inline-flex;
		flex-direction: column;
		gap: 3px;
	}
	.wb-rail__indicator span {
		width: 3px;
		height: 3px;
		border-radius: 50%;
		background: var(--funk-magenta);
		animation: wb-rail-dots 1.2s ease-in-out infinite;
	}
	.wb-rail__indicator span:nth-child(2) {
		animation-delay: 0.2s;
	}
	.wb-rail__indicator span:nth-child(3) {
		animation-delay: 0.4s;
	}
	@keyframes wb-rail-dots {
		0%,
		80%,
		100% {
			opacity: 0.25;
		}
		40% {
			opacity: 1;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.wb-rail__indicator span {
			animation: none;
			opacity: 0.6;
		}
	}
</style>
