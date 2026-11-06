<script lang="ts">
	interface Props {
		open: boolean;
		size?: number;
		class?: string;
	}

	let { open, size = 10, class: klass = '' }: Props = $props();
</script>

<span
	class="dm {klass}"
	class:dm--open={open}
	data-motion="disclose-marker"
	aria-hidden="true"
	style="font-size: {size}px; width: {size}px">▸</span
>

<style>
	.dm {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex: 0 0 auto;
		font-family: var(--font-mono, 'JetBrains Mono Variable', monospace);
		line-height: 1;
		color: var(--text-3);
	}
	.dm--open {
		color: var(--data-teal-fg);
		transform: rotate(90deg);
	}
</style>
