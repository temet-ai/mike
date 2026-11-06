<script lang="ts">
	import { Switch } from '$lib/components/ui/switch';
	interface Props {
		name: string;
		label: string;
		description: string;
		enabled?: boolean;
		loading?: boolean;
		disabled?: boolean;
		error?: string | null;
		onToggle?: (value: boolean) => void;
	}

	let {
		name,
		label,
		description,
		enabled = true,
		loading = false,
		disabled = false,
		error = null,
		onToggle
	}: Props = $props();

	let saved = $state(false);
	let wasSaving = false;
	$effect(() => {
		if (loading) saved = false;
		else if (wasSaving) saved = error === null;
		wasSaving = loading;
	});

	const errorId = $derived(`${name}-error`);

	function handleClick(): void {
		if (!loading && !disabled) {
			onToggle?.(!enabled);
		}
	}
</script>

<div
	class="cfg-toggle-row"
	aria-busy={loading}
	class:cfg-toggle-row--loading={loading}
	class:cfg-toggle-row--disabled={disabled && !loading}
	class:cfg-toggle-row--error={error !== null}
	role={error !== null ? 'alert' : undefined}
>
	<Switch
		bind:checked={() => enabled, () => handleClick()}
		aria-label="{name}: {description}"
		aria-describedby={error !== null ? errorId : undefined}
		disabled={loading || disabled}
	/>

	<div class="cfg-toggle-body">
		<div class="cfg-toggle-label-row">
			<span class="cfg-toggle-name" class:cfg-toggle-name--off={!enabled}>{label}</span>
			<span
				class="cfg-toggle-state"
				class:cfg-toggle-state--on={enabled}
				class:cfg-toggle-state--off={!enabled}
			>
				{enabled ? 'ON' : 'OFF'}
			</span>
		</div>

		<p class="cfg-toggle-desc" class:cfg-toggle-desc--off={!enabled}>
			{description}
		</p>

		<p
			class="cfg-save-status"
			class:cfg-save-status--saved={saved}
			role="status"
			aria-live="polite"
		>
			{loading
				? 'Saving changes'
				: error !== null
					? 'Changes not saved'
					: saved
						? 'Changes saved'
						: ''}
		</p>

		{#if error !== null}
			<p class="cfg-toggle-error" id={errorId}>
				{error}
			</p>
		{/if}
	</div>
</div>

<style>
	.cfg-toggle-row {
		display: flex;
		align-items: flex-start;
		gap: 16px;
		padding: 16px 20px;
		border-bottom: 1px solid var(--border-subtle);
		background: var(--surface-card);
		transition: background var(--dur-1) var(--ease-snap);
	}

	.cfg-toggle-row:last-child {
		border-bottom: none;
	}

	.cfg-toggle-row:hover {
		background: var(--surface-2);
	}

	.cfg-toggle-row--loading {
		pointer-events: none;
		cursor: not-allowed;
	}

	.cfg-toggle-row--disabled {
		pointer-events: none;
		cursor: not-allowed;
	}

	.cfg-toggle-row--error {
		background: var(--status-danger-bg);
		box-shadow: inset 2px 0 0 0 var(--status-danger-border);
	}

	.cfg-toggle-body {
		display: flex;
		flex-direction: column;
		gap: 4px;
		flex: 1 1 0;
		min-width: 0;
	}

	.cfg-toggle-label-row {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.cfg-toggle-name {
		font-family: inherit;
		font-size: 13px;
		font-weight: 600;
		color: var(--text-strong);
		flex-shrink: 0;
	}

	.cfg-toggle-name--off {
		color: var(--text-2);
	}

	.cfg-toggle-state {
		font-family: inherit;
		font-size: 10px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		flex-shrink: 0;
	}

	.cfg-toggle-state--on {
		color: var(--status-success-fg);
	}

	.cfg-toggle-state--off {
		color: var(--text-3);
	}

	.cfg-toggle-desc {
		margin: 0;
		font-family: inherit;
		font-size: 12px;
		color: var(--text-2);
		line-height: 1.6;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.cfg-toggle-desc--off {
		color: var(--text-3);
	}

	.cfg-toggle-error {
		margin: 6px 0 0;
		font-family: inherit;
		font-size: 12px;
		color: var(--status-danger-fg);
		line-height: 1.55;
		overflow-wrap: break-word;
		word-break: break-word;
	}

	.cfg-save-status {
		min-height: 1.55em;
		margin: 0;
		color: var(--text-3);
		font-size: var(--text-xs);
		font-family: var(--font-machine);
	}
	.cfg-save-status--saved {
		color: var(--status-success-fg);
	}
</style>
