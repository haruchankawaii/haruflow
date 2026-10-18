// Disposable-instance integration check. Never run against a business installation.
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

const mode = process.argv[2];
assert.ok(['seed', 'mutate', 'verify'].includes(mode), 'Use smoke.mjs seed|mutate|verify on a disposable instance.');
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
} else {
  await login();
}

if (mode === 'mutate') {
  await api('/customers', 'POST', { name: 'Must disappear after backup restore' });
  console.log('Post-backup mutation created on disposable instance.');
} else {
  const dashboard = await api('/dashboard');
  for (const [key, expected] of Object.entries({ revenue: 160000, outstanding: 0, inventory_value: 400000, cash: 160000, customers_count: 1, products_count: 1, orders_count: 1 })) {
    assert.equal(Number(dashboard[key]), expected, `Dashboard ${key}`);
  }
  const trial = await api('/accounting/trial-balance');
  assert.equal(trial.balanced, true);
  assert.equal(Number(trial.total_debit), 660000);
  assert.equal(Number(trial.total_credit), 660000);
  const products = await api('/products');
  assert.equal(Number(products.items[0].quantity), 8);
  const journals = await api('/accounting/journals');
  assert.equal(journals.items.length, 4);
  console.log(`Disposable-instance ${mode} check passed: frontend, login, stock, sales, payment, ledger balances.`);
}
