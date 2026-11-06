<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect } from 'storybook/test';
	import HeadersTable from './HeadersTable.svelte';

	function buildHeaders(pairs: Array<[string, string]>): Array<[string, string]> {
		return pairs;
	}

	const MASKED = '••••••••';

	function rowCells(canvasElement: HTMLElement): Array<[string, string]> {
		const rows = Array.from(canvasElement.querySelectorAll('tbody tr'));
		return rows.map((tr) => {
			const cells = Array.from(tr.querySelectorAll('td'));
			const name = (cells[0]?.textContent ?? '').replace(/\s+/g, ' ').trim();
			const value = (cells[1]?.textContent ?? '').replace(/\s+/g, ' ').trim();
			return [name, value];
		});
	}

	const TYPICAL_REQUEST_HEADERS = buildHeaders([
		['anthropic-version', '2023-06-01'],
		['anthropic-beta', 'interleaved-thinking-2025-05-14'],
		['x-api-key', 'sk-ant-api03-redacted'],
		['user-agent', 'claude-code/1.2.3 (darwin arm64; node 22.0.0)'],
		['content-type', 'application/json'],
		['content-length', '8192']
	]);

	const { Story } = defineMeta({
		title: 'Components/Detail/HeadersTable',
		component: HeadersTable,
		tags: ['autodocs'],
		argTypes: {
			headers: { control: 'object' },
			emptyLabel: { control: 'text' },
			nullLabel: { control: 'text' }
		}
	});
</script>

<Story
	name="TypicalRequest"
	tags={['interactive']}
	args={{
		headers: TYPICAL_REQUEST_HEADERS,
		emptyLabel: 'No headers',
		nullLabel: 'Not captured'
	}}
	play={async ({ canvasElement }) => {
		const link = canvasElement.querySelector<HTMLAnchorElement>(
			'a[data-slot="tooltip-trigger-link"]'
		);
		expect(link).not.toBeNull();
		expect(link?.textContent).toContain('anthropic-version');
		expect(link?.getAttribute('href')).toBe('/help/glossary#anthropic-headers');
		const cells = rowCells(canvasElement);
		expect(cells[0]).toEqual(['anthropic-version', '2023-06-01']);
		expect(cells[2]).toEqual(['x-api-key', MASKED]);
		expect(canvasElement.textContent).toContain('claude-code/1.2.3');
	}}
>
	{#snippet template(args)}
		<div class="p-4">
			<HeadersTable {...args} />
		</div>
	{/snippet}
</Story>
