// Synthetic small-business acceptance and crash/retry/restore proof. Owns only its random database and child process.
import assert from 'node:assert/strict';
import { randomBytes, randomUUID } from 'node:crypto';
import { spawn } from 'node:child_process';
import { createServer } from 'node:net';
import { mkdir, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fingerprint } from './backup-manifest.mjs';

const root = fileURLToPath(new URL('../', import.meta.url));
const binary = resolve(root, process.env.HARUFLOW_READINESS_BINARY || `target/debug/haruflow${process.platform === 'win32' ? '.exe' : ''}`);
const parent = new URL(process.env.HARUFLOW_READINESS_POSTGRES_URL || 'postgres://postgres@127.0.0.1:55432/postgres');
assert.ok(['postgres:', 'postgresql:'].includes(parent.protocol));
const database = `haruflow_readiness_${randomBytes(4).toString('hex')}`;
const connection = new URL(parent); connection.pathname = `/${database}`;
const tool = name => process.env[`HARUFLOW_READINESS_${name.toUpperCase()}`] || name;
const pgEnv = db => ({ ...process.env, PGHOST: parent.hostname, PGPORT: parent.port || '5432', PGUSER: decodeURIComponent(parent.username), PGPASSWORD: decodeURIComponent(parent.password), PGDATABASE: db });
const setup = randomBytes(32).toString('hex'), password = randomBytes(24).toString('hex');
let child, base, cookie = '', created = false, log = '';
async function command(exe, args, env = process.env) {
  const process = spawn(exe, args, { cwd: root, env, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
  let stdout = '', stderr = '';
  process.stdout.on('data', c => { stdout += c; }); process.stderr.on('data', c => { stderr += c; });
  await new Promise((ok, fail) => { process.once('error', fail); process.once('exit', code => code === 0 ? ok() : fail(new Error(`Command failed (${code}): ${stderr.slice(-3000)}`))); });
  return stdout.trim();
}
const sql = (query, db = database) => command(tool('psql'), ['-X', '-Atq', '-v', 'ON_ERROR_STOP=1', '-c', query], pgEnv(db));
const json = async query => JSON.parse(await sql(query));
async function start() {
  const listener = createServer(); await new Promise(ok => listener.listen(0, '127.0.0.1', ok));
  const port = listener.address().port; await new Promise(ok => listener.close(ok)); base = `http://127.0.0.1:${port}`;
  child = spawn(binary, ['serve'], { cwd: root, windowsHide: true, env: { ...process.env, DATABASE_URL: connection.href, HARUFLOW_BIND: `127.0.0.1:${port}`, HARUFLOW_SETUP_TOKEN: setup, HARUFLOW_SECURE_COOKIES: 'false', HARUFLOW_STATIC_DIR: resolve(root, 'apps/web/dist') }, stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout.on('data', c => { log = (log + c).slice(-6000); }); child.stderr.on('data', c => { log = (log + c).slice(-6000); });
  let failure; child.once('error', e => { failure = e; });
  for (let i = 0; i < 100; i++) {
    if (failure || child.exitCode !== null) throw failure || new Error(log);
    if (await fetch(`${base}/health/ready`).then(r => r.ok).catch(() => false)) return;
    await new Promise(ok => setTimeout(ok, 100));
  }
  throw new Error(`Server readiness timeout: ${log}`);
}
async function stop() { if (child && child.exitCode === null) { const own = child; await new Promise(ok => { own.once('exit', ok); own.kill(); }); } child = undefined; }
async function api(path, body, method = body ? 'POST' : 'GET') {
  const response = await fetch(`${base}/api${path}`, { method, headers: { 'content-type': 'application/json', 'x-haruflow-request': '1', ...(cookie ? { cookie } : {}) }, ...(body ? { body: JSON.stringify(body) } : {}), signal: AbortSignal.timeout(45000) });
  const session = response.headers.get('set-cookie'); if (session) cookie = session.split(';')[0];
  const value = await response.json(); assert.ok(response.ok, `${method} ${path}: ${response.status} ${JSON.stringify(value)}`); return value;
}
async function post(path, body = {}) { const input = { ...body, idempotency_key: randomUUID() }; const result = await api(path, input); assert.deepEqual(await api(path, input), result, `Retry ${path}`); return result; }
async function importCsv(kind, csv) { const preview = await api(`/transaction-imports/${kind}/preview`, { csv, delimiter: ',' }); assert.equal(preview.preview.error_count, 0, JSON.stringify(preview)); return post(`/transaction-imports/${preview.id}/commit`, { confirmed: true }); }
const money = (actual, expected) => assert.equal(Number(actual), expected);
const journal = description => ({ description, lines: [{ account_code: '1100', debit: '10', credit: '0' }, { account_code: '3000', debit: '0', credit: '10' }], idempotency_key: randomUUID() });
const snapshot = () => json(`SELECT json_build_object('journals',(SELECT json_agg(j ORDER BY id) FROM journal_entries j),'lines',(SELECT json_agg(l ORDER BY id) FROM journal_lines l),'products',(SELECT json_agg(p ORDER BY id) FROM products p),'buckets',(SELECT json_agg(b ORDER BY id) FROM stock_buckets b),'documents',(SELECT json_agg(d ORDER BY id) FROM historical_documents d),'payments',(SELECT json_agg(p ORDER BY id) FROM historical_payments p),'statements',(SELECT json_agg(s ORDER BY id) FROM bank_statements s),'matches',(SELECT json_agg(e ORDER BY id) FROM bank_reconciliation_events e),'closings',(SELECT json_agg(c ORDER BY id) FROM fiscal_closings c))`);
try {
  await sql(`CREATE DATABASE ${database}`, 'postgres'); created = true;
  await command(binary, ['migrate'], { ...process.env, DATABASE_URL: connection.href }); await start();
  const build = await fetch(`${base}/version`).then(r => r.json()); assert.equal(build.schema_version, 30);
  await api('/setup', { token: setup, company_name: 'READINESS synthetic small business', name: 'Readiness administrator', email: 'readiness@example.test', password });
  await post('/accounting/journals', { description: 'Modal uji sintetis', lines: [{ account_code: '1100', debit: '2000', credit: '0' }, { account_code: '3000', debit: '0', credit: '2000' }] });
  await post('/treasury/accounts', { account_code: '1110', name: 'Bank operasional uji', kind: 'bank' });
  await post('/treasury/transfers', { from_account: '1100', to_account: '1110', amount: '700', reason: 'Setoran kas ke bank uji', reference: 'BANK-READINESS' });
  const today = await sql('SELECT (now() AT TIME ZONE timezone)::date::text FROM company');
  const statement = { account_code: '1110', reference: 'READINESS-STATEMENT', from: today, to: today, opening_balance: '0', closing_balance: '700', csv: `date,reference,description,amount\n${today},BANK-READINESS,Setoran kas,700\n` };
  const preview = await api('/treasury/statements/preview', statement); await post('/treasury/statements/import', { ...statement, sha256: preview.sha256 });
  const reconciliation = await api('/treasury/reconciliation?account_code=1110');
  await post(`/treasury/statement-lines/${reconciliation.lines[0].id}/match`, { version: reconciliation.lines[0].version, journal_line_id: reconciliation.ledger[0].id, reason: 'Cocok berdasarkan dokumen setoran' });
  const matched = await api('/treasury/reconciliation?account_code=1110'); assert.equal(matched.statements[0].unmatched_bank, 0); assert.equal(matched.statements[0].unmatched_ledger, 0);
  const stock = await api('/products', { sku: 'RD-STOCK', name: 'Barang uji', unit: 'pcs', price: '30' });
  const service = await api('/products', { sku: 'RD-SERVICE', name: 'Jasa uji', kind: 'service', unit: 'jam', price: '70' });
  const warehouse = await post('/inventory/warehouses', { code: 'RD-CABANG', name: 'Gudang cabang uji' });
  const main = (await api('/inventory/warehouses')).items.find(w => w.is_default);
  await post('/inventory/receipts', { product_id: stock.id, quantity: '10', unit_cost: '10', reference: 'RD-RECEIPT', stock_context: { warehouse_id: warehouse.id } });
  let position = (await api(`/inventory/positions?product_id=${stock.id}`)).items.find(p => p.warehouse_id === warehouse.id);
  await post('/inventory/transfers', { from_warehouse_id: warehouse.id, to_warehouse_id: main.id, items: [{ product_id: stock.id, quantity: '4', version: position.version }], reason: 'Pindahkan stok bebas ke utama' });
  const supplier = await post('/suppliers', { code: 'RD-SUPPLIER', name: 'Supplier uji' });
  let po = await post('/purchase-orders', { supplier_id: supplier.id, items: [{ product_id: stock.id, quantity: '2', unit_cost: '100' }, { product_id: service.id, quantity: '1', unit_cost: '50' }] });
  po = await post(`/purchase-orders/${po.id}/confirm`, { version: po.version });
  po = await post(`/purchase-orders/${po.id}/receive`, { version: po.version });
  const receipt = po.receipts.find(r => r.id === po.active_receipt_id);
  const bill = await post('/purchase-bills', { receipt_id: receipt.id, order_version: po.version, supplier_invoice_number: 'RD-BILL', invoice_date: today, due_date: today, shipping: '20', discount: '10', tax: { code: 'EXAMPLE10', name: 'Pajak contoh uji', rate: '10', basis: 'exclusive' }, items: receipt.items.map(line => ({ receipt_line_id: line.id, quantity: line.quantity, unit_cost: line.product_kind === 'service' ? '60' : '110' })) });
  money(bill.total, 319); money(bill.receipt_value, 250);
  const customer = await api('/customers', { name: 'Pelanggan historis uji' });
  const imported = await importCsv('customer_invoices', `reference,party_id,document_date,due_date,total,counter_account,description\nRD-HISTORY,${customer.id},2020-01-02,2020-02-02,100,4000,Invoice migrasi uji\n`);
  await importCsv('customer_payments', `reference,document_reference,party_id,payment_date,amount,cash_account_code\nRD-PAY,RD-HISTORY,${customer.id},2020-01-10,40,1100\n`);
  const historical = await api(`/historical-documents/${imported.items[0].id}`); money(historical.outstanding, 60);
  const subledger = await api(`/parties/customer/${customer.id}/statement?as_of=2020-01-10`); money(subledger.summary.outstanding, 60); assert.equal(subledger.reconciliation.balanced, true);
  const plan = await api('/accounting/fiscal/preview?year=2020'); money(plan.net_profit, 100);
  await post('/accounting/fiscal/close', { year: 2020, period_version: plan.period_version, journal_count: plan.journal_count, reason: 'Uji penutupan tahun sintetis', confirmed: true });
  const report = await api('/accounting/reports?from=2020-01-01&to=2020-12-31'); money(report.income_statement.net_profit, 100); assert.equal(report.balance_sheet.balanced, true);
  // Hold a journal inside its SQL transaction, kill only our server, then retry exactly the same request.
  await sql("CREATE FUNCTION readiness_pause() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.description='INTERRUPTED READINESS JOURNAL' THEN PERFORM pg_sleep(30); END IF; RETURN NEW; END $$; CREATE TRIGGER readiness_pause BEFORE INSERT ON journal_entries FOR EACH ROW EXECUTE FUNCTION readiness_pause();");
  const interruptedBody = journal('INTERRUPTED READINESS JOURNAL');
  const pending = api('/accounting/journals', interruptedBody).then(() => 'response', () => 'disconnected');
  let sleeping = false;
  for (let i = 0; i < 100; i++) { sleeping = Number(await sql("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND wait_event='PgSleep'")) > 0; if (sleeping) break; await new Promise(ok => setTimeout(ok, 50)); }
  assert.ok(sleeping, 'Transaction must be in progress before interruption'); await stop();
  await sql("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid()");
  await pending;
  assert.equal(Number(await sql("SELECT count(*) FROM journal_entries WHERE description='INTERRUPTED READINESS JOURNAL'")), 0, 'Uncommitted journal rolled back');
  await sql('DROP TRIGGER readiness_pause ON journal_entries; DROP FUNCTION readiness_pause()');
  await start(); const recovered = await api('/accounting/journals', interruptedBody); assert.deepEqual(await api('/accounting/journals', interruptedBody), recovered);
  assert.equal(Number(await sql("SELECT count(*) FROM journal_entries WHERE description='INTERRUPTED READINESS JOURNAL'")), 1);
  // Restart after a committed response and retry again: cache and journal survive together.
  await stop(); await start(); assert.deepEqual(await api('/accounting/journals', interruptedBody), recovered);
  assert.equal(Number(await sql('SELECT count(*) FROM (SELECT journal_id FROM journal_lines GROUP BY journal_id HAVING sum(debit)<>sum(credit))x')), 0);
  assert.equal(Number(await sql('SELECT count(*) FROM products p WHERE p.quantity<>(SELECT COALESCE(sum(quantity),0) FROM stock_buckets b WHERE b.product_id=p.id) OR p.inventory_value<>(SELECT COALESCE(sum(inventory_value),0) FROM stock_buckets b WHERE b.product_id=p.id)')), 0);
  const before = await snapshot(); await stop();
  await mkdir(resolve(root, '.local'), { recursive: true }); const archive = resolve(root, `.local/${database}.backup`);
  await command(tool('pg_dump'), ['-Fc', '-f', archive], pgEnv(database)); await command(tool('pg_restore'), ['--list', archive], pgEnv(database));
  await start(); await post('/accounting/journals', { ...journal('READINESS temporary mutation') }); await stop();
  await command(tool('pg_restore'), ['--clean', '--if-exists', '--single-transaction', '--exit-on-error', '-d', database, archive], pgEnv(database)); assert.deepEqual(await snapshot(), before);
  await start(); assert.deepEqual(await api('/accounting/journals', interruptedBody), recovered);
  const output = resolve(root, `output/readiness/${database}.json`); await mkdir(resolve(root, 'output/readiness'), { recursive: true });
  await writeFile(output, JSON.stringify({ at: new Date().toISOString(), build, database, scope: 'Synthetic disposable small-business fixture, not real business pilot acceptance', checks: ['multiple cash accounts and reconciliation', 'legacy receipts with location context and stock transfers', 'mixed goods/services purchasing with variance, shipping, discount and explicit example tax', 'historical invoices/payments and as-of party ledger', 'year close preserving profit and balanced balance sheet', 'in-flight transaction rollback after owned server termination', 'same-key retry after restart', 'backup/mutate/restore exact business snapshot'], backup: await fingerprint(archive), snapshot: before }, null, 2));
  console.log(`Community readiness passed: ${output}`);
} finally {
  await stop(); if (created) { assert.match(database, /^haruflow_readiness_[a-f0-9]{8}$/); await sql(`SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='${database}' AND pid<>pg_backend_pid()`, 'postgres'); await sql(`DROP DATABASE ${database}`, 'postgres'); }
}
