import type { SetupCheck } from '$lib/api/setup';

export type SetupStepId =
	'database' | 'service' | 'claude' | 'features' | 'models' | 'embeddings' | 'terminal';

export const SETUP_STEP_COMMAND: Record<SetupStepId, string> = {
	database: 'mike setup database',
	service: 'mike setup service',
	claude: 'mike setup claude',
	features: 'mike setup features',
	models: 'mike setup models',
	embeddings: 'mike setup embeddings',
	terminal: 'mike setup terminal'
};

const STEP_BY_CHECK: Record<string, SetupStepId> = {
	database_url: 'database',
	database_reachable: 'database',
	postgres_version: 'database',
	database_extensions: 'database',
	database_migrations: 'database',
	background_service: 'service',
	proxy_port: 'service',
	inspect_port: 'service',
	spa_port: 'service',
	spa_built: 'service',
	claude: 'claude',
	mcp_binary: 'claude',
	mcp_registered: 'claude',
	sidecar: 'claude',
	sidecar_ca: 'claude',
	claude_route: 'claude',
	shell_function: 'claude',
	models: 'models',
	embeddings: 'embeddings',
	launch_backend: 'terminal'
};

export function checksForStep(checks: SetupCheck[], step: SetupStepId): SetupCheck[] {
	return checks.filter((check) => STEP_BY_CHECK[check.id] === step);
}

export function generalChecks(checks: SetupCheck[]): SetupCheck[] {
	return checks.filter((check) => STEP_BY_CHECK[check.id] === undefined);
}
