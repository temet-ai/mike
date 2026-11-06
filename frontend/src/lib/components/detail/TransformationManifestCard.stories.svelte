<script module lang="ts">
	import { defineMeta } from '@storybook/addon-svelte-csf';
	import TransformationManifestCard from './TransformationManifestCard.svelte';
	import type { TransformationManifestPayload } from '$lib/types';

	const ORIGINAL_SHA = 'a94a8fe5ccb19ba61c4c0873d391e987982fbbd3a94a8fe5ccb19ba61c4c087';
	const FILE_READ_OUT_SHA = 'c3d5f7a9b1c3e5f7a9b1c3e5f7a9b1c3e5f7a9b1c3e5f7a9b1c3e5f7a9b1c3e5';

	const manifest: TransformationManifestPayload = {
		request_id: 4821,
		has_manifest: true,
		manifest: {
			schema_version: 1,
			original_sha256: ORIGINAL_SHA,
			sent_sha256: FILE_READ_OUT_SHA,
			stages: [
				{
					ordinal: 1,
					stage: 'filter',
					outcome: 'unchanged',
					policy_id: 'default',
					policy_version: 1,
					resolved_parameters: {},
					input_sha256: ORIGINAL_SHA,
					output_sha256: ORIGINAL_SHA,
					targets: [],
					reason_code: 'no_matching_rule',
					detail: null
				},
				{
					ordinal: 2,
					stage: 'compression.file_read',
					outcome: 'applied',
					policy_id: 'file_read',
					policy_version: 2,
					resolved_parameters: { min_bytes: 2048, keep_last_lines: 20 },
					input_sha256: ORIGINAL_SHA,
					output_sha256: FILE_READ_OUT_SHA,
					targets: [
						{
							outcome: 'applied',
							content_kind: 'read_result_text',
							json_path: '/messages/7/content/0/content',
							tool_use_id: 'toolu_01Abc2Def3Ghi4Jkl',
							producer: 'Read',
							input_range: { start: 120, end: 340 },
							output_range: { start: 120, end: 210 },
							original_bytes: 220,
							sent_bytes: 90,
							original_tokens: 55,
							sent_tokens: 22,
							original_content_sha256:
								'd4e6f8a0b2d4e6f8a0b2d4e6f8a0b2d4e6f8a0b2d4e6f8a0b2d4e6f8a0b2d4e6',
							sent_content_sha256:
								'e5f7a9c1d3e5f7a9c1d3e5f7a9c1d3e5f7a9c1d3e5f7a9c1d3e5f7a9c1d3e5f7',
							recovery_ref: null,
							reason_code: 'large_read_result_reduced',
							detail:
								'Reduced a repeated whitespace-heavy log dump to a summary line plus a re-read pointer.'
						},
						{
							outcome: 'skipped',
							content_kind: 'read_result_text',
							json_path: '/messages/9/content/2/content',
							tool_use_id: 'toolu_02Xyz9Wvu8Tsr7Qpo',
							producer: 'Read',
							input_range: { start: 0, end: 0 },
							output_range: { start: 0, end: 0 },
							original_bytes: 40,
							sent_bytes: 40,
							original_tokens: 10,
							sent_tokens: 10,
							original_content_sha256:
								'f6a8b0c2d4f6a8b0c2d4f6a8b0c2d4f6a8b0c2d4f6a8b0c2d4f6a8b0c2d4f6a8',
							sent_content_sha256:
								'f6a8b0c2d4f6a8b0c2d4f6a8b0c2d4f6a8b0c2d4f6a8b0c2d4f6a8b0c2d4f6a8',
							recovery_ref: null,
							reason_code: 'below_size_threshold',
							detail: '40 bytes is below the 2048-byte reduction threshold.'
						}
					],
					reason_code: 'reduced_1_of_2_candidates',
					detail: '1 candidate exceeded the reduction threshold; 1 was already small.'
				},
				{
					ordinal: 3,
					stage: 'composer',
					outcome: 'unchanged',
					policy_id: 'composer',
					policy_version: 1,
					resolved_parameters: {},
					input_sha256: FILE_READ_OUT_SHA,
					output_sha256: FILE_READ_OUT_SHA,
					targets: [],
					reason_code: 'static_prefix_only',
					detail: null
				}
			]
		}
	};

	const { Story } = defineMeta({
		title: 'Components/Detail/TransformationManifestCard',
		component: TransformationManifestCard,
		tags: ['autodocs']
	});
</script>

<Story
	name="MultiStageChain"
	args={{ payload: manifest, loading: false, error: null }}
	play={async ({ canvasElement }) => {
		const { within, expect } = await import('storybook/test');
		const canvas = within(canvasElement);
		await expect(canvas.getByText('filter')).toBeInTheDocument();
		await expect(canvas.getByText('compression.file_read')).toBeInTheDocument();
		await expect(canvas.getByText('composer')).toBeInTheDocument();
		await expect(canvas.getAllByText('Unchanged').length).toBeGreaterThan(0);
		await expect(canvas.getAllByText('Applied').length).toBeGreaterThan(0);
		await expect(canvas.getByText('Skipped')).toBeInTheDocument();
		await expect(canvas.getAllByText(ORIGINAL_SHA).length).toBeGreaterThan(0);
		await expect(canvas.getAllByText(FILE_READ_OUT_SHA).length).toBeGreaterThan(0);
		await expect(canvas.getByText(/220 B.*90 B/)).toBeInTheDocument();
		await expect(canvas.getByText('large_read_result_reduced')).toBeInTheDocument();
	}}
/>
