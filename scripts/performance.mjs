// Creates its own uniquely named PostgreSQL database. Never points the ERP at an existing business database.
import assert from 'node:assert/strict';
import { randomBytes, randomUUID, createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { cpus, totalmem, platform, release } from 'node:os';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { performance } from 'node:perf_hooks';
import { fingerprint } from './backup-manifest.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const binary = resolve(root, process.env.HARUFLOW_PERF_BINARY || `target/debug/haruflow${platform() === 'win32' ? '.exe' : ''}`);
const psql = process.env.HARUFLOW_PERF_PSQL || 'psql';
const pgDump = process.env.HARUFLOW_PERF_PG_DUMP || 'pg_dump';
const pgRestore = process.env.HARUFLOW_PERF_PG_RESTORE || 'pg_restore';
const parent = new URL(process.env.HARUFLOW_PERF_POSTGRES_URL || 'postgres://postgres@127.0.0.1:55432/postgres');
assert.ok(['postgres:', 'postgresql:'].includes(parent.protocol), 'Provide a PostgreSQL admin connection for disposable test databases.');
const database = `haruflow_perf_${randomBytes(4).toString('hex')}`;
assert.match(database, /^haruflow_perf_[a-f0-9]{8}$/);
const rows = Number(process.env.HARUFLOW_PERF_ROWS || 1200);
const concurrency = Number(process.env.HARUFLOW_PERF_CONCURRENCY || 5);
assert.ok(Number.isSafeInteger(rows) && rows >= 100 && rows <= 50000, 'HARUFLOW_PERF_ROWS must be 100–50000.');
assert.ok(Number.isSafeInteger(concurrency) && concurrency >= 1 && concurrency <= 10, 'Concurrency must be 1–10.');
const output = resolve(root, `output/performance/${database}.json`);
const archive = resolve(root, `.local/${database}.backup`);
const setup = randomBytes(32).toString('hex'), password = randomBytes(24).toString('hex');
let created = false, server, serverLog = '', cookie = '', base;
const pgEnv = db => ({ ...process.env, PGHOST: parent.hostname, PGPORT: parent.port || '5432',
  PGUSER: decodeURIComponent(parent.username), PGPASSWORD: decodeURIComponent(parent.password), PGDATABASE: db });
async function command(executable, args, env = process.env, input) {
  const child = spawn(executable, args, { cwd: root, env, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'] });
  let stdout = '', stderr = '';
  child.stdout.on('data', chunk => { stdout += chunk; }); child.stderr.on('data', chunk => { stderr += chunk; });
  child.stdin.on('error', () => {}); child.stdin.end(input);
  await new Promise((accept, reject) => {
    child.once('error', reject); child.once('exit', code => code === 0 ? accept() : reject(new Error(`${executable} exited ${code}: ${stderr.slice(-3000)}`)));
  });
  return stdout.trim();
}
const sql = (db, query) => command(psql, ['-X', '-Atq', '-v', 'ON_ERROR_STOP=1', '-c', query], pgEnv(db));
async function start() {
  const listener = createServer(); await new Promise(accept => listener.listen(0, '127.0.0.1', accept));
  const port = listener.address().port; await new Promise(accept => listener.close(accept)); base = `http://127.0.0.1:${port}`;
  const connection = new URL(parent); connection.pathname = `/${database}`;
  server = spawn(binary, ['serve'], { cwd: root, windowsHide: true, env: { ...process.env,
    DATABASE_URL: connection.href, HARUFLOW_BIND: `127.0.0.1:${port}`, HARUFLOW_SETUP_TOKEN: setup,
    HARUFLOW_SECURE_COOKIES: 'false', HARUFLOW_STATIC_DIR: resolve(root, 'apps/web/dist') }, stdio: ['ignore', 'pipe', 'pipe'] });
  server.stdout.on('data', chunk => { serverLog = (serverLog + chunk).slice(-10000); });
  server.stderr.on('data', chunk => { serverLog = (serverLog + chunk).slice(-10000); });
  let failure; server.once('error', error => { failure = error; });
  for (let i = 0; i < 100; i++) {
    if (failure || server.exitCode !== null) throw failure || new Error(`Test server stopped: ${serverLog}`);
    if (await fetch(`${base}/health/ready`).then(r => r.ok).catch(() => false)) return;
    await new Promise(accept => setTimeout(accept, 100));
  }
  throw new Error('Disposable server did not become ready.');
}
async function stop() {
  if (server && server.exitCode === null) { const current = server; await new Promise(accept => { current.once('exit', accept); current.kill(); }); }
  server = undefined;
}
async function api(path, body, method = body ? 'POST' : 'GET') {
  const response = await fetch(`${base}/api${path}`, { method, headers: { 'content-type': 'application/json', 'x-haruflow-request': '1', ...(cookie ? { cookie } : {}) },
    ...(body ? { body: JSON.stringify(body) } : {}), signal: AbortSignal.timeout(30000) });
  const next = response.headers.get('set-cookie'); if (next) cookie = next.split(';')[0];
  const value = await response.json(); assert.ok(response.ok, `${method} ${path}: ${response.status}: ${JSON.stringify(value)}`); return value;
}
const journalBody = description => ({ description, lines: [{ account_code: '1100', debit: '1', credit: '0' }, { account_code: '3000', debit: '0', credit: '1' }], idempotency_key: randomUUID() });
function proofPdf(size) {
  let padding = size - 512;
  for (let attempt = 0; attempt < 5; attempt++) {
    let content = `%PDF-1.4\n%${'x'.repeat(padding)}\n`;
    const objects = ['<< /Type /Catalog /Pages 2 0 R >>', '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
      '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>', '<< /Length 0 >>\nstream\n\nendstream'];
    const offsets = [0];
    objects.forEach((body, i) => { offsets.push(Buffer.byteLength(content)); content += `${i + 1} 0 obj\n${body}\nendobj\n`; });
    const xref = Buffer.byteLength(content);
    content += `xref\n0 5\n0000000000 65535 f \n${offsets.slice(1).map(offset => `${String(offset).padStart(10, '0')} 00000 n \n`).join('')}trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
    const bytes = Buffer.from(content);
    if (bytes.length === size) return bytes;
    padding += size - bytes.length;
  }
  throw new Error('Could not construct the fixed-size PDF fixture.');
}
async function measure(name, count, workers, operation) {
  const samples = [], errors = []; let index = 0; const start = performance.now();
  await Promise.all(Array.from({ length: Math.min(workers, count) }, async () => { while (index < count) {
    const current = index++; const before = performance.now();
    try { await operation(current); } catch (error) { errors.push(String(error)); }
    samples.push(performance.now() - before);
  } }));
  samples.sort((a, b) => a - b); const percentile = p => Number(samples[Math.max(0, Math.ceil(samples.length * p) - 1)].toFixed(2));
  return { name, requests: count, concurrency: workers, errors: errors.length, error_messages: [...new Set(errors)].slice(0, 5),
    elapsed_ms: Number((performance.now() - start).toFixed(2)), p50_ms: percentile(.5), p95_ms: percentile(.95), min_ms: percentile(0), max_ms: percentile(1) };
}

try {
  await mkdir(dirname(output), { recursive: true }); await mkdir(dirname(archive), { recursive: true });
  await sql('postgres', `CREATE DATABASE ${database}`); created = true;
  const connection = new URL(parent); connection.pathname = `/${database}`;
  await command(binary, ['migrate'], { ...process.env, DATABASE_URL: connection.href });
  await start();
  const build = await fetch(`${base}/version`).then(r => r.json());
  await api('/setup', { token: setup, company_name: 'PERF disposable company', name: 'PERF administrator', email: 'performance@example.test', password });
  await sql(database, `INSERT INTO customers(id,name,created_at) SELECT md5('perf-customer-'||n)::uuid,'PERF customer '||lpad(n::text,6,'0'),'2020-01-01'::timestamptz FROM generate_series(1,${rows})n`);
  for (let i = 0; i < 100; i++) await api('/accounting/journals', journalBody(`PERF seed ${i}`));
  const metrics = [];
  metrics.push(await measure('server list: 50 rows from full customer dataset', 30, concurrency, () => api('/records/customers?q=PERF&page_size=50')));
  metrics.push(await measure('all financial reports with full journal dataset', 20, concurrency, () => api('/accounting/reports')));
  metrics.push(await measure('new balanced journal posting', 30, concurrency, i => api('/accounting/journals', journalBody(`PERF measured ${i}`))));
  const retry = journalBody('PERF one effect under concurrent retry'); const retryIds = [];
  metrics.push(await measure('same idempotency key concurrent journal retry', 10, concurrency, async () => { retryIds.push((await api('/accounting/journals', retry)).id); }));
  assert.equal(new Set(retryIds).size, 1);
  assert.equal(Number(await sql(database, "SELECT count(*) FROM journal_entries WHERE description='PERF one effect under concurrent retry'")), 1);
  const bytes = proofPdf(5 * 1024 * 1024);
  let uploaded;
  metrics.push(await measure('5 MiB proof upload', 1, 1, async () => {
    const response = await fetch(`${base}/api/documents/journal/${retryIds[0]}/attachments?filename=performance.pdf&idempotency_key=${randomUUID()}`, {
      method: 'POST', headers: { cookie, 'content-type': 'application/pdf', 'x-haruflow-request': '1' }, body: bytes });
    uploaded = await response.json(); assert.ok(response.ok, JSON.stringify(uploaded));
  }));
  if (uploaded?.id) metrics.push(await measure('proof download and SHA-256 verification', 3, 1, async () => {
    const response = await fetch(`${base}/api/attachments/${uploaded.id}/download`, { headers: { cookie } }); assert.ok(response.ok);
    const actual = Buffer.from(await response.arrayBuffer()); assert.equal(createHash('sha256').update(actual).digest('hex'), createHash('sha256').update(bytes).digest('hex'));
  }));
  const snapshotQuery = "SELECT json_build_object('customers',(SELECT count(*) FROM customers),'journals',(SELECT count(*) FROM journal_entries),'cash',(SELECT sum(debit-credit)::text FROM journal_lines WHERE account_code='1100'),'files',(SELECT count(*) FROM document_attachments),'proof_sha',(SELECT min(sha256) FROM document_attachments))";
  const before = await sql(database, snapshotQuery);
  metrics.push(await measure('pg_dump custom archive', 1, 1, () => command(pgDump, ['-Fc', '-f', archive], pgEnv(database))));
  await stop();
  metrics.push(await measure('pg_restore single transaction', 1, 1, () => command(pgRestore, ['--clean', '--if-exists', '--single-transaction', '--exit-on-error', '-d', database, archive], pgEnv(database))));
  assert.equal(await sql(database, snapshotQuery), before, 'Restore must preserve measured dataset/cash/proof checksum.');
  const result = { created_at: new Date().toISOString(), scope: 'Synthetic loopback workload on an isolated disposable database; not a production SLA or business pilot.',
    database, build, profile: binary.includes('release') ? 'release' : binary.includes('debug') ? 'debug' : 'unspecified',
    host: { os: `${platform()} ${release()}`, cpu: cpus()[0]?.model, logical_cpus: cpus().length, memory_bytes: totalmem() },
    dataset: { customers: rows, seed_journals: 100, measured_new_journals: 31, proof_bytes: bytes.length },
    postgres_version: await sql(database, 'SHOW server_version'), archive: await fingerprint(archive), metrics, restored_snapshot: JSON.parse(before) };
  await writeFile(output, JSON.stringify(result, null, 2));
  console.log(`Performance evidence saved: ${output}`);
  for (const metric of metrics) console.log(`${metric.name}: n=${metric.requests}, concurrency=${metric.concurrency}, p50=${metric.p50_ms}ms, p95=${metric.p95_ms}ms, errors=${metric.errors}`);
  assert.equal(metrics.reduce((sum, metric) => sum + metric.errors, 0), 0, 'A failed workload is not a passing performance result.');
} finally {
  await stop();
  if (created) {
    assert.match(database, /^haruflow_perf_[a-f0-9]{8}$/);
    await sql('postgres', `SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='${database}' AND pid<>pg_backend_pid();`);
    await sql('postgres', `DROP DATABASE ${database}`);
  }
}
