<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { within, expect } from 'storybook/test';
	import SearchMatchRow from './SearchMatchRow.svelte';
	import { buildSearchMatchSnippet } from '$lib/components/__fixtures__/session';

	const SESSION_ID = '11111111-0000-0000-0000-000000000001';
	const MATCH = buildSearchMatchSnippet({ request_id: 4242 });

	const { Story } = defineMeta({
		title: 'Components/Sessions/SearchMatchRow',
		component: SearchMatchRow,
		tags: ['autodocs'],
		parameters: {
			layout: 'padded',
			docs: {
				description: {
					component:
						'One content match from the unified sessions-page search, shared by the raw-body and conversation-chunk lanes. Renders the backend snippet verbatim (before · matched · after) with the match highlighted, a provenance badge, and exposes both "open session" and "open request" navigation.'
				}
			}
		}
	});
</script>

<Story
	name="SearchMatchRow"
	tags={['interactive']}
	args={{ sessionId: SESSION_ID, match: MATCH }}
	play={async ({ canvasElement }) => {
		const canvas = within(canvasElement);

		const mark = canvasElement.querySelector('mark');
		await expect(mark).toBeTruthy();
		await expect(mark?.textContent).toBe(MATCH.matched);

		await expect(canvasElement.textContent).toContain(MATCH.before);
		await expect(canvasElement.textContent).toContain(MATCH.after);

		const sessionLink = canvas.getByRole('link', { name: /session/i });
		const requestLink = canvas.getByRole('link', { name: /request/i });
		await expect(sessionLink.getAttribute('href')).toBe(
			`/sessions/${encodeURIComponent(SESSION_ID)}`
		);
		await expect(requestLink.getAttribute('href')).toBe(
			`/sessions/${encodeURIComponent(SESSION_ID)}/requests/${MATCH.request_id}`
		);
	}}
>
	{#snippet template(args)}
		<ul role="list" style="list-style:none; margin:0; padding:0;">
			<SearchMatchRow sessionId={args.sessionId} match={args.match} />
		</ul>
	{/snippet}
</Story>
