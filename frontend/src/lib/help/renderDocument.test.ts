// @vitest-environment jsdom
import DOMPurify from 'dompurify';
import { describe, expect, it } from 'vitest';
import { renderDocument } from './renderDocument';

describe('packaged document sections', () => {
	it('provides stable unique anchors and a task-oriented contents list', () => {
		const content = '## Start mike\n\nUse the installed binary.\n\n## Start mike\n\nAgain.';
		const rendered = renderDocument(content);
		expect(rendered.sections).toEqual([
			{ slug: 'start-mike', label: 'Start mike' },
			{ slug: 'start-mike-2', label: 'Start mike' }
		]);
		expect(rendered.html).toContain('<h2 id="start-mike">');
		expect(renderDocument(content)).toEqual(rendered);
	});

	it('does not interpret HTML or script links in document content', () => {
		const { html } = renderDocument('<script>alert(1)</script>\n\n[bad](javascript:alert(1))');
		expect(html).not.toContain('<script>');
		expect(html).not.toContain('href="javascript:');
	});

	it('preserves document structure and explicit links through sanitization', () => {
		const { html, sections } = renderDocument(
			[
				'## Setup & recovery',
				'',
				'[Maintenance](/help/docs/recovery-and-maintenance#backups)',
				'',
				'| Task | Command |',
				'| --- | --- |',
				'| Inspect | `mike status` |',
				'',
				'```sh',
				'mike status --json',
				'```'
			].join('\n')
		);
		const rendered = document.createElement('div');
		rendered.innerHTML = DOMPurify.sanitize(html);
		expect(sections).toEqual([{ slug: 'setup-recovery', label: 'Setup & recovery' }]);
		expect(rendered.querySelector('#setup-recovery')?.textContent).toBe('Setup & recovery');
		expect(rendered.querySelector('a')?.getAttribute('href')).toBe(
			'/help/docs/recovery-and-maintenance#backups'
		);
		expect(rendered.querySelector('table td code')?.textContent).toBe('mike status');
		expect(rendered.querySelector('pre code.language-sh')?.textContent).toBe(
			'mike status --json\n'
		);
	});

	it('keeps repeated email and unknown-scheme text literal with linkification disabled', () => {
		const content = 'a@b.co\na://\n'.repeat(1000);
		const rendered = document.createElement('div');
		rendered.innerHTML = DOMPurify.sanitize(renderDocument(content).html);
		expect(rendered.querySelector('a')).toBeNull();
		expect(rendered.querySelector('p')?.textContent).toBe(content.trimEnd());
	});

	it('removes executable attributes and URLs while retaining safe markup', () => {
		const rendered = document.createElement('div');
		rendered.innerHTML = DOMPurify.sanitize(
			'<p id="safe">Keep <strong>formatting</strong></p>' +
				'<img src="x" onerror="alert(1)">' +
				'<a href="javascript:alert(1)" onclick="alert(1)">unsafe</a>' +
				'<script>alert(1)</script>'
		);
		expect(rendered.querySelector('#safe strong')?.textContent).toBe('formatting');
		expect(rendered.querySelector('script, [onerror], [onclick]')).toBeNull();
		expect(rendered.querySelector('a')?.hasAttribute('href')).toBe(false);
	});
});
