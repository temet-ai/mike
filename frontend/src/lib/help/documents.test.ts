import { describe, expect, it } from 'vitest';
import { DOCUMENT_CONTENT } from './documents';
import { EXTERNAL_DOCS, PROJECT_DOCS, ANTHROPIC_DOCS } from './externalDocs';

describe('packaged user documentation', () => {
	it('uses the existing shared catalogue for local documents and upstream references', () => {
		expect(PROJECT_DOCS.length).toBeGreaterThan(0);
		expect([...PROJECT_DOCS, ...ANTHROPIC_DOCS]).toEqual(EXTERNAL_DOCS);
		for (const document of PROJECT_DOCS) {
			expect(document.internal).toBe(true);
			expect(document.href).toMatch(/^\/help\/docs\/[^/]+$/);
			expect(DOCUMENT_CONTENT[document.href.split('/').at(-1)!]).toContain('## ');
		}
	});

	it('bundles every selected document without private repository dependencies or roadmap links', () => {
		const selected = new Set(PROJECT_DOCS.map((entry) => entry.href.split('/').at(-1)));
		expect(Object.keys(DOCUMENT_CONTENT).every((slug) => selected.has(slug))).toBe(true);
		for (const content of Object.values(DOCUMENT_CONTENT)) {
			expect(content).not.toMatch(/github\.com\/temet-ai|\/blob\/|roadmap/i);
		}
	});

	it('keeps bundled document-to-document links resolvable after deep-link refresh', () => {
		for (const content of Object.values(DOCUMENT_CONTENT)) {
			for (const match of content.matchAll(/\]\(\/help\/docs\/([^/)#]+)\)/g)) {
				expect(DOCUMENT_CONTENT[match[1]!]).toBeTruthy();
			}
		}
	});

	it('documents capture admission, persistence and the local trust boundary', () => {
		expect(DOCUMENT_CONTENT['connections-and-privacy']).toContain('x-claude-code-session-id');
		expect(DOCUMENT_CONTENT['connections-and-privacy']).toContain('forwarded without capture');
		expect(DOCUMENT_CONTENT['connections-and-privacy']).toContain('no authentication');
		expect(DOCUMENT_CONTENT['installed-setup']).toContain('Trial mode is explicitly ephemeral');
		expect(DOCUMENT_CONTENT['recovery-and-maintenance']).toContain('backup');
	});

	it('documents the optional installed maintenance kit with safe recovery boundaries', () => {
		const recovery = DOCUMENT_CONTENT['recovery-and-maintenance'];
		expect(recovery).toContain('mike-maintenance.pyz');
		expect(recovery).toContain('Bash and Python 3.11+');
		expect(recovery).toContain('If you have no kit installed');
		for (const command of [
			'backup',
			'verify',
			'restore',
			'upgrade-check',
			'install',
			'uninstall'
		]) {
			expect(recovery).toContain(`"$MAINTENANCE" ${command}`);
		}
		expect(recovery).toContain('--confirm-stopped-writers');
		expect(recovery).toContain('fresh, empty PostgreSQL 19 target');
		expect(recovery).toContain('there is no purge option');
		expect(recovery).toContain('does not turn an unqualified download into a supported release');
		expect(recovery).not.toContain('does not provide a general backup');
	});

	it('describes matching archive pairs and provider application honestly', () => {
		expect(DOCUMENT_CONTENT['installed-setup']).toContain('matching archive pair');
		expect(DOCUMENT_CONTENT['installed-setup']).not.toContain('from the same archive');
		const configuration = DOCUMENT_CONTENT['configuration-and-import'];
		expect(configuration).toContain('can start or restart that server');
		expect(configuration).toContain('setup remains incomplete');
		expect(configuration).toContain('not authentication with the provider');
	});
});
