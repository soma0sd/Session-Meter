import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { runInNewContext } from 'node:vm';

// Exercise the exact injected script with session responses that previously caused sign-out.
const source = readFileSync(new URL('../src-tauri/src/codex_helper.rs', import.meta.url), 'utf8');
const script = source.match(/const LOGIN_JS: &str = r#"([\s\S]*?)"#;/)?.[1];
assert.ok(script, 'the browser session script must exist');

async function probe({ sessionOnly = true, status = 200, challenge = false, session = {}, origin = 'https://chatgpt.com', subframe = false }) {
  const messages = [];
  let requests = 0;
  const window = { ipc: { postMessage: value => messages.push(JSON.parse(value)) } };
  window.top = subframe ? {} : window;
  runInNewContext(script.replace('__SM_SESSION_ONLY__', String(sessionOnly)), {
    window, location: { origin }, navigator: { userAgent: 'Browser UA' },
    AbortController, setTimeout: () => 1, clearTimeout() {}, setInterval: () => 1, clearInterval() {},
    fetch: async () => {
      requests++;
      return {
        ok: status >= 200 && status < 300, status,
        headers: { get: name => name === 'cf-mitigated' && challenge ? 'challenge' : null },
        json: async () => session,
      };
    },
  });
  await new Promise(resolve => setImmediate(resolve));
  return { messages, requests };
}

for (const accessKey of ['accessToken', 'access_token']) {
  const session = { [accessKey]: 'memory-only-token', accountId: 'workspace-123' };
  const result = await probe({ sessionOnly: false, session });
  assert.equal(result.messages[0]?.type, 'SESSION_READY');
  assert.deepEqual(result.messages[0]?.session, session);
}
for (const session of [{}, { user: null }, { WARNING_BANNER: 'Sign in required' }, { error: { code: 'WARNING_BANNER' } }]) {
  assert.equal((await probe({ session })).messages[0]?.type, 'SESSION_SIGNED_OUT');
  assert.equal((await probe({ sessionOnly: false, session })).messages.length, 0);
}
for (const status of [200, 401, 403, 503]) {
  assert.equal((await probe({ status, challenge: true })).messages[0]?.type, 'SESSION_ERROR');
}
assert.equal((await probe({ status: 401 })).messages[0]?.type, 'SESSION_SIGNED_OUT');
assert.equal((await probe({ status: 403 })).messages[0]?.type, 'SESSION_ERROR');
assert.equal((await probe({ session: { user: { id: 'user-123' } } })).messages[0]?.type, 'SESSION_ERROR');
for (const input of [{ origin: 'https://example.com' }, { subframe: true }]) {
  assert.equal((await probe(input)).requests, 0);
}
console.log('Codex browser session regression checks passed');
