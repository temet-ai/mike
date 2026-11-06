<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { expect, fn, userEvent } from 'storybook/test';
	import PromptEditor from './PromptEditor.svelte';

	const BUILT_IN_DEFAULT =
		"You used the word '{word}' in your previous response, which is a forbidden quality pattern. Rewrite your response. Start with: 'I used a weasel word, let me fix that:' then give the corrected answer without using '{word}'.";

	const CUSTOM_PROMPT = 'Please rephrase with supporting evidence rather than assertion.';

	const { Story } = defineMeta({
		title: 'Routes/Settings/detail/PromptEditor',
		component: PromptEditor,
		tags: ['autodocs']
	});

	let blankAsksValue = $state(CUSTOM_PROMPT);
	const blankAsksSaveSpy = fn();

	let blankRevertValue = $state(CUSTOM_PROMPT);
	const blankRevertSaveSpy = fn();

	let cmdEnterValue = $state(CUSTOM_PROMPT);
	const cmdEnterSaveSpy = fn();

	let defaultStoryValue = $state(CUSTOM_PROMPT);
	const defaultSaveSpy = fn();
</script>

<Story
	name="BlankSaveAsksToRevert"
	play={async ({ canvas }) => {
		const textarea = canvas.getByLabelText('Correction prompt') as HTMLTextAreaElement;
		await userEvent.clear(textarea);
		await userEvent.click(canvas.getByRole('button', { name: 'Save' }));

		await expect(
			canvas.getByText('A prompt cannot be blank. Revert to the built-in prompt?')
		).toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: 'Keep editing' }));

		await expect(blankAsksSaveSpy).not.toHaveBeenCalled();
		await expect(textarea.value).toBe('');
		await expect(textarea).toHaveFocus();
	}}
>
	{#snippet template()}
		<div style="max-width:640px;">
			<PromptEditor
				value={blankAsksValue}
				defaultValue={BUILT_IN_DEFAULT}
				maxLen={4096}
				label="Correction prompt"
				onSave={(v) => {
					blankAsksSaveSpy(v);
					blankAsksValue = v;
				}}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="BlankSaveRevertRestoresDefault"
	play={async ({ canvas }) => {
		const textarea = canvas.getByLabelText('Correction prompt') as HTMLTextAreaElement;
		await userEvent.clear(textarea);
		await userEvent.click(canvas.getByRole('button', { name: 'Save' }));

		await expect(
			canvas.getByText('A prompt cannot be blank. Revert to the built-in prompt?')
		).toBeInTheDocument();

		await userEvent.click(canvas.getByRole('button', { name: 'Revert to default' }));

		await expect(blankRevertSaveSpy).toHaveBeenCalledTimes(1);
		await expect(blankRevertSaveSpy).toHaveBeenCalledWith(BUILT_IN_DEFAULT);
		await expect(textarea.value).toBe(BUILT_IN_DEFAULT);
	}}
>
	{#snippet template()}
		<div style="max-width:640px;">
			<PromptEditor
				value={blankRevertValue}
				defaultValue={BUILT_IN_DEFAULT}
				maxLen={4096}
				label="Correction prompt"
				onSave={(v) => {
					blankRevertSaveSpy(v);
					blankRevertValue = v;
				}}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="CmdEnterBlankAsksToRevert"
	play={async ({ canvas }) => {
		const textarea = canvas.getByLabelText('Correction prompt') as HTMLTextAreaElement;
		await userEvent.clear(textarea);
		textarea.focus();
		await userEvent.keyboard('{Control>}{Enter}{/Control}');

		await expect(
			canvas.getByText('A prompt cannot be blank. Revert to the built-in prompt?')
		).toBeInTheDocument();
		await expect(cmdEnterSaveSpy).not.toHaveBeenCalled();
	}}
>
	{#snippet template()}
		<div style="max-width:640px;">
			<PromptEditor
				value={cmdEnterValue}
				defaultValue={BUILT_IN_DEFAULT}
				maxLen={4096}
				label="Correction prompt"
				onSave={(v) => {
					cmdEnterSaveSpy(v);
					cmdEnterValue = v;
				}}
			/>
		</div>
	{/snippet}
</Story>

<Story
	name="Default"
	play={async ({ canvas }) => {
		const textarea = canvas.getByLabelText('Correction prompt') as HTMLTextAreaElement;
		await expect(textarea.value).toBe(CUSTOM_PROMPT);
		await expect(
			canvas.getByText('Saved. This prompt replaces the built-in one.')
		).toBeInTheDocument();
		await expect(canvas.getByRole('button', { name: 'Save' })).toBeDisabled();
	}}
>
	{#snippet template()}
		<div style="max-width:640px;">
			<PromptEditor
				value={defaultStoryValue}
				defaultValue={BUILT_IN_DEFAULT}
				maxLen={4096}
				label="Correction prompt"
				onSave={(v) => {
					defaultSaveSpy(v);
					defaultStoryValue = v;
				}}
			/>
		</div>
	{/snippet}
</Story>
