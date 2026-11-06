<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, within } from 'storybook/test';
	import WriteCall from './WriteCall.svelte';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/WriteCall',
		parameters: {
			docs: {
				description: {
					component:
						'Per-kind body for a captured `Write` tool call: the destination path marked ' +
						'"new file" plus the full file content as it will be written. Content renders ' +
						'verbatim and wraps — never truncated.'
				}
			}
		}
	});

	const writeInput = {
		file_path: '/home/user/temet-proxy/frontend/src/lib/format.ts',
		content:
			'export function formatBytes(n: number): string {\n' +
			'\tif (n < 1024) return `${n} B`;\n' +
			'\tconst kib = n / 1024;\n' +
			'\treturn kib < 1024 ? `${kib.toFixed(1)} KiB` : `${(kib / 1024).toFixed(1)} MiB`;\n' +
			'}\n'
	};
</script>

<Story
	name="Default"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		expect(
			canvas.getByText('/home/user/temet-proxy/frontend/src/lib/format.ts')
		).toBeInTheDocument();
		expect(canvas.getByText('new file')).toBeInTheDocument();

		expect(
			canvas.getByText(/export function formatBytes\(n: number\): string/)
		).toBeInTheDocument();
	}}
>
	<div style="max-width: 640px; padding: 16px; background: var(--surface-bg);">
		<WriteCall input={writeInput} />
	</div>
</Story>
