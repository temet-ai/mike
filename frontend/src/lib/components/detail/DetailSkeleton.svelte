<div class="ds-elev" data-elevation="card">
	<div class="ds-card" data-augmented-ui="tl-clip br-clip border" aria-busy="true">
		<div
			class="ds-skel ds-skel--title"
			data-motion="wait"
			data-augmented-ui="tl-clip br-clip"
		></div>
		<div
			class="ds-skel ds-skel--line ds-skel--wide"
			data-motion="wait"
			data-augmented-ui="tl-clip br-clip"
		></div>
		<div
			class="ds-skel ds-skel--line ds-skel--medium"
			data-motion="wait"
			data-augmented-ui="tl-clip br-clip"
		></div>
	</div>
</div>

<style>
	.ds-card {
		--edge: 8px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		--aug-border-all: 1px;
		--aug-border-bg: var(--border-default);
		background: var(--surface-card);
		box-shadow: inset 0 1px 0 var(--surface-shine);
		padding: calc(var(--edge) + 12px);
	}
	.ds-skel {
		--edge: 3px;
		--aug-tl: var(--edge);
		--aug-br: var(--edge);
		background: linear-gradient(
			90deg,
			var(--surface-2) 0%,
			var(--surface-shine) 50%,
			var(--surface-2) 100%
		);
		background-size: 200% 100%;
	}
	.ds-skel--title {
		height: 16px;
		width: 192px;
	}
	.ds-skel--line {
		height: 12px;
		margin-top: 16px;
	}
	.ds-skel--line.ds-skel--medium {
		margin-top: 8px;
	}
	.ds-skel--wide {
		width: 288px;
	}
	.ds-skel--medium {
		width: 256px;
	}
</style>
