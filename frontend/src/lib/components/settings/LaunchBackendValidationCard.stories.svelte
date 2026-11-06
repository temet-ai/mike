<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import { userEvent, waitFor, expect } from 'storybook/test';
	import LaunchBackendValidationCard from './LaunchBackendValidationCard.svelte';
	import type { DoctorReport } from '$lib/api/launches';

	const REPORT: DoctorReport = {
		backend: 'herdr',
		passed: false,
		checks: [
			{ name: 'backend chosen', passed: true, detail: 'herdr', remedy: null },
			{ name: 'probe: herdr on PATH', passed: true, detail: '/usr/bin/herdr', remedy: null },
			{
				name: 'probe: server reachable',
				passed: true,
				detail: 'ping answered in 4ms',
				remedy: null
			},
			{ name: 'probe: protocol pinned', passed: true, detail: 'protocol 22', remedy: null },
			{
				name: 'validate: scratch workspace',
				passed: true,
				detail: 'opened, ran, closed cleanly',
				remedy: null
			},
			{
				name: 'sidecar',
				passed: false,
				detail: 'http://127.0.0.1:9987 did not answer',
				remedy:
					'start proxy-forward on http://127.0.0.1:9987 (scripts/start-proxy.sh brings it up) or set MIKE_LAUNCH_PROXY_FORWARD_URL'
			},
			{
				name: 'ca cert',
				passed: true,
				detail: '/home/vimes/.local/share/proxy-forward/ca-cert.pem',
				remedy: null
			},
			{ name: 'claude', passed: true, detail: '/home/vimes/.local/bin/claude', remedy: null },
			{
				name: 'inspect api',
				passed: true,
				detail: 'http://127.0.0.1:9989/api/health',
				remedy: null
			},
			{ name: 'database', passed: true, detail: 'reachable; launches are recorded', remedy: null },
			{ name: 'last validation', passed: true, detail: '412s ago by doctor (passed)', remedy: null }
		]
	};

	function stubDoctor(): void {
		globalThis.fetch = async (input: RequestInfo | URL) => {
			const url = typeof input === 'string' ? input : input.toString();
			if (url.includes('/api/launches/doctor')) {
				await new Promise((r) => setTimeout(r, 30));
				return new Response(JSON.stringify(REPORT), {
					status: 200,
					headers: { 'content-type': 'application/json' }
				});
			}
			return new Response('Not Found', { status: 404 });
		};
	}

	const { Story } = defineMeta({
		title: 'Settings/LaunchBackendValidationCard',
		component: LaunchBackendValidationCard,
		tags: ['autodocs']
	});
</script>

<Story
	name="Default"
	args={{ backend: 'none' }}
	loaders={[
		async () => {
			stubDoctor();
			return {};
		}
	]}
	play={async ({ canvasElement }) => {
		await expect(canvasElement.textContent).toContain('none');
		await expect(canvasElement.textContent).toContain('Not validated this session');
		await expect(canvasElement.textContent).not.toMatch(/running\s+right\s+now/);

		const validateButton = Array.from(canvasElement.querySelectorAll('button')).find(
			(b) => b.textContent?.trim() === 'Validate'
		);
		if (!validateButton) throw new Error('Validate button not found');
		await userEvent.click(validateButton);

		await waitFor(() => {
			if (!canvasElement.textContent?.includes('Validating')) {
				throw new Error('busy state not yet rendered');
			}
		});

		await waitFor(
			() => {
				if (!canvasElement.textContent?.includes('One or more checks failed')) {
					throw new Error('report not yet rendered');
				}
			},
			{ timeout: 3000 }
		);

		await expect(canvasElement.textContent).toContain('sidecar');
		await expect(canvasElement.textContent).toContain('http://127.0.0.1:9987 did not answer');
		await expect(canvasElement.textContent).toContain('start proxy-forward');
		await expect(canvasElement.textContent).toContain('inspect api');

		await expect(canvasElement.textContent).toMatch(/running\s+right\s+now/);
		await expect(canvasElement.textContent).toContain('MIKE_LAUNCH_BACKEND');
	}}
>
	{#snippet template({ backend })}
		<LaunchBackendValidationCard {backend} />
	{/snippet}
</Story>
