<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, within, expect, waitFor } from 'storybook/test';
	import SessionRowHarness from './__harness__/SessionRowHarness.svelte';
	import {
		buildSessionSummary,
		buildDetailMetrics,
		buildSearchMatchSnippet
	} from '$lib/components/__fixtures__/session';
	import type { SessionMatchInfo } from './sessionRowTypes';

	const STATIC_NOW = Date.parse('2026-06-20T10:00:00Z');

	const SESSION = buildSessionSummary({
		id: '11111111-0000-0000-0000-000000000001',
		project: '/Users/dev/alpha',
		title: 'Implement auth middleware',
		last_status: 'Complete',
		request_count: 34,
		first_seen: '2026-06-20T09:00:00Z',
		last_seen: '2026-06-20T09:45:00Z',
		pending_inbox_count: 2,
		pinned: false,
		originating_operator_prompt:
			'Implement JWT-based auth middleware for the Express API. It should validate tokens, handle expiry, and attach the decoded user to req.user.'
	});

	const DETAIL = buildDetailMetrics();

	const MATCH_INFO: SessionMatchInfo = {
		titleHit: false,
		conversationCount: 5,
		bodyCount: 3,
		conversationTruncated: true,
		conversationSnippets: [
			buildSearchMatchSnippet({
				request_id: 34021,
				before: 'Make sure the auth ',
				matched: 'token',
				after: ' expiry is handled with a clean 401, not a stack trace.',
				provenance: 'prompt',
				provenanceTitle: 'Matched in the operator prompt'
			}),
			buildSearchMatchSnippet({
				request_id: 34030,
				before: 'Wired the bearer-',
				matched: 'token',
				after: ' validation into the Express router ahead of the protected routes.',
				provenance: 'assistant',
				provenanceTitle: 'Matched in the assistant reply'
			})
		],
		bodySnippets: [
			buildSearchMatchSnippet({ request_id: 34021, line: 9, column: 21 }),
			buildSearchMatchSnippet({ request_id: 34021, line: 22, column: 5 }),
			buildSearchMatchSnippet({ request_id: 34030, line: 3, column: 8 })
		]
	};

	const { Story } = defineMeta({
		title: 'Components/Sessions/SessionRow',
		component: SessionRowHarness,
		tags: ['autodocs'],
		parameters: {
			layout: 'padded',
			docs: {
				description: {
					component:
						'One session row: headline, pending-mail badge, hover-revealed detail band, copy-id, pin toggle, and the search-mode "matched in" chip strip with its snippet expander. Shared by the browse-mode project card and the search-mode result cards so the two can never drift.'
				}
			}
		}
	});
</script>

<Story
	name="Populated"
	tags={['data-driven', 'interactive']}
	args={{ session: SESSION, now: STATIC_NOW, detail: DETAIL, matchInfo: MATCH_INFO }}
	play={async ({ canvasElement, args }) => {
		const canvas = within(canvasElement);

		await expect(canvas.getByText('Implement auth middleware')).toBeInTheDocument();

		const rowWrap = canvasElement.querySelector('.ssn-row-wrap') as HTMLElement;
		await expect(rowWrap).toBeTruthy();
		await expect(rowWrap.querySelector('.ssn-mail-badge')?.textContent?.trim()).toBe('2');

		await userEvent.hover(rowWrap);
		await expect(rowWrap.querySelector('.ssn-detail-meta')?.textContent).toContain(
			'claude-opus-4-7'
		);
		await expect(rowWrap.querySelector('.ssn-detail-meta')?.textContent).toContain('12,000');
		await expect(rowWrap.querySelector('.ssn-detail-meta')?.textContent).toContain('$0.42');

		const copyBtn = rowWrap.querySelector('button[aria-label="Copy full session id"]');
		await expect(copyBtn, 'CopyIconButton should render for the row').toBeTruthy();

		const pinBtn = rowWrap.querySelector('button[aria-label="Pin session"]') as HTMLButtonElement;
		await expect(pinBtn).toBeTruthy();
		await userEvent.click(pinBtn);
		await waitFor(() => expect(pinBtn).toHaveAttribute('aria-pressed', 'true'));

		const matchToggle = rowWrap.querySelector('.ssn-match-toggle') as HTMLButtonElement;
		await expect(matchToggle).toHaveAttribute('aria-expanded', 'false');
		await expect(matchToggle.textContent).toContain('5 conversation');
		await expect(matchToggle.textContent).toContain('3 raw body');
		await expect(rowWrap.querySelectorAll('.sbm-row').length).toBe(0);

		await userEvent.click(matchToggle);
		await waitFor(() => expect(matchToggle).toHaveAttribute('aria-expanded', 'true'));
		await expect(rowWrap.querySelectorAll('.sbm-row').length).toBe(5);
		await expect(rowWrap.textContent?.replace(/\s+/g, ' ')).toContain(
			'Showing 2 of 5 conversation matches'
		);
	}}
>
	{#snippet template(args)}
		<SessionRowHarness {...args} />
	{/snippet}
</Story>
