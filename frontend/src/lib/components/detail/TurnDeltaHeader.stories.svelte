<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import TurnDeltaHeader from './TurnDeltaHeader.svelte';
	import 'augmented-ui/augmented-ui.min.css';
	import type { MessageDelta, DeltaShape } from '$lib/components/message-view/messageWalker';

	const { Story } = defineMeta({
		title: 'Components/Detail/TurnDeltaHeader',
		component: TurnDeltaHeader,
		tags: ['autodocs']
	});

	function makeDelta(
		shape: DeltaShape,
		flags: Partial<
			Pick<MessageDelta, 'systemChanged' | 'toolsChanged' | 'modelChanged' | 'paramsChanged'>
		> = {}
	): MessageDelta {
		return {
			messages_added: [],
			commonLen: 0,
			currLen: 0,
			systemChanged: false,
			toolsChanged: false,
			modelChanged: false,
			paramsChanged: false,
			shape,
			...flags
		};
	}

	const noop = () => {};
</script>

<Story name="WithFlagChips">
	{#snippet template()}
		<div class="max-w-2xl p-4">
			<TurnDeltaHeader
				mode="delta"
				delta={makeDelta('tail-append', {
					systemChanged: true,
					toolsChanged: true,
					modelChanged: true,
					paramsChanged: true
				})}
				fullBytes={210_944}
				deltaBytes={4_096}
				loading={false}
				onViewFull={noop}
				onViewDelta={noop}
			/>
		</div>
	{/snippet}
</Story>
