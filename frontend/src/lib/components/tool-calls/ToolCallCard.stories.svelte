<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent, within } from 'storybook/test';
	import ToolCallCard from './ToolCallCard.svelte';
	import type { ToolUseDto } from '$lib/types';

	const { Story } = defineMeta({
		title: 'Components/ToolCalls/ToolCallCard',
		parameters: {
			docs: {
				description: {
					component:
						'Dispatcher that renders a captured Claude tool call styled like the ' +
						'real tool (per-kind icon + accent + layout), with the full raw input ' +
						'JSON one click away. This story renders a realistic sequence of varied ' +
						'real-shaped calls (bash, read, edit, fetch, mcp, generic), plus a dense ' +
						'timeline-mode row for the flat single-line layout the turn timeline uses.'
				}
			}
		}
	});

	const bashInput = {
		command: 'cargo nextest run -p mike --test proxy_roundtrip',
		description: 'Run the proxy roundtrip integration test'
	};
	const readInput = {
		file_path: '/home/user/temet-proxy/crates/mike/src/store/persistence/messages.rs',
		offset: 120,
		limit: 60
	};
	const editInput = {
		file_path: '/home/user/temet-proxy/frontend/src/lib/api.ts',
		old_string: 'const PAGE_SIZE = 500;',
		new_string: 'const PAGE_SIZE = 1000;'
	};
	const fetchInput = {
		url: 'https://docs.anthropic.com/en/api/messages',
		prompt: 'Extract the streaming SSE event ordering for /v1/messages.'
	};
	const mcpInput = {
		session_id: '7f3a9c2e-1b4d-4e8a-9c0f-2a1b3c4d5e6f',
		limit: 50,
		order: 'asc'
	};
	const genericInput = {
		operation: 'reindex',
		dry_run: true,
		targets: ['sessions', 'requests']
	};

	function buildToolUse(overrides: Partial<ToolUseDto> = {}): ToolUseDto {
		return {
			request_id: 1042,
			session_id: 'storybook-session',
			created_at: '2026-07-02T14:09:04Z',
			tool_id: 'toolu_01AbcDef',
			tool_name: 'Bash',
			input: { command: 'pnpm run check', description: 'Run the full frontend gate' },
			file_path: null,
			ord: 0,
			result: {
				tool_use_id: 'toolu_01AbcDef',
				is_error: true,
				content:
					'Error: Property "durPct" is missing in type ToolCardProps but required.\n  at src/lib/components/tool-calls/ToolCallCard.svelte:42:5\n1 error, 0 warnings',
				content_sha256: 'sha',
				content_bytes: 1800
			},
			...overrides
		};
	}
	const denseToolUse = buildToolUse();
	const onOpenDrawerSpy = fn();
</script>

<Story
	name="Sequence"
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		expect(canvas.getAllByText('Bash').length).toBeGreaterThan(0);
		expect(canvas.getAllByText('Read').length).toBeGreaterThan(0);
		expect(canvas.getAllByText('Edit').length).toBeGreaterThan(0);
		expect(canvas.getByText('WebFetch')).toBeInTheDocument();
		expect(canvas.getByText('mcp__mike__list_requests')).toBeInTheDocument();
		expect(canvas.getByText('SomeUnknownTool')).toBeInTheDocument();

		expect(
			canvas.getByText('cargo nextest run -p mike --test proxy_roundtrip')
		).toBeInTheDocument();

		expect(canvas.getByText('const PAGE_SIZE = 500;')).toBeInTheDocument();
		expect(canvas.getByText('const PAGE_SIZE = 1000;')).toBeInTheDocument();

		expect(canvas.getAllByText('mike').length).toBeGreaterThan(0);
		expect(canvas.getByText('list_requests')).toBeInTheDocument();

		expect(canvas.getByText('read')).toBeInTheDocument();
		expect(canvas.getByText('write')).toBeInTheDocument();
		expect(canvas.getByText('exec')).toBeInTheDocument();

		const opBadges = canvasElement.querySelectorAll('.tool-card__op-badge');
		expect(opBadges.length).toBe(3);

		const rawToggles = canvas.getAllByRole('button', { name: /raw input/i });
		expect(rawToggles.length).toBe(6);

		await rawToggles[0]!.click();
		expect(canvas.getByText(/"command":/)).toBeInTheDocument();

		const denseToggle = canvas.getByRole('button', { name: /expand bash call/i });
		await userEvent.click(denseToggle);
		await expect(canvas.getByText('INPUT')).toBeInTheDocument();
		await expect(canvas.getByText('ERROR')).toBeInTheDocument();
		await expect(canvas.getByText(/durPct/)).toBeInTheDocument();
		await userEvent.click(canvas.getByRole('button', { name: /view request/i }));
		await expect(onOpenDrawerSpy).toHaveBeenCalledWith('request');
	}}
>
	<div
		style="display: flex; flex-direction: column; gap: 12px; max-width: 720px; padding: 16px; background: var(--surface-bg);"
	>
		<ToolCallCard name="Bash" input={bashInput} operation="exec" />
		<ToolCallCard name="Read" input={readInput} operation="read" />
		<ToolCallCard name="Edit" input={editInput} operation="write" />
		<ToolCallCard name="WebFetch" input={fetchInput} />
		<ToolCallCard name="mcp__mike__list_requests" input={mcpInput} />
		<ToolCallCard name="SomeUnknownTool" input={genericInput} />
		<ToolCallCard
			tool={denseToolUse}
			requestNumber={7}
			maxBytes={38000}
			running={false}
			dim={false}
			onOpenDrawer={onOpenDrawerSpy}
		/>
	</div>
</Story>
