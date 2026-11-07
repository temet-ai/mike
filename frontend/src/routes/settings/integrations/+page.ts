import type { PageLoad } from './$types';
import { ApiError } from '$lib/api';
import { USAGE_MODELS_DAYS, getModelUsage, getOpenRouterUsage } from '$lib/api/usage';
import type { ModelUsageRow, OpenRouterUsageDto } from '$lib/api/usage';
import { listSteerers } from '$lib/api/steering';
import type { SteererDto } from '$lib/types';
import { getAnthropicIntegration, getCodexIntegration } from '$lib/api/integrations';
import type { AnthropicIntegrationStatus, CodexIntegrationStatus } from '$lib/api/integrations';
import { getSettings } from '$lib/api/settings';
import { getLlmSpend, llmSpendSinceForDays } from '$lib/api/llmSpend';
import type { LlmSpendResponse } from '$lib/api/llmSpend';

import { LLM_SPEND_DEFAULT_WINDOW_DAYS } from './llmSpendWindow';

export interface IntegrationsPageData {
	openrouterUsage: OpenRouterUsageDto | null;
	openrouterError: string | null;
	openrouterUnavailable: boolean;
	modelUsage: ModelUsageRow[] | null;
	modelUsageError: string | null;
	modelUsageUnavailable: boolean;
	steerers: SteererDto[] | null;
	steerersError: string | null;
	steerersUnavailable: boolean;
	anthropicStatus: AnthropicIntegrationStatus | null;
	anthropicError: string | null;
	anthropicUnavailable: boolean;
	codexStatus: CodexIntegrationStatus | null;
	codexError: string | null;
	codexUnavailable: boolean;
	provider: 'anthropic' | 'codex';
	spend: LlmSpendResponse | null;
	spendError: string | null;
}

function describeFailure(reason: unknown): { message: string; unavailable: boolean } {
	if (reason instanceof ApiError) {
		return { message: reason.message, unavailable: reason.status === 404 };
	}
	return {
		message: reason instanceof Error ? reason.message : String(reason),
		unavailable: false
	};
}

export const load: PageLoad = async (): Promise<IntegrationsPageData> => {
	const [
		orResult,
		modelsResult,
		steerersResult,
		anthropicResult,
		codexResult,
		settingsResult,
		spendResult
	] = await Promise.allSettled([
		getOpenRouterUsage(),
		getModelUsage(USAGE_MODELS_DAYS),
		listSteerers(),
		getAnthropicIntegration(),
		getCodexIntegration(),
		getSettings(),
		getLlmSpend({ since: llmSpendSinceForDays(LLM_SPEND_DEFAULT_WINDOW_DAYS) })
	]);

	const openrouterUsage = orResult.status === 'fulfilled' ? orResult.value : null;
	const orFailure = orResult.status === 'rejected' ? describeFailure(orResult.reason) : null;

	const modelUsage = modelsResult.status === 'fulfilled' ? modelsResult.value.models : null;
	const modelsFailure =
		modelsResult.status === 'rejected' ? describeFailure(modelsResult.reason) : null;

	const steerers = steerersResult.status === 'fulfilled' ? steerersResult.value : null;
	const steerersFailure =
		steerersResult.status === 'rejected' ? describeFailure(steerersResult.reason) : null;

	const anthropicStatus = anthropicResult.status === 'fulfilled' ? anthropicResult.value : null;
	const anthropicFailure =
		anthropicResult.status === 'rejected' ? describeFailure(anthropicResult.reason) : null;
	const codexStatus = codexResult.status === 'fulfilled' ? codexResult.value : null;
	const codexFailure =
		codexResult.status === 'rejected' ? describeFailure(codexResult.reason) : null;
	const providerSetting =
		settingsResult.status === 'fulfilled'
			? settingsResult.value.settings.find((setting) => setting.key === 'assistant.provider')?.value
			: null;

	return {
		openrouterUsage,
		openrouterError: orFailure?.message ?? null,
		openrouterUnavailable: orFailure?.unavailable ?? false,
		modelUsage,
		modelUsageError: modelsFailure?.message ?? null,
		modelUsageUnavailable: modelsFailure?.unavailable ?? false,
		steerers,
		steerersError: steerersFailure?.message ?? null,
		steerersUnavailable: steerersFailure?.unavailable ?? false,
		anthropicStatus,
		anthropicError: anthropicFailure?.message ?? null,
		anthropicUnavailable: anthropicFailure?.unavailable ?? false,
		codexStatus,
		codexError: codexFailure?.message ?? null,
		codexUnavailable: codexFailure?.unavailable ?? false,
		provider: providerSetting === 'codex' ? 'codex' : 'anthropic',
		spend: spendResult.status === 'fulfilled' ? spendResult.value : null,
		spendError:
			spendResult.status === 'rejected' ? describeFailure(spendResult.reason).message : null
	};
};
