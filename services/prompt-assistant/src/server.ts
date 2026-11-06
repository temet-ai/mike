import http from 'node:http';
import { query } from '@anthropic-ai/claude-agent-sdk';
import { loadConfig } from './config.js';
import { createRequestListener } from './http/app.js';
import { InspectClient } from './inspectClient.js';
import { resolvePromptAssistantSettings } from './settings.js';
import { sweepOldConversations } from './store/sweep.js';

const config = loadConfig();
const inspectClient = new InspectClient(config.inspectUrl);

const listener = createRequestListener({
	config,
	inspectClient,
	queryFn: query,
	now: () => new Date()
});

const server = http.createServer(listener);

async function runSweep(): Promise<void> {
	try {
		const settings = await resolvePromptAssistantSettings(inspectClient);
		const deleted = await sweepOldConversations(config.stateDir, settings.keep_days, new Date());
		if (deleted.length > 0) {
			console.log(`[prompt-assistant] swept ${deleted.length} conversation(s) older than ${settings.keep_days}d`);
		}
	} catch (err) {
		console.error('[prompt-assistant] sweep failed:', (err as Error).message);
	}
}

server.listen(config.port, config.host, () => {
	console.log(`[prompt-assistant] listening on http://${config.host}:${config.port}`);
	console.log(`[prompt-assistant] inspect API: ${config.inspectUrl}`);
	console.log(`[prompt-assistant] state dir: ${config.stateDir}`);
	void runSweep();
});

const sweepTimer = setInterval(() => void runSweep(), config.sweepIntervalMs);
sweepTimer.unref();

function shutdown(): void {
	clearInterval(sweepTimer);
	server.close(() => process.exit(0));
	setTimeout(() => process.exit(0), 2000).unref();
}
process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
