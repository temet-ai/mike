<script lang="ts">
	import { resolve } from '$app/paths';

	interface Props {
		sessionId: string;
		requestId: number;
		label?: string;
	}

	let { sessionId, requestId, label }: Props = $props();

	const href = $derived(
		resolve(
			`sessions/${encodeURIComponent(sessionId)}/requests/${requestId}` as `sessions/${string}/requests/${string}`
		)
	);
</script>

<a class="drl" {href}>{label ?? `request #${requestId}`}</a>

<style>
	.drl {
		font-family: var(--font-mono, monospace);
		font-size: var(--text-xs);
		color: var(--cyan);
		text-decoration: none;
		white-space: nowrap;
	}
	.drl:hover {
		text-decoration: underline;
	}
	.drl:focus-visible {
		outline: 2px solid var(--focus);
		outline-offset: 2px;
		border-radius: var(--r-tol);
	}
</style>
