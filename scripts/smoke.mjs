// Disposable-instance integration check. Never run against a business installation.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const mode = process.argv[2];
assert.ok(['seed', 'mutate', 'verify', 'recover'].includes(mode), 'Use smoke.mjs seed|mutate|verify|recover on a disposable instance.');
const base = process.env.HARUFLOW_SMOKE_URL || 'http://127.0.0.1:8080';
const password = process.env.HARUFLOW_SMOKE_PASSWORD || 'haruflow-ci-only-password-2026';
let cookie = '';

async function api(path, method = 'GET', body) {
  const response = await fetch(`${base}/api${path}`, {
    method,
    headers: { 'Content-Type': 'application/json', 'X-Haruflow-Request': '1', ...(cookie ? { Cookie: cookie } : {}) },
    ...(body !== undefined ? { body: JSON.stringify(body) } : {}),
    signal: AbortSignal.timeout(15000),
  });
  const result = await response.json();
  assert.ok(response.ok, `${method} ${path} failed: ${response.status} ${JSON.stringify(result)}`);
  const session = response.headers.get('set-cookie');
  if (session) cookie = session.split(';')[0];
  return result;
}

async function login() {
  await api('/auth/login', 'POST', { email: 'ci@example.test', password });
}

const health = await fetch(`${base}/health/ready`, { signal: AbortSignal.timeout(10000) });
assert.equal(health.status, 200, 'Application/database is not ready');
const page = await fetch(base, { signal: AbortSignal.timeout(10000) });
assert.equal(page.status, 200);
assert.match(await page.text(), /id="root"/, 'Built frontend is not being served');

if (mode === 'seed') {
  const status = await api('/setup/status');
  assert.equal(status.needs_setup, true, 'Seed requires a new disposable installation');
  let token = process.env.HARUFLOW_SMOKE_SETUP_TOKEN;
  if (!token) {
    const env = await readFile(new URL('../.env', import.meta.url), 'utf8');
    token = env.match(/^HARUFLOW_SETUP_TOKEN=(.+)$/m)?.[1].trim();
  }
  assert.ok(token, 'Missing setup token');
  await api('/setup', 'POST', { token, company_name: 'CI Disposable Company', name: 'CI Administrator', email: 'ci@example.test', password });
  const customer = await api('/customers', 'POST', { name: 'CI Customer' });
  const product = await api('/products', 'POST', { sku: 'CI-001', name: 'CI Product', unit: 'pcs', price: '80000' });
  const openingInput = {
    description: 'CI initial balances',
    items: [{ product_id: product.id, quantity: '10', unit_cost: '50000' }],
    lines: [{ account_code: '1100', debit: '100000', credit: '0' }, { account_code: '3000', debit: '0', credit: '600000' }],
  };
  const draft = await api('/accounting/opening', 'PUT', { ...openingInput, version: 0 });
  assert.equal(draft.document.balanced, true);
  assert.equal(Number(draft.document.inventory_total), 500000);
  const firstPost = { version: draft.document.version, idempotency_key: crypto.randomUUID() };
  const posted = await api('/accounting/opening/post', 'POST', firstPost);
  assert.deepEqual(await api('/accounting/opening/post', 'POST', firstPost), posted, 'Retry cannot duplicate opening balances');
  const reversed = await api('/accounting/opening/reverse', 'POST', { version: posted.document.version, reason: 'CI checks correction history', idempotency_key: crypto.randomUUID() });
  assert.equal(reversed.document.status, 'draft');
  const correctedDraft = await api('/accounting/opening', 'PUT', { ...openingInput, description: 'CI reviewed opening balances', version: reversed.document.version });
  await api('/accounting/opening/post', 'POST', { version: correctedDraft.document.version, idempotency_key: crypto.randomUUID() });
  const order = await api('/orders', 'POST', { customer_id: customer.id, items: [{ product_id: product.id, quantity: '2', unit_price: '80000' }], idempotency_key: crypto.randomUUID() });
  await api(`/orders/${order.id}/confirm`, 'POST', { idempotency_key: crypto.randomUUID() });
  await api(`/orders/${order.id}/ship`, 'POST', { idempotency_key: crypto.randomUUID() });
  const invoice = await api(`/orders/${order.id}/invoice`, 'POST', { idempotency_key: crypto.randomUUID() });
  await api(`/invoices/${invoice.id}/payments`, 'POST', { amount: '160000', idempotency_key: crypto.randomUUID() });
  const company = await api('/company');
  await api('/company', 'PATCH', { name: company.name, timezone: company.timezone, address: 'CI test address', order_prefix: 'ORD', invoice_prefix: 'BILL' });
  const service = await api('/products', 'POST', { sku: 'CI-SERVICE', name: 'CI Service', kind: 'service', unit: 'jam', price: '10000' });
  const serviceOrder = await api('/orders', 'POST', { customer_id: customer.id, items: [{ product_id: service.id, quantity: '1', unit_price: '10000' }], idempotency_key: crypto.randomUUID() });
  assert.equal(serviceOrder.number, 'ORD-000002');
  assert.equal(serviceOrder.has_stock_items, false);
  await api(`/orders/${serviceOrder.id}/confirm`, 'POST', { idempotency_key: crypto.randomUUID() });
  const serviceInvoice = await api(`/orders/${serviceOrder.id}/invoice`, 'POST', { idempotency_key: crypto.randomUUID() });
  assert.equal(serviceInvoice.number, 'BILL-000002');
  await api(`/invoices/${serviceInvoice.id}/payments`, 'POST', { amount: '10000', idempotency_key: crypto.randomUUID() });
  const wrongReceipt = await api('/inventory/receipts', 'POST', { product_id: product.id, quantity: '2', unit_cost: '70000', reference: 'CI wrong receipt', idempotency_key: crypto.randomUUID() });
  const receiptCorrection = { reason: 'CI corrects the receipt quantity and cost', idempotency_key: crypto.randomUUID() };
  const receiptReversal = await api(`/inventory/receipts/${wrongReceipt.id}/reverse`, 'POST', receiptCorrection);
  assert.deepEqual(await api(`/inventory/receipts/${wrongReceipt.id}/reverse`, 'POST', receiptCorrection), receiptReversal);
  await api('/inventory/receipts', 'POST', { product_id: product.id, quantity: '3', unit_cost: '60000', reference: 'CI reviewed receipt', idempotency_key: crypto.randomUUID() });
  const wrongJournal = await api('/accounting/journals', 'POST', { description: 'CI wrong manual cash', lines: [{ account_code: '1100', debit: '1000', credit: '0' }, { account_code: '3000', debit: '0', credit: '1000' }], idempotency_key: crypto.randomUUID() });
  const journalCorrection = { reason: 'CI removes incorrect manual cash', idempotency_key: crypto.randomUUID() };
  const journalReversal = await api(`/accounting/journals/${wrongJournal.id}/reverse`, 'POST', journalCorrection);
  assert.deepEqual(await api(`/accounting/journals/${wrongJournal.id}/reverse`, 'POST', journalCorrection), journalReversal);
} else {
  await login();
}

if (mode === 'mutate') {
  await api('/customers', 'POST', { name: 'Must disappear after backup restore' });
  console.log('Post-backup mutation created on disposable instance.');
} else {
  if (mode === 'recover') {
    assert.equal(process.env.CI, 'true', 'CLI recovery smoke runs only on disposable CI infrastructure');
    assert.equal(process.env.COMPOSE_PROJECT_NAME, 'haruflow-ci');
    const recoveryProcess = spawn('docker', ['compose', '--env-file', '.env', '-f', 'deploy/community/compose.yml', 'exec', '-T', 'app', 'haruflow', 'admin', 'reset-password', 'ci@example.test', '--password-stdin'], {
      cwd: fileURLToPath(new URL('../', import.meta.url)), stdio: ['pipe', 'inherit', 'inherit'],
    });
    const done = new Promise((accept, reject) => {
      recoveryProcess.once('error', reject);
      recoveryProcess.once('exit', code => code === 0 ? accept() : reject(new Error(`Recovery CLI exited with ${code}`)));
    });
    recoveryProcess.stdin.on('error', () => {});
    recoveryProcess.stdin.end(`${password}\n`);
    await done;
    const revoked = await fetch(`${base}/api/auth/me`, { headers: { Cookie: cookie }, signal: AbortSignal.timeout(10000) });
    assert.equal(revoked.status, 401, 'Recovery must invalidate the old browser session');
    await login();
  }
  const dashboard = await api('/dashboard');
  for (const [key, expected] of Object.entries({ revenue: 170000, outstanding: 0, inventory_value: 580000, cash: 270000, customers_count: 1, products_count: 2, orders_count: 2 })) {
    assert.equal(Number(dashboard[key]), expected, `Dashboard ${key}`);
  }
  const trial = await api('/accounting/trial-balance');
  assert.equal(trial.balanced, true);
  assert.equal(Number(trial.total_debit), 950000);
  assert.equal(Number(trial.total_credit), 950000);
  const products = await api('/products');
  assert.equal(Number(products.items.find(product => product.sku === 'CI-001').quantity), 11);
  const service = products.items.find(product => product.sku === 'CI-SERVICE');
  assert.equal(service.kind, 'service');
  assert.equal(Number(service.quantity), 0);
  assert.equal(Number(service.inventory_value), 0);
  const movements = await api('/inventory/movements');
  assert.equal(movements.items.length, 7, 'Opening, stock shipping, and receipt corrections retain their history');
  const correctedReceipt = movements.items.find(movement => movement.reference === 'CI wrong receipt');
  assert.equal(correctedReceipt.reversal.reason, 'CI corrects the receipt quantity and cost');
  assert.equal(correctedReceipt.can_reverse, false);
  const reviewedReceipt = movements.items.find(movement => movement.reference === 'CI reviewed receipt');
  assert.equal(reviewedReceipt.can_reverse, true);
  const opening = await api('/accounting/opening');
  assert.equal(opening.document.status, 'posted');
  assert.equal(opening.eligible, false);
  assert.equal(opening.can_reverse, false, 'Opening cannot reverse after business postings');
  assert.equal(opening.document.history.length, 2);
  assert.equal(opening.document.history.filter(posting => posting.reversed_at === null).length, 1);
  assert.ok(opening.document.history.some(posting => posting.reason === 'CI checks correction history'));
  const company = await api('/company');
  assert.equal(company.order_prefix, 'ORD');
  assert.equal(company.invoice_prefix, 'BILL');
  const orders = await api('/orders');
  assert.ok(orders.items.some(order => order.number === 'SO-000001'), 'Existing numbers survive prefix edits');
  const journals = await api('/accounting/journals');
  assert.equal(journals.items.length, 13);
  const correctedJournal = journals.items.find(journal => journal.description === 'CI wrong manual cash');
  assert.equal(correctedJournal.reversal.reason, 'CI removes incorrect manual cash');
  assert.equal(correctedJournal.can_reverse, false);
  const insights = await api('/dashboard/insights?months=6');
  assert.equal(insights.series.length, 6);
  for (const [key, expected] of Object.entries({ revenue: 170000, cogs: 100000, gross_profit: 70000, cash_in: 371000, cash_out: 101000, cash_net: 270000 })) {
    assert.equal(Number(insights.totals[key]), expected, `Chart ${key}`);
  }
  assert.equal(Number(insights.receivables.invoiced), 170000);
  assert.equal(Number(insights.receivables.paid), 170000);
  assert.equal(Number(insights.receivables.outstanding), 0);
  console.log(`Disposable-instance ${mode} check passed: corrections, chart totals, stock, sales, payments, and restored ledger balances.`);
}
