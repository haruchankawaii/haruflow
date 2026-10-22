// Process-level smoke: no PostgreSQL or actual ERP data is used.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createServer } from 'node:http';
import { createInterface } from 'node:readline';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const executable = resolve(process.argv[2] || 'target/debug/haruflow.exe');
const token = 'smoke-only-mcp-token-123456789';
let revoked = false;
let emptyScopes = false;
let malformedProfile = false;
let apiRequests = 0;
const server = createServer(async (request, response) => {
  apiRequests++;
  assert.equal(request.headers.authorization, `Bearer ${token}`);
  assert.equal(request.headers['x-haruflow-request'], '1');
  response.setHeader('content-type', 'application/json');
  if (revoked) {
    response.writeHead(401).end(JSON.stringify({ error: `must never be forwarded ${token}` }));
    return;
  }
  if (request.url === '/api/ai/mcp/profile') {
    assert.equal(request.method, 'GET');
    response.end(JSON.stringify({ user: { id: 'smoke', name: 'Smoke', role: malformedProfile ? token : 'finance' }, scopes: emptyScopes ? [] : ['finance'], read_only: true, ignored_secret: token }));
  } else if (request.url === '/api/ai/mcp/tools') {
    assert.equal(request.method, 'GET');
    response.end(JSON.stringify({ tools: [{ name: 'cash_position', description: 'Smoke report', inputSchema: { type: 'object' }, annotations: { readOnlyHint: true } }] }));
  } else if (request.url === '/api/ai/mcp/call') {
    assert.equal(request.method, 'POST');
    let body = '';
    for await (const chunk of request) body += chunk;
    assert.deepEqual(JSON.parse(body), { name: 'cash_position', arguments: {} });
    response.end(JSON.stringify({ content: [{ type: 'text', text: 'Cash 100000 IDR' }], structuredContent: { cash: '100000' }, isError: false }));
  } else {
    response.writeHead(404).end('{}');
    throw new Error(`Unexpected bridge request ${request.url}`);
  }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const environment = {
  ...process.env,
  DATABASE_URL: 'invalid-database-must-never-be-opened',
  RUST_LOG: 'trace',
  HARUFLOW_MCP_URL: `http://127.0.0.1:${server.address().port}`,
  HARUFLOW_MCP_TOKEN: token,
  HTTP_PROXY: 'http://127.0.0.1:1',
  HTTPS_PROXY: 'http://127.0.0.1:1',
  ALL_PROXY: 'http://127.0.0.1:1',
  NO_PROXY: '',
};
delete environment.HARUFLOW_MCP_TOKEN_FILE;
const temporary = await mkdtemp(join(tmpdir(), 'haruflow-mcp-smoke-'));
const tokenFile = join(temporary, 'smoke-token.txt');
await writeFile(tokenFile, `${token}\n`, { encoding: 'utf8', mode: 0o600 });
// A synthetic file wins over an invalid environment token. No real credentials
// or application configuration are read or modified by this test.
const checkEnvironment = { ...environment, HARUFLOW_MCP_TOKEN_FILE: tokenFile, HARUFLOW_MCP_TOKEN: 'invalid' };
function runCheck() {
  return new Promise((resolve, reject) => {
    const check = spawn(executable, ['mcp', 'check'], { env: checkEnvironment, stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true });
    let output = '';
    let error = '';
    const timeout = setTimeout(() => check.kill(), 5000);
    check.stdout.on('data', chunk => { output += chunk; });
    check.stderr.on('data', chunk => { error += chunk; });
    check.once('error', failure => { clearTimeout(timeout); reject(failure); });
    check.once('close', (code, signal) => { clearTimeout(timeout); resolve({ code, signal, output, error }); });
  });
}
const child = spawn(executable, ['mcp'], { env: environment, stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
const pending = new Map();
let stdout = '';
let stderr = '';
let nextId = 0;
const reader = createInterface({ input: child.stdout });
reader.on('line', line => {
  stdout += `${line}\n`;
  const reply = JSON.parse(line); // Any log on stdout fails the smoke.
  assert.equal(reply.jsonrpc, '2.0');
  const receiver = pending.get(reply.id);
  assert.ok(receiver, `Unexpected stdout message ${line}`);
  pending.delete(reply.id);
  receiver(reply);
});
child.stderr.on('data', chunk => { stderr += chunk; });
const exited = new Promise((resolve, reject) => {
  child.once('error', reject);
  child.once('exit', (code, signal) => resolve({ code, signal }));
});
function request(method, params) {
  const id = ++nextId;
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(`MCP request ${method} timed out; stderr: ${stderr}`)), 5000);
    pending.set(id, reply => { clearTimeout(timeout); resolve(reply); });
    child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, ...(params === undefined ? {} : { params }) })}\n`);
  });
}
try {
  const diagnostics = await runCheck();
  assert.equal(diagnostics.code, 0, diagnostics.error);
  assert.equal(diagnostics.signal, null);
  assert.equal(diagnostics.error, '', 'Diagnostics must not initialize application logging or database access');
  assert.deepEqual(JSON.parse(diagnostics.output), {
    connected: true, read_only: true, user: { id: 'smoke', name: 'Smoke', role: 'finance' }, scopes: ['finance'], tools: ['cash_position'],
  });
  assert.ok(!diagnostics.output.includes(token) && !diagnostics.error.includes(token), 'Diagnostics leaked a secret field');
  emptyScopes = true;
  const noScopes = await runCheck();
  assert.notEqual(noScopes.code, 0);
  assert.equal(noScopes.output, '');
  assert.ok(noScopes.error.includes('no effective scopes'));
  emptyScopes = false;
  malformedProfile = true;
  const malformed = await runCheck();
  assert.notEqual(malformed.code, 0);
  assert.equal(malformed.output, '');
  assert.ok(malformed.error.includes('invalid MCP profile') && !malformed.error.includes(token));
  malformedProfile = false;
  const handshake = await request('initialize', { protocolVersion: '2025-11-25', capabilities: {}, clientInfo: { name: 'haruflow-smoke', version: '1' } });
  assert.equal(handshake.result.protocolVersion, '2025-11-25');
  assert.equal(handshake.result.serverInfo.name, 'haruflow-community');
  assert.ok(handshake.result.instructions.includes(environment.HARUFLOW_MCP_URL), 'Source links need the actual ERP origin, including its port');
  child.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`);
  assert.equal((await request('tools/list')).result.tools[0].name, 'cash_position');
  assert.equal((await request('tools/call', { name: 'cash_position', arguments: {} })).result.structuredContent.cash, '100000');
  assert.equal((await request('tools/call', { name: 'execute_sql', arguments: {} })).error.code, -32602);
  revoked = true;
  assert.equal((await request('tools/call', { name: 'cash_position', arguments: {} })).result.isError, true);
  assert.deepEqual((await request('ping')).result, {});
  child.stdin.end();
  const timer = setTimeout(() => child.kill(), 5000);
  const exit = await exited;
  clearTimeout(timer);
  assert.equal(exit.code, 0, stderr);
  assert.equal(exit.signal, null);
  assert.equal(pending.size, 0);
  assert.ok(!stdout.includes(token) && !stderr.includes(token), 'Connection secret leaked');
  assert.equal(stderr, '', 'The bridge must start without application/database logging');
  const revokedCheck = await runCheck();
  assert.notEqual(revokedCheck.code, 0);
  assert.equal(revokedCheck.output, '');
  assert.ok(revokedCheck.error.includes('revoked') && !revokedCheck.error.includes(token));
  console.log(`MCP STDIO process smoke passed: initialize, list, call, unknown tool, revocation, ping, clean EOF; diagnostics metadata, token file precedence, no scopes, malformed server and revoked access; ${apiRequests} authenticated local requests.`);
} finally {
  child.kill();
  server.closeAllConnections();
  await new Promise(resolve => server.close(resolve));
  await rm(temporary, { recursive: true, force: true });
}
