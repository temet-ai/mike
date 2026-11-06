<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import { captureStorySnapshot } from '$lib/storybook/snapshots';
	import HealthStrip from './HealthStrip.svelte';
	import type { HealthSnapshot } from '$lib/state/liveSystem.svelte';

	function makeHealth(overrides: Partial<HealthSnapshot> = {}): HealthSnapshot {
		return {
			ws_state: 'connected',
			in_flight: 0,
			broadcast_receivers: 2,
			broadcast_backlog: 0,
			db_pool_size: 10,
			db_pool_idle: 6,
			...overrides
		};
	}

	const healthAllOk = makeHealth();

	const healthInFlight = makeHealth({ in_flight: 3 });

	const healthWsConnecting = makeHealth({ ws_state: 'connecting' });

	const healthWsStale = makeHealth({ ws_state: 'stale' });

	const healthWsDisconnected = makeHealth({ ws_state: 'disconnected' });

	const healthPoolSaturated = makeHealth({ db_pool_idle: 0, db_pool_size: 10 });

	const healthPoolNull = makeHealth({ db_pool_size: null, db_pool_idle: null });

	const healthBacklogPresent = makeHealth({
		broadcast_receivers: 4,
		broadcast_backlog: 7
	});

	const healthHighInFlight = makeHealth({ in_flight: 42 });

	const healthCombinedWarnings = makeHealth({
		in_flight: 5,
		db_pool_idle: 0,
		db_pool_size: 10,
		broadcast_receivers: 3,
		broadcast_backlog: 12
	});

	const { Story } = defineMeta({
		title: 'Routes/Home/HealthStrip',
		component: HealthStrip,
		tags: ['autodocs'],
		argTypes: {
			health: { control: false }
		}
	});
</script>

<Story name="AllOk" tags={['data-driven']} args={{ health: healthAllOk }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="InFlight" tags={['data-driven']} args={{ health: healthInFlight }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="WsConnecting" tags={['data-driven']} args={{ health: healthWsConnecting }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="WsStale" tags={['data-driven']} args={{ health: healthWsStale }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="WsDisconnected" tags={['data-driven']} args={{ health: healthWsDisconnected }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="PoolSaturated" tags={['data-driven']} args={{ health: healthPoolSaturated }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="PoolNull" tags={['data-driven']} args={{ health: healthPoolNull }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="BacklogPresent" tags={['data-driven']} args={{ health: healthBacklogPresent }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="HighInFlight" tags={['data-driven']} args={{ health: healthHighInFlight }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip {...args} />
		</div>
	{/snippet}
</Story>

<Story name="CombinedWarnings" tags={['data-driven']} args={{ health: healthCombinedWarnings }}>
	{#snippet template(args)}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="ARIAStructure"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const list = canvasElement.querySelector('[role="list"]');
		await expect(list, 'Container should have role="list"').toBeTruthy();
		const ariaLabel = list?.getAttribute('aria-label');
		await expect(ariaLabel, 'List container should have aria-label').toBeTruthy();

		const items = canvasElement.querySelectorAll('[role="listitem"]');
		await expect(items.length, 'Should have exactly 4 role="listitem" cells').toBe(4);

		const itemsArr = Array.from(items);
		for (let i = 0; i < 4; i++) {
			const label = itemsArr[i]?.querySelector('.lp-health-label');
			await expect(label, `Cell ${i} should have a .lp-health-label span`).toBeTruthy();
		}
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={healthAllOk} />
		</div>
	{/snippet}
</Story>

<Story
	name="WsCellStates"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const items = canvasElement.querySelectorAll('[role="listitem"]');
		await expect(items.length, 'Should have 4 cells').toBe(4);

		const wsCell = Array.from(items)[0];
		await expect(wsCell, 'WS cell should be first listitem').toBeTruthy();

		const valSpan = wsCell?.querySelector('.lp-health-val');
		await expect(valSpan, 'WS cell should have a value span').toBeTruthy();
		await expect(valSpan?.textContent?.trim(), 'WS cell value should show "connected"').toBe(
			'connected'
		);

		await expect(
			valSpan?.classList.contains('lp-health-val--ok'),
			'WS val span must carry lp-health-val--ok when connected'
		).toBe(true);
		await expect(
			valSpan?.classList.contains('lp-health-val--warn'),
			'WS val span must NOT carry lp-health-val--warn when connected'
		).toBe(false);
		await expect(
			valSpan?.classList.contains('lp-health-val--err'),
			'WS val span must NOT carry lp-health-val--err when connected'
		).toBe(false);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={makeHealth({ ws_state: 'connected' })} />
		</div>
	{/snippet}
</Story>

<Story
	name="WsCellWarnStates"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const items = Array.from(canvasElement.querySelectorAll('[role="listitem"]'));
		const wsCell = items[0];
		const valSpan = wsCell?.querySelector('.lp-health-val');

		await expect(valSpan, 'WS cell should have a value span').toBeTruthy();
		await expect(valSpan?.textContent?.trim(), 'WS cell value should show "stale"').toBe('stale');

		await expect(
			valSpan?.classList.contains('lp-health-val--warn'),
			'WS val span must carry lp-health-val--warn when stale'
		).toBe(true);
		await expect(
			valSpan?.classList.contains('lp-health-val--ok'),
			'WS val span must NOT carry lp-health-val--ok when stale'
		).toBe(false);
		await expect(
			valSpan?.classList.contains('lp-health-val--err'),
			'WS val span must NOT carry lp-health-val--err when stale'
		).toBe(false);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={makeHealth({ ws_state: 'stale' })} />
		</div>
	{/snippet}
</Story>

<Story
	name="WsCellConnectingWarnClass"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const items = Array.from(canvasElement.querySelectorAll('[role="listitem"]'));
		const wsCell = items[0];
		const valSpan = wsCell?.querySelector('.lp-health-val');

		await expect(valSpan, 'WS cell should have a value span').toBeTruthy();
		await expect(valSpan?.textContent?.trim(), 'WS cell value should show "connecting"').toBe(
			'connecting'
		);

		await expect(
			valSpan?.classList.contains('lp-health-val--warn'),
			'WS val span must carry lp-health-val--warn when connecting'
		).toBe(true);
		await expect(
			valSpan?.classList.contains('lp-health-val--ok'),
			'WS val span must NOT carry lp-health-val--ok when connecting'
		).toBe(false);
		await expect(
			valSpan?.classList.contains('lp-health-val--err'),
			'WS val span must NOT carry lp-health-val--err when connecting'
		).toBe(false);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={makeHealth({ ws_state: 'connecting' })} />
		</div>
	{/snippet}
</Story>

<Story
	name="WsCellErrorState"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const items = Array.from(canvasElement.querySelectorAll('[role="listitem"]'));
		const wsCell = items[0];
		const valSpan = wsCell?.querySelector('.lp-health-val');

		await expect(valSpan, 'WS cell should have a value span').toBeTruthy();
		await expect(valSpan?.textContent?.trim(), 'WS cell value should show "disconnected"').toBe(
			'disconnected'
		);

		await expect(
			valSpan?.classList.contains('lp-health-val--err'),
			'WS val span must carry lp-health-val--err when disconnected'
		).toBe(true);
		await expect(
			valSpan?.classList.contains('lp-health-val--ok'),
			'WS val span must NOT carry lp-health-val--ok when disconnected'
		).toBe(false);
		await expect(
			valSpan?.classList.contains('lp-health-val--warn'),
			'WS val span must NOT carry lp-health-val--warn when disconnected'
		).toBe(false);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={makeHealth({ ws_state: 'disconnected' })} />
		</div>
	{/snippet}
</Story>

<Story
	name="InFlightAmberClass"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const items = Array.from(canvasElement.querySelectorAll('[role="listitem"]'));

		const inFlightVal = items[1]?.querySelector('.lp-health-val');
		await expect(inFlightVal, 'In-flight cell should have a value span').toBeTruthy();
		await expect(inFlightVal?.textContent?.trim(), 'In-flight cell should show "3"').toBe('3');

		await expect(
			inFlightVal?.classList.contains('lp-health-val--hi'),
			'In-flight span must carry lp-health-val--hi when in_flight > 0'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={makeHealth({ in_flight: 3 })} />
		</div>
	{/snippet}
</Story>

<Story
	name="InFlightZeroNoBacklog"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const items = Array.from(canvasElement.querySelectorAll('[role="listitem"]'));

		const inFlightVal = items[1]?.querySelector('.lp-health-val');
		await expect(
			inFlightVal?.textContent?.trim(),
			'In-flight cell should show "0" when in_flight=0'
		).toBe('0');

		await expect(
			inFlightVal?.classList.contains('lp-health-val--hi'),
			'In-flight span must NOT carry lp-health-val--hi when in_flight=0'
		).toBe(false);

		const backlogNote = items[2]?.querySelector('.lp-health-warn');
		await expect(backlogNote, 'No .lp-health-warn span when broadcast_backlog=0').toBeNull();
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={makeHealth({ in_flight: 0, broadcast_backlog: 0 })} />
		</div>
	{/snippet}
</Story>

<Story
	name="ReceiversBacklogNote"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const items = Array.from(canvasElement.querySelectorAll('[role="listitem"]'));
		const receiverCell = items[2];

		const backlogNote = receiverCell?.querySelector('.lp-health-warn');
		await expect(backlogNote, '.lp-health-warn span must be present when backlog > 0').toBeTruthy();

		await expect(
			backlogNote?.textContent ?? '',
			'Backlog note must contain the count "9"'
		).toContain('9');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={makeHealth({ broadcast_receivers: 2, broadcast_backlog: 9 })} />
		</div>
	{/snippet}
</Story>

<Story
	name="DbPoolNullShowsNA"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const items = Array.from(canvasElement.querySelectorAll('[role="listitem"]'));
		const poolCell = items[3];
		const valSpan = poolCell?.querySelector('.lp-health-val');
		await expect(
			valSpan?.textContent?.trim(),
			'Pool cell should show "n/a" when both values are null'
		).toBe('n/a');

		await expect(
			valSpan?.classList.contains('lp-health-val--warn'),
			'Pool val span must NOT carry lp-health-val--warn when pool is null'
		).toBe(false);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={makeHealth({ db_pool_size: null, db_pool_idle: null })} />
		</div>
	{/snippet}
</Story>

<Story
	name="DbPoolIdleOverSize"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const items = Array.from(canvasElement.querySelectorAll('[role="listitem"]'));
		const poolCell = items[3];
		const valSpan = poolCell?.querySelector('.lp-health-val');
		const text = valSpan?.textContent?.trim() ?? '';

		await expect(text, 'Pool cell text must contain idle count "7"').toContain('7');
		await expect(text, 'Pool cell text must contain size "10"').toContain('10');
		await expect(text, 'Pool cell text must contain "idle" label').toContain('idle');

		await expect(
			valSpan?.classList.contains('lp-health-val--warn'),
			'Pool val span must NOT carry lp-health-val--warn when idle > 0'
		).toBe(false);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={makeHealth({ db_pool_size: 10, db_pool_idle: 7 })} />
		</div>
	{/snippet}
</Story>

<Story
	name="DbPoolSaturatedClass"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const items = Array.from(canvasElement.querySelectorAll('[role="listitem"]'));
		const poolCell = items[3];
		const valSpan = poolCell?.querySelector('.lp-health-val');

		await expect(valSpan, 'Pool cell should have a value span').toBeTruthy();

		await expect(
			valSpan?.classList.contains('lp-health-val--warn'),
			'Pool val span must carry lp-health-val--warn when db_pool_idle=0'
		).toBe(true);

		const text = valSpan?.textContent?.trim() ?? '';
		await expect(text, 'Pool cell must show "0" idle count').toContain('0');
		await expect(text, 'Pool cell must show "10" pool size').toContain('10');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={makeHealth({ db_pool_idle: 0, db_pool_size: 10 })} />
		</div>
	{/snippet}
</Story>

<Story
	name="AllCssStateClasses"
	tags={['data-driven']}
	play={async ({ canvasElement }) => {
		const items = Array.from(canvasElement.querySelectorAll('[role="listitem"]'));
		await expect(items.length, 'Should have exactly 4 role="listitem" cells').toBe(4);

		const wsVal = items[0]?.querySelector('.lp-health-val');
		await expect(wsVal, 'WS cell value span must exist').toBeTruthy();
		await expect(
			wsVal?.classList.contains('lp-health-val--ok'),
			'WS val span must carry lp-health-val--ok when connected'
		).toBe(true);

		const inFlightVal = items[1]?.querySelector('.lp-health-val');
		await expect(
			inFlightVal?.classList.contains('lp-health-val--hi'),
			'In-flight span must carry lp-health-val--hi when in_flight > 0'
		).toBe(true);

		const poolVal = items[3]?.querySelector('.lp-health-val');
		await expect(
			poolVal?.classList.contains('lp-health-val--warn'),
			'Pool val span must carry lp-health-val--warn when db_pool_idle=0'
		).toBe(true);
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={makeHealth({ in_flight: 2, db_pool_idle: 0, db_pool_size: 10 })} />
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotAllOk"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'HealthStrip/AllOk');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={healthAllOk} />
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotInFlight"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'HealthStrip/InFlight');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={healthInFlight} />
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotWsDisconnected"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'HealthStrip/WsDisconnected');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={healthWsDisconnected} />
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotWsConnecting"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'HealthStrip/WsConnecting');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={healthWsConnecting} />
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotWsStale"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'HealthStrip/WsStale');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={healthWsStale} />
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotPoolSaturated"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'HealthStrip/PoolSaturated');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={healthPoolSaturated} />
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotCombinedWarnings"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'HealthStrip/CombinedWarnings');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={healthCombinedWarnings} />
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotPoolNull"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'HealthStrip/PoolNull');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={healthPoolNull} />
		</div>
	{/snippet}
</Story>

<Story
	name="SnapshotBacklogPresent"
	tags={['visual']}
	play={async ({ canvasElement }) => {
		await captureStorySnapshot(canvasElement, 'HealthStrip/BacklogPresent');
	}}
>
	{#snippet template()}
		<div style="padding: 16px; background: var(--surface, #161b22);">
			<HealthStrip health={healthBacklogPresent} />
		</div>
	{/snippet}
</Story>
