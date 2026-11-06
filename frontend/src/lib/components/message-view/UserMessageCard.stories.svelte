<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import UserMessageCard from './UserMessageCard.svelte';
	import 'augmented-ui/augmented-ui.min.css';

	const VERY_LONG_BODY = [
		'Here is the full task brief. Read it carefully before starting — do not begin until you have the whole picture.',
		'',
		'## Context',
		'',
		Array.from(
			{ length: 18 },
			(_, i) =>
				`${i + 1}. We need the proxy capture pipeline to preserve byte-identity end to end. The current implementation re-serialises the request body in the middleware layer, which means a body sent with deliberate non-canonical whitespace, a specific field order, or trailing newlines does NOT round-trip. The SHA-256 of the body the upstream receives must equal the SHA-256 of the body the client sent. This is mandatory test #9 and it is currently failing intermittently under concurrent load.`
		).join('\n'),
		'',
		'## Constraints',
		'',
		'- Do not introduce any truncation anywhere in the capture / projection / rendering path. Long content scrolls; it is never clipped.',
		'- The inspect API list rows must contain either full short fields or IDs only — no 200-char preview projections.',
		'- Every Result-returning function needs at least one Err-path test.',
		'',
		'## Acceptance',
		'',
		'When this is done I expect to see the captured request body, the body forwarded upstream, and the body the client sent all hash-identical, with a property test (500 cases in nightly) proving random byte payloads survive the roundtrip unchanged. Paste the captured test output in your terminal turn — an attestation without output is not acceptance.'
	].join('\n');

	const { Story } = defineMeta({
		title: 'Components/MessageView/UserMessageCard',
		component: UserMessageCard,
		tags: ['autodocs']
	});
</script>

<Story
	name="Short"
	args={{ text: 'Given the merge of the 2.8 work into main. where are we with the roadmap?' }}
>
	{#snippet template(args)}
		<div style="padding: 40px; background: var(--surface-bg); max-width: 860px;">
			<UserMessageCard {...args} />
		</div>
	{/snippet}
</Story>

<Story
	name="Multiline"
	args={{
		text: 'Please do the following:\n\n1. Review the API changes\n2. Run the test suite\n3. Post the diff as a PR\n\nContext: I already ran `cargo fmt` locally.'
	}}
>
	{#snippet template(args)}
		<div style="padding: 40px; background: var(--surface-bg); max-width: 860px;">
			<UserMessageCard {...args} />
		</div>
	{/snippet}
</Story>

<Story name="VeryLongBody" args={{ text: VERY_LONG_BODY }}>
	{#snippet template(args)}
		<div style="padding: 40px; background: var(--surface-bg); max-width: 860px;">
			<UserMessageCard {...args} />
		</div>
	{/snippet}
</Story>
