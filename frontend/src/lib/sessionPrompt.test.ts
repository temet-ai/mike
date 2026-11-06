import { describe, it, expect } from 'vitest';
import { operatorPromptText } from './sessionPrompt';

describe('operatorPromptText', () => {
	it('unwraps the title-gen envelope to the operator prompt', () => {
		const raw =
			'<session>\nget from the triage map the next set of tasks to work on.\n</session>\n\n' +
			'Write the title in the predominant language of the session — a stray word or code ' +
			"token in another language doesn't change it.";
		expect(operatorPromptText(raw)).toBe(
			'get from the triage map the next set of tasks to work on.'
		);
	});

	it('keeps the whole inner prompt, however long or multi-line', () => {
		const inner = 'first line\n\nsecond line, with detail\n\nthird line';
		expect(operatorPromptText(`<session>\n${inner}\n</session>\n\nWrite the title.`)).toBe(inner);
	});

	it('unwraps at the envelope tag when the prompt itself quotes one', () => {
		const raw =
			'<session>\ncan you fix the session page that it shows the <session> in the line\n' +
			'</session>\n\nWrite the title in the language the user wrote in.';
		expect(operatorPromptText(raw)).toBe(
			'can you fix the session page that it shows the <session> in the line'
		);
	});

	it('leaves a genuine prompt that mentions a session tag mid-sentence alone', () => {
		const raw = 'the row shows a literal <session> tag — strip it';
		expect(operatorPromptText(raw)).toBe(raw);
	});

	it('leaves an ordinary prompt untouched', () => {
		const raw = 'Implement JWT-based auth middleware for the Express API.';
		expect(operatorPromptText(raw)).toBe(raw);
	});

	it('returns an empty string for a missing prompt', () => {
		expect(operatorPromptText(null)).toBe('');
		expect(operatorPromptText(undefined)).toBe('');
		expect(operatorPromptText('')).toBe('');
	});

	it('returns an empty string for an envelope with no operator text', () => {
		expect(operatorPromptText('<session>\n\n</session>\n\nWrite the title.')).toBe('');
	});
});

describe('operatorPromptText — relay envelopes', () => {
	it('unwraps a teammate-message envelope', () => {
		expect(
			operatorPromptText(
				'<teammate-message teammate_id="team-lead"> Build a project-scoped sessions page </teammate-message>'
			)
		).toBe('Build a project-scoped sessions page');
	});

	it('unwraps an agent-message envelope', () => {
		expect(
			operatorPromptText('<agent-message from="coordinator">Audit the retry path</agent-message>')
		).toBe('Audit the retry path');
	});

	it('unwraps a truncated envelope that never closed', () => {
		expect(operatorPromptText('<teammate-message teammate_id="x"> Read the design spec and')).toBe(
			'Read the design spec and'
		);
	});

	it('leaves a prompt that merely quotes the tag mid-sentence alone', () => {
		const raw = 'the row still shows <teammate-message> in the name, fix it';
		expect(operatorPromptText(raw)).toBe(raw);
	});

	it('still unwraps the title-gen envelope first', () => {
		expect(operatorPromptText('<session>fix the retry backoff</session>\nWrite the title...')).toBe(
			'fix the retry backoff'
		);
	});
});
