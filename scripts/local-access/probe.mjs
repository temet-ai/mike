export async function browserProbe(addresses, fixture, marker) {
  const results = await Promise.all(addresses.map(async base => {
    const socket = new WebSocket(base.replace('http:', 'ws:') + '/api/ws');
    const result = { base, opened: false, messages: [], origin: location.origin };
    socket.addEventListener('message', event => result.messages.push(String(event.data)));
    await new Promise(resolve => {
      const timer = setTimeout(resolve, 2000);
      socket.addEventListener('open', () => { result.opened = true; clearTimeout(timer); resolve(); });
      socket.addEventListener('error', () => { clearTimeout(timer); resolve(); });
    });
    return { socket, result };
  }));
  const trigger = await fetch(fixture + '/capture');
  if (!trigger.ok) throw new Error('Synthetic capture trigger failed: ' + trigger.status);
  await trigger.text();
  await new Promise(resolve => setTimeout(resolve, 300));
  for (const { socket, result } of results) {
    result.receivedCapture = result.messages.some(message => message.includes(marker));
    try {
      const listing = await fetch(result.base + '/api/requests');
      result.readStatus = listing.status;
      const requests = await listing.json();
      const detail = await fetch(result.base + '/api/requests/' + requests[0].id + '/body/request');
      result.readCapture = (await detail.text()).includes(marker);
    } catch {
      result.readStatus = 'blocked';
      result.readCapture = false;
    }
    try {
      const mutation = await fetch(result.base + '/api/settings/features/composition', {
        method: 'PATCH', headers: { 'content-type': 'application/json' }, body: '{"enabled":true}',
      });
      result.mutationStatus = mutation.status;
    } catch {
      result.mutationStatus = 'blocked';
    }
    socket.close();
    delete result.messages;
  }
  return results.map(({ result }) => result);
}

export function probeExpression(probe, args, opaque) {
  const invocation = `(${probe.toString()})(...${JSON.stringify(args)})`;
  if (!opaque) return invocation;
  const child = `<script>(${invocation}).then(result => parent.postMessage({ result }, '*')).catch(error => parent.postMessage({ error: String(error) }, '*'));</script>`;
  return `new Promise((resolve, reject) => {
    const frame = document.createElement('iframe');
    frame.setAttribute('sandbox', 'allow-scripts');
    window.addEventListener('message', event => {
      if (event.source !== frame.contentWindow) return;
      if (event.data.error) reject(new Error(event.data.error));
      else resolve(event.data.result);
    });
    frame.srcdoc = ${JSON.stringify(child)};
    document.body.append(frame);
  })`;
}
