import { spawn } from 'node:child_process';
import { once } from 'node:events';

export async function openChromium(directory, executable = 'chromium') {
  const process = spawn(executable, [
    '--headless', '--no-sandbox', '--no-first-run', '--disable-default-apps',
    '--disable-background-networking', '--disable-component-update', '--disable-sync',
    '--remote-debugging-address=127.0.0.1', '--remote-debugging-port=0',
    `--user-data-dir=${directory}`, 'about:blank',
  ], { stdio: ['ignore', 'ignore', 'pipe'] });
  let diagnostics = '';
  const endpoint = await new Promise((resolve, reject) => {
    const deadline = setTimeout(() => reject(new Error(`Chromium startup timeout: ${diagnostics}`)), 10000);
    process.stderr.on('data', chunk => {
      diagnostics += chunk;
      const match = diagnostics.match(/DevTools listening on (ws:\/\/\S+)/);
      if (match) { clearTimeout(deadline); resolve(match[1]); }
    });
    process.once('error', reject);
    process.once('exit', code => { clearTimeout(deadline); reject(new Error(`Chromium exited ${code}: ${diagnostics}`)); });
  }).catch(error => { process.kill('SIGKILL'); throw error; });
  const socket = new WebSocket(endpoint);
  await once(socket, 'open');
  let sequence = 0;
  const pending = new Map();
  socket.addEventListener('message', event => {
    const message = JSON.parse(event.data);
    const entry = pending.get(message.id);
    if (!entry) return;
    pending.delete(message.id);
    clearTimeout(entry.deadline);
    if (message.error) entry.reject(new Error(JSON.stringify(message.error)));
    else entry.resolve(message.result);
  });
  const call = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const id = ++sequence;
    const deadline = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 15000);
    pending.set(id, { resolve, reject, deadline });
    socket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  return {
    async page(url, expression) {
      const { targetId } = await call('Target.createTarget', { url });
      const { sessionId } = await call('Target.attachToTarget', { targetId, flatten: true });
      try {
        await call('Runtime.enable', {}, sessionId);
        const result = await call('Runtime.evaluate', {
          expression: `(async () => { while (document.readyState !== 'complete' || location.href === 'about:blank') await new Promise(r => setTimeout(r, 20)); return await (${expression}); })()`,
          awaitPromise: true, returnByValue: true,
        }, sessionId);
        if (result.exceptionDetails) throw new Error(JSON.stringify(result.exceptionDetails));
        return result.result.value;
      } finally {
        await call('Target.closeTarget', { targetId });
      }
    },
    async version() { return call('Browser.getVersion'); },
    async close() {
      await call('Browser.close').catch(() => {});
      socket.close();
      if (process.exitCode === null && process.signalCode === null) {
        const timer = setTimeout(() => process.kill('SIGKILL'), 3000);
        await once(process, 'exit').catch(() => {});
        clearTimeout(timer);
      }
    },
  };
}
