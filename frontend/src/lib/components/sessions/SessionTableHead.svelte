<script lang="ts">
</script>

<div class="ssn-thead" aria-hidden="true">
	<span></span>
	<span>Session</span>
	<span class="ssn-thead-right">Req</span>
	<span>Started</span>
	<span>Last active</span>
	<span>Session id</span>
</div>

<style>
	.ssn-thead {
		display: grid;
		grid-template-columns: 14px minmax(0, 1fr) 70px 110px 110px 190px;
		gap: 14px;
		align-items: center;
		padding: 4px 16px;
		border-bottom: 1px solid var(--border-default);
		background: var(--surface-2);
		font-size: var(--text-micro);
		font-weight: 600;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-faint);
	}
	.ssn-thead-right {
		text-align: right;
	}
</style>
