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
  await api('/inventory/receipts', 'POST', { product_id: product.id, quantity: '10', unit_cost: '50000', reference: 'CI opening stock', idempotency_key: crypto.randomUUID() });
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
  for (const [key, expected] of Object.entries({ revenue: 170000, outstanding: 0, inventory_value: 400000, cash: 170000, customers_count: 1, products_count: 2, orders_count: 2 })) {
    assert.equal(Number(dashboard[key]), expected, `Dashboard ${key}`);
  }
  const trial = await api('/accounting/trial-balance');
  assert.equal(trial.balanced, true);
  assert.equal(Number(trial.total_debit), 670000);
  assert.equal(Number(trial.total_credit), 670000);
  const products = await api('/products');
  assert.equal(Number(products.items.find(product => product.sku === 'CI-001').quantity), 8);
  const service = products.items.find(product => product.sku === 'CI-SERVICE');
  assert.equal(service.kind, 'service');
  assert.equal(Number(service.quantity), 0);
  assert.equal(Number(service.inventory_value), 0);
  const movements = await api('/inventory/movements');
  assert.equal(movements.items.length, 2, 'Service invoicing cannot create stock movements');
  const company = await api('/company');
  assert.equal(company.order_prefix, 'ORD');
  assert.equal(company.invoice_prefix, 'BILL');
  const orders = await api('/orders');
  assert.ok(orders.items.some(order => order.number === 'SO-000001'), 'Existing numbers survive prefix edits');
  const journals = await api('/accounting/journals');
  assert.equal(journals.items.length, 6);
  console.log(`Disposable-instance ${mode} check passed: frontend, login, stock, sales, payment, ledger balances.`);
}
