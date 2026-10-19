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

async function retried(path, body, method = 'POST') {
  const input = { ...body, idempotency_key: crypto.randomUUID() };
  const result = await api(path, method, input);
  assert.deepEqual(await api(path, method, input), result, `${method} ${path} must be idempotent`);
  return result;
}

async function conflict(path, body) {
  const response = await fetch(`${base}/api${path}`, {
    method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Haruflow-Request': '1', Cookie: cookie },
    body: JSON.stringify({ ...body, idempotency_key: crypto.randomUUID() }), signal: AbortSignal.timeout(15000),
  });
  assert.equal(response.status, 409, `${path} must reject an inconsistent posting: ${await response.text()}`);
}

function shiftDate(today, days) {
  const date = new Date(`${today}T12:00:00Z`);
  date.setUTCDate(date.getUTCDate() + days);
  return date.toISOString().slice(0, 10);
}

// The additional documents keep their histories while their net financial
// effects return to the original fixture balances. A separate product makes
// partial purchasing/sales evidence independent of the original stock receipt.
async function seedCommunityFlows({ customer, product, supplier, invoice, today }) {
  let customerInvoice = await api(`/invoices/${invoice.id}`);
  const returnedCustomer = await retried(`/invoices/${invoice.id}/returns`, {
    version: customerInvoice.version, reason: 'CI customer paid return', refund_confirmed: true,
    items: [{ invoice_line_id: customerInvoice.items[0].id, quantity: '1' }],
  });
  assert.equal(Number(returnedCustomer.net_total), 80000);
  assert.equal(Number(returnedCustomer.paid), 80000);
  const customerReturn = returnedCustomer.returns.find(item => item.reason === 'CI customer paid return');
  assert.equal(Number(customerReturn.refund), 80000);
  assert.equal(Number(customerReturn.cost_total), 50000, 'Customer return keeps original shipment cost');
  customerInvoice = await retried(`/sales-returns/${customerReturn.id}/reverse`, {
    version: returnedCustomer.version, reason: 'CI reverses customer return',
  });
  assert.equal(Number(customerInvoice.credit_total), 0);
  assert.equal(Number(customerInvoice.paid), 160000);

  let bill = (await api('/purchase-bills')).items.find(item => item.supplier_invoice_number === 'CI-AP-002');
  bill = await retried(`/purchase-bills/${bill.id}/payments`, {
    version: bill.version, amount: '60000', reference: 'CI supplier partial payment',
  });
  assert.equal(bill.status, 'partial');
  assert.equal(Number(bill.outstanding), 120000);
  await conflict(`/purchase-bills/${bill.id}/payments`, { version: bill.version, amount: '120000.01' });
  bill = await retried(`/purchase-bills/${bill.id}/payments`, {
    version: bill.version, amount: '120000', reference: 'CI supplier final payment',
  });
  assert.equal(bill.status, 'paid');
  const returnedSupplier = await retried(`/purchase-bills/${bill.id}/returns`, {
    version: bill.version, reason: 'CI paid supplier return', refund_confirmed: true,
    items: [{ bill_line_id: bill.items[0].id, quantity: '1' }],
  });
  assert.equal(Number(returnedSupplier.paid), 120000);
  const supplierReturn = returnedSupplier.returns.find(item => item.reason === 'CI paid supplier return');
  assert.equal(Number(supplierReturn.refund), 60000);
  bill = await retried(`/supplier-returns/${supplierReturn.id}/reverse`, {
    version: returnedSupplier.version, reason: 'CI reverses supplier return',
  });
  for (const reference of ['CI supplier final payment', 'CI supplier partial payment']) {
    const payment = bill.payments.find(item => item.reference === reference);
    bill = await retried(`/supplier-payments/${payment.id}/reverse`, {
      version: bill.version, reason: `CI reverses ${reference}`,
    });
  }
  assert.equal(bill.status, 'unpaid');
  assert.equal(Number(bill.paid), 0);
  assert.equal(Number(bill.outstanding), 180000);

  const partialProduct = await api('/products', 'POST', { sku: 'CI-PARTIAL', name: 'CI Partial Flow Product', unit: 'pcs', price: '80000' });
  let purchase = await retried('/purchase-orders', {
    supplier_id: supplier.id, notes: 'CI partial receiving and billing',
    items: [{ product_id: partialProduct.id, quantity: '3', unit_cost: '60000' }],
  });
  purchase = await retried(`/purchase-orders/${purchase.id}/confirm`, { version: purchase.version });
  purchase = await retried(`/purchase-orders/${purchase.id}/receive`, {
    version: purchase.version, reference: 'CI partial receipt one',
    items: [{ order_line_id: purchase.items[0].id, quantity: '1' }],
  });
  assert.equal(purchase.status, 'partial_received');
  const firstReceipt = purchase.receipts.find(item => item.reference === 'CI partial receipt one');
  assert.equal(Number(firstReceipt.items[0].quantity), 1);
  const newBills = [];
  for (const [index, quantity] of ['0.1', '0.2', '0.3', '0.4'].entries()) {
    purchase = await api(`/purchase-orders/${purchase.id}`);
    const newBill = await retried('/purchase-bills', {
      receipt_id: firstReceipt.id, order_version: purchase.version,
      supplier_invoice_number: `CI-PARTIAL-AP-${index + 1}`, invoice_date: shiftDate(today, -100),
      due_date: shiftDate(today, -[10, 40, 70, 100][index]), notes: 'CI split supplier bill',
      items: [{ receipt_line_id: firstReceipt.items[0].id, quantity }],
    });
    assert.equal(Number(newBill.items[0].quantity), Number(quantity));
    newBills.push(newBill);
  }
  purchase = await api(`/purchase-orders/${purchase.id}`);
  purchase = await retried(`/purchase-orders/${purchase.id}/receive`, {
    version: purchase.version, reference: 'CI partial receipt remainder',
    items: [{ order_line_id: purchase.items[0].id, quantity: '2' }],
  });
  assert.equal(purchase.status, 'received');
  const lastReceipt = purchase.receipts.find(item => item.reference === 'CI partial receipt remainder');
  const finalBill = await retried('/purchase-bills', {
    receipt_id: lastReceipt.id, order_version: purchase.version, supplier_invoice_number: 'CI-PARTIAL-AP-5',
    invoice_date: today, due_date: shiftDate(today, 7), notes: 'CI remainder supplier bill',
  });
  newBills.push(finalBill);
  const aging = await api('/purchase-bills/analysis?months=6');
  assert.deepEqual(Object.fromEntries(aging.aging.map(item => [item.key, Number(item.total)])), {
    current: 300000, days_1_30: 6000, days_31_60: 12000, days_61_90: 18000, over_90: 24000,
  }, 'Aging includes all current/late ranges across split bills');

  let partialOrder = await retried('/orders', {
    customer_id: customer.id, items: [{ product_id: partialProduct.id, quantity: '3', unit_price: '80000' }],
  });
  partialOrder = await retried(`/orders/${partialOrder.id}/confirm`, {});
  partialOrder = await retried(`/orders/${partialOrder.id}/ship`, {
    version: partialOrder.version, items: [{ line_id: partialOrder.items[0].id, quantity: '1' }],
  });
  assert.equal(Number(partialOrder.items[0].unshipped_quantity), 2);
  const splitInvoices = [];
  for (const quantity of ['0.4', '0.6']) {
    partialOrder = await api(`/orders/${partialOrder.id}`);
    const splitInvoice = await retried(`/orders/${partialOrder.id}/invoice`, {
      version: partialOrder.version, items: [{ line_id: partialOrder.items[0].id, quantity }],
    });
    assert.equal(Number(splitInvoice.items[0].quantity), Number(quantity));
    splitInvoices.push(splitInvoice);
  }
  partialOrder = await api(`/orders/${partialOrder.id}`);
  await conflict(`/orders/${partialOrder.id}/invoice`, { version: partialOrder.version, items: [{ line_id: partialOrder.items[0].id, quantity: '0.001' }] });
  partialOrder = await retried(`/orders/${partialOrder.id}/ship`, {
    version: partialOrder.version, items: [{ line_id: partialOrder.items[0].id, quantity: '2' }],
  });
  splitInvoices.push(await retried(`/orders/${partialOrder.id}/invoice`, {
    version: partialOrder.version, items: [{ line_id: partialOrder.items[0].id, quantity: '2' }],
  }));
  assert.equal(splitInvoices.reduce((sum, item) => sum + Number(item.total), 0), 240000);
  partialOrder = await api(`/orders/${partialOrder.id}`);
  assert.equal(partialOrder.status, 'invoiced');
  for (const splitInvoice of splitInvoices) {
    await retried(`/invoices/${splitInvoice.id}/reverse`, { reason: 'CI reverses split sales invoice' });
  }
  partialOrder = await api(`/orders/${partialOrder.id}`);
  for (const shipment of [...partialOrder.shipments].reverse()) {
    partialOrder = await retried(`/orders/${partialOrder.id}/shipments/${shipment.id}/reverse`, {
      version: partialOrder.version, reason: 'CI reverses partial delivery',
    });
  }
  await retried(`/orders/${partialOrder.id}/cancel`, { reason: 'CI completed partial sales correction proof' });
  let firstBill = await api(`/purchase-bills/${newBills[0].id}`);
  firstBill = await retried(`/purchase-bills/${firstBill.id}/payments`, {
    version: firstBill.version, amount: '6000', reference: 'CI split bill payment before return',
  });
  for (const candidate of newBills) {
    const current = await api(`/purchase-bills/${candidate.id}`);
    const returned = await retried(`/purchase-bills/${candidate.id}/returns`, {
      version: current.version, reason: 'CI fully returns partial purchase', refund_confirmed: true,
      items: [{ bill_line_id: current.items[0].id, quantity: current.items[0].quantity }],
    });
    assert.equal(Number(returned.net_total), 0);
    assert.equal(Number(returned.paid), 0);
    assert.equal(returned.status, 'paid');
  }
  const afterProducts = (await api('/products')).items;
  assert.equal(Number(afterProducts.find(item => item.id === partialProduct.id).quantity), 0);
  assert.equal(Number(afterProducts.find(item => item.id === partialProduct.id).inventory_value), 0);
  assert.equal(Number(afterProducts.find(item => item.id === product.id).quantity), 14);
  const opening = await api('/accounting/opening');
  const activeOpening = opening.document.history.find(item => item.reversed_at === null);
  await retried(`/accounting/reports/cash-flow/${activeOpening.journal_id}`, {
    category: 'financing', version: 0, reason: 'CI confirms owner opening cash funding',
  }, 'PUT');
}

async function verifyCommunityFlows({ bills, purchases, orders, movements, opening }) {
  const products = (await api('/products')).items;
  const partialProduct = products.find(item => item.sku === 'CI-PARTIAL');
  assert.ok(partialProduct);
  assert.equal(Number(partialProduct.quantity), 0);
  assert.equal(Number(partialProduct.inventory_value), 0);
  const partialOrder = orders.find(item => item.items.some(line => line.product_id === partialProduct.id));
  assert.ok(partialOrder);
  assert.equal(partialOrder.status, 'cancelled');
  assert.equal(partialOrder.shipments.length, 2);
  assert.ok(partialOrder.shipments.every(item => item.reversed && item.reversal_reason === 'CI reverses partial delivery'));
  assert.equal(partialOrder.invoices.length, 3);
  assert.ok(partialOrder.invoices.every(item => item.status === 'void'));
  assert.equal(partialOrder.invoices.reduce((sum, item) => sum + Number(item.total), 0), 240000);
  assert.deepEqual(partialOrder.invoices.map(item => Number(item.items[0].quantity)).sort((a, b) => a - b), [0.4, 0.6, 2]);
  assert.equal(Number(partialOrder.items[0].shipped_quantity), 0);
  assert.equal(Number(partialOrder.items[0].invoiced_quantity), 0);
  const originalOrder = orders.find(item => item.number === 'SO-000001');
  const customerInvoice = await api(`/invoices/${originalOrder.invoice_id}`);
  assert.equal(customerInvoice.returns.length, 1);
  assert.equal(customerInvoice.returns[0].reason, 'CI customer paid return');
  assert.equal(customerInvoice.returns[0].reversal.reason, 'CI reverses customer return');
  assert.equal(Number(customerInvoice.credit_total), 0);
  assert.equal(Number(customerInvoice.paid), 160000);
  assert.deepEqual(await api(`/sales-returns/${customerInvoice.returns[0].id}`), customerInvoice.returns[0]);
  const baselineBill = bills.find(item => item.supplier_invoice_number === 'CI-AP-002');
  assert.equal(baselineBill.payments.length, 2);
  assert.ok(baselineBill.payments.every(item => item.reversal && item.reversal.reason.startsWith('CI reverses CI supplier')));
  assert.equal(baselineBill.returns.length, 1);
  assert.equal(baselineBill.returns[0].reversal.reason, 'CI reverses supplier return');
  assert.equal(Number(baselineBill.credit_total), 0);
  const splitBills = bills.filter(item => item.supplier_invoice_number.startsWith('CI-PARTIAL-AP-'));
  assert.equal(splitBills.length, 5);
  assert.ok(splitBills.every(item => item.status === 'paid' && Number(item.net_total) === 0 && Number(item.paid) === 0));
  assert.equal(splitBills.reduce((sum, item) => sum + Number(item.credit_total), 0), 180000);
  assert.ok(splitBills.every(item => item.returns.length === 1 && !item.returns[0].reversal));
  assert.equal(splitBills.reduce((sum, item) => sum + Number(item.returns[0].refund), 0), 6000);
  assert.equal(splitBills.reduce((sum, item) => sum + Number(item.returns[0].inventory_value), 0), 180000);
  const paidSplit = splitBills.find(item => item.supplier_invoice_number === 'CI-PARTIAL-AP-1');
  assert.equal(paidSplit.payments.length, 1);
  assert.equal(paidSplit.payments[0].reference, 'CI split bill payment before return');
  assert.equal(Number(paidSplit.payments[0].amount), 6000);
  assert.equal(paidSplit.payments[0].reversal, null);
  assert.deepEqual(await api(`/supplier-returns/${paidSplit.returns[0].id}`), paidSplit.returns[0]);
  const partialPurchase = purchases.find(item => item.notes === 'CI partial receiving and billing');
  assert.ok(partialPurchase);
  assert.equal(partialPurchase.status, 'received');
  assert.equal(partialPurchase.receipts.length, 2);
  assert.equal(partialPurchase.receipts.reduce((sum, item) => sum + Number(item.items[0].quantity), 0), 3);
  assert.ok(partialPurchase.receipts.every(item => !item.reversal && !item.can_bill));
  assert.equal(movements.filter(item => item.kind === 'sales_return').length, 1);
  assert.equal(movements.filter(item => item.kind === 'sales_return_reversal').length, 1);
  assert.equal(movements.filter(item => item.kind === 'supplier_return').length, 6);
  assert.equal(movements.filter(item => item.kind === 'supplier_return_reversal').length, 1);
  assert.ok(movements.filter(item => item.kind.startsWith('sales_return')).every(item => item.sales_return_id && item.sales_return_invoice_id));
  assert.ok(movements.filter(item => item.kind.startsWith('supplier_return')).every(item => item.supplier_return_id && item.supplier_return_bill_id));
  const analysis = await api('/purchase-bills/analysis?months=6');
  assert.equal(analysis.series.length, 6);
  assert.deepEqual(Object.fromEntries(analysis.aging.map(item => [item.key, Number(item.total)])), {
    current: 180000, days_1_30: 0, days_31_60: 0, days_61_90: 0, over_90: 0,
  });
  assert.equal(analysis.bills.length, 1);
  assert.equal(analysis.bills[0].id, baselineBill.id);
  assert.equal(analysis.series.reduce((sum, item) => sum + Number(item.paid), 0), 246000);
  assert.equal(analysis.series.reduce((sum, item) => sum + Number(item.reversed), 0), 246000);
  assert.equal(analysis.series.reduce((sum, item) => sum + Number(item.net), 0), 0);
  const reports = await api(`/accounting/reports?from=2000-01-01&to=${analysis.today}`);
  assert.equal(Number(reports.income_statement.revenue_total), 170000);
  assert.equal(Number(reports.income_statement.expense_total), 100000);
  assert.equal(Number(reports.income_statement.net_profit), 70000);
  assert.equal(reports.balance_sheet.balanced, true);
  assert.equal(Number(reports.balance_sheet.assets_total), 1030000);
  assert.equal(Number(reports.balance_sheet.liabilities_total), 180000);
  assert.equal(Number(reports.balance_sheet.equity_total), 780000);
  assert.equal(Number(reports.balance_sheet.retained_earnings), 70000);
  assert.equal(Number(reports.balance_sheet.difference), 0);
  assert.equal(reports.cash_flow.reconciled, true);
  assert.equal(Number(reports.cash_flow.beginning_balance), 0);
  assert.equal(Number(reports.cash_flow.ending_balance), 270000);
  assert.equal(Number(reports.cash_flow.cash_in), 697000);
  assert.equal(Number(reports.cash_flow.cash_out), 427000);
  assert.equal(Number(reports.cash_flow.difference), 0);
  const activeOpening = opening.document.history.find(item => item.reversed_at === null);
  const classified = reports.cash_flow.entries.find(item => item.journal_id === activeOpening.journal_id);
  assert.ok(classified);
  assert.equal(classified.category, 'financing');
  assert.equal(classified.classification.version, 1);
  assert.equal(classified.classification.reason, 'CI confirms owner opening cash funding');
  assert.equal(classified.classification.history.length, 1, 'Idempotent classification retains one audit event through restore');
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
  const supplier = await api('/suppliers', 'POST', {
    code: 'CI-SUP', name: 'CI Supplier', email: 'supplier@example.test',
    idempotency_key: crypto.randomUUID(),
  });
  const purchaseInput = {
    supplier_id: supplier.id, expected_date: '2027-01-15', notes: 'CI purchase planning',
    items: [{ product_id: product.id, quantity: '2', unit_cost: '60000' }],
    idempotency_key: crypto.randomUUID(),
  };
  const purchase = await api('/purchase-orders', 'POST', purchaseInput);
  assert.deepEqual(await api('/purchase-orders', 'POST', purchaseInput), purchase, 'PO retry cannot duplicate documents');
  const editedPurchase = await api(`/purchase-orders/${purchase.id}`, 'PUT', {
    ...purchaseInput, version: purchase.version,
    items: [{ product_id: product.id, quantity: '2.5', unit_cost: '60000.1234' }],
    idempotency_key: crypto.randomUUID(),
  });
  assert.equal(editedPurchase.total, '150000.31', 'Purchase costs round to cents after multiplying');
  const confirmedPurchase = await api(`/purchase-orders/${purchase.id}/confirm`, 'POST', {
    version: editedPurchase.version, idempotency_key: crypto.randomUUID(),
  });
  await api(`/purchase-orders/${purchase.id}/cancel`, 'POST', {
    version: confirmedPurchase.version, reason: 'CI supplier changed delivery terms', idempotency_key: crypto.randomUUID(),
  });
  const reviewedPurchase = await api('/purchase-orders', 'POST', {
    ...purchaseInput, notes: 'CI reviewed purchase',
    items: [{ product_id: product.id, quantity: '3', unit_cost: '60000' }],
    idempotency_key: crypto.randomUUID(),
  });
  const readyPurchase = await api(`/purchase-orders/${reviewedPurchase.id}/confirm`, 'POST', {
    version: reviewedPurchase.version, idempotency_key: crypto.randomUUID(),
  });
  const receiveInput = { version: readyPurchase.version, reference: 'CI goods receipt before review', idempotency_key: crypto.randomUUID() };
  const firstReceived = await api(`/purchase-orders/${reviewedPurchase.id}/receive`, 'POST', receiveInput);
  assert.deepEqual(await api(`/purchase-orders/${reviewedPurchase.id}/receive`, 'POST', receiveInput), firstReceived, 'Retry cannot duplicate PO goods receipts');
  const purchaseCorrection = { version: firstReceived.version, reason: 'CI checks PO receipt correction', idempotency_key: crypto.randomUUID() };
  const receiptReversedPurchase = await api(`/purchase-receipts/${firstReceived.active_receipt_id}/reverse`, 'POST', purchaseCorrection);
  assert.deepEqual(await api(`/purchase-receipts/${firstReceived.active_receipt_id}/reverse`, 'POST', purchaseCorrection), receiptReversedPurchase);
  assert.equal(receiptReversedPurchase.status, 'confirmed');
  const billedPurchase = await api(`/purchase-orders/${reviewedPurchase.id}/receive`, 'POST', {
    version: receiptReversedPurchase.version, reference: 'CI reviewed goods receipt', idempotency_key: crypto.randomUUID(),
  });
  const { today } = await api('/purchase-bills/summary');
  const due = new Date(`${today}T12:00:00Z`);
  due.setUTCDate(due.getUTCDate() + 7);
  const billInput = {
    receipt_id: billedPurchase.active_receipt_id, order_version: billedPurchase.version,
    supplier_invoice_number: 'CI-AP-001', invoice_date: today, due_date: due.toISOString().slice(0, 10),
    notes: 'CI supplier invoice before review', idempotency_key: crypto.randomUUID(),
  };
  const firstBill = await api('/purchase-bills', 'POST', billInput);
  assert.deepEqual(await api('/purchase-bills', 'POST', billInput), firstBill, 'Retry cannot duplicate supplier bills');
  const billCorrection = { version: firstBill.version, reason: 'CI corrects supplier invoice reference', idempotency_key: crypto.randomUUID() };
  const reversedBill = await api(`/purchase-bills/${firstBill.id}/reverse`, 'POST', billCorrection);
  assert.deepEqual(await api(`/purchase-bills/${firstBill.id}/reverse`, 'POST', billCorrection), reversedBill);
  const refreshedPurchase = await api(`/purchase-orders/${billedPurchase.id}`);
  await api('/purchase-bills', 'POST', {
    ...billInput, order_version: refreshedPurchase.version, supplier_invoice_number: 'CI-AP-002',
    notes: 'CI reviewed supplier invoice', idempotency_key: crypto.randomUUID(),
  });
  await seedCommunityFlows({ customer, product, supplier, invoice, today });
} else {
  await login();
}

if (mode === 'mutate') {
  await api('/customers', 'POST', { name: 'Must disappear after backup restore' });
  const supplier = (await api('/suppliers')).items[0];
  await api(`/suppliers/${supplier.id}`, 'PUT', {
    code: supplier.code, name: 'Must disappear after backup restore', email: supplier.email,
    phone: supplier.phone, address: supplier.address, active: supplier.active,
    version: supplier.version, idempotency_key: crypto.randomUUID(),
  });
  const product = (await api('/products')).items.find(item => item.kind === 'stock');
  await api('/purchase-orders', 'POST', {
    supplier_id: supplier.id, items: [{ product_id: product.id, quantity: '1', unit_cost: '100' }],
    idempotency_key: crypto.randomUUID(),
  });
  const unpaidBill = (await api('/purchase-bills')).items.find(item => item.status === 'unpaid');
  assert.ok(unpaidBill);
  await api(`/purchase-bills/${unpaidBill.id}/payments`, 'POST', {
    version: unpaidBill.version, amount: '1000', reference: 'Must disappear after backup restore', idempotency_key: crypto.randomUUID(),
  });
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
  for (const [key, expected] of Object.entries({ revenue: 170000, outstanding: 0, inventory_value: 760000, cash: 270000, customers_count: 1, products_count: 3, orders_count: 3 })) {
    assert.equal(Number(dashboard[key]), expected, `Dashboard ${key}`);
  }
  const trial = await api('/accounting/trial-balance');
  assert.equal(trial.balanced, true);
  assert.equal(Number(trial.total_debit), 1130000);
  assert.equal(Number(trial.total_credit), 1130000);
  const products = await api('/products');
  assert.equal(Number(products.items.find(product => product.sku === 'CI-001').quantity), 14);
  const service = products.items.find(product => product.sku === 'CI-SERVICE');
  assert.equal(service.kind, 'service');
  assert.equal(Number(service.quantity), 0);
  assert.equal(Number(service.inventory_value), 0);
  const movements = await api('/inventory/movements');
  assert.equal(movements.items.length, 25, 'Receipts, partial deliveries and customer/supplier returns retain their correction history');
  const correctedReceipt = movements.items.find(movement => movement.reference === 'CI wrong receipt');
  assert.equal(correctedReceipt.reversal.reason, 'CI corrects the receipt quantity and cost');
  assert.equal(correctedReceipt.can_reverse, false);
  const reviewedReceipt = movements.items.find(movement => movement.reference === 'CI reviewed receipt');
  assert.equal(reviewedReceipt.can_reverse, false, 'Later PO receipts block reversal of an earlier manual receipt');
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
  assert.equal(journals.items.length, 50);
  const correctedJournal = journals.items.find(journal => journal.description === 'CI wrong manual cash');
  assert.equal(correctedJournal.reversal.reason, 'CI removes incorrect manual cash');
  assert.equal(correctedJournal.can_reverse, false);
  const insights = await api('/dashboard/insights?months=6');
  assert.equal(insights.series.length, 6);
  for (const [key, expected] of Object.entries({ revenue: 170000, cogs: 100000, gross_profit: 70000, cash_in: 697000, cash_out: 427000, cash_net: 270000 })) {
    assert.equal(Number(insights.totals[key]), expected, `Chart ${key}`);
  }
  assert.equal(Number(insights.receivables.invoiced), 170000);
  assert.equal(Number(insights.receivables.paid), 170000);
  assert.equal(Number(insights.receivables.outstanding), 0);
  const suppliers = await api('/suppliers');
  assert.equal(suppliers.items.length, 1);
  assert.equal(suppliers.items[0].code, 'CI-SUP');
  assert.equal(suppliers.items[0].name, 'CI Supplier', 'Supplier mutation must disappear after restore');
  const purchases = await api('/purchase-orders');
  assert.equal(purchases.items.length, 3, 'PO creation after backup must disappear after restore');
  const cancelledPurchase = purchases.items.find(purchase => purchase.status === 'cancelled');
  assert.ok(cancelledPurchase);
  assert.equal(cancelledPurchase.total, '150000.31');
  assert.equal(cancelledPurchase.cancellation_reason, 'CI supplier changed delivery terms');
  assert.equal(cancelledPurchase.items[0].unit_cost, '60000.1234');
  const activePurchase = purchases.items.find(purchase => purchase.notes === 'CI reviewed purchase');
  assert.ok(activePurchase);
  assert.equal(activePurchase.total, '180000.00');
  assert.equal(activePurchase.supplier_name, 'CI Supplier');
  assert.equal(activePurchase.expected_date, '2027-01-15');
  assert.equal(activePurchase.receipts.length, 2);
  assert.equal(activePurchase.receipts.filter(receipt => receipt.reversal === null).length, 1);
  const firstGoodsReceipt = activePurchase.receipts.find(receipt => receipt.reversal !== null);
  assert.equal(firstGoodsReceipt.reversal.reason, 'CI checks PO receipt correction');
  assert.equal(firstGoodsReceipt.can_reverse, false);
  const currentGoodsReceipt = activePurchase.receipts.find(receipt => receipt.id === activePurchase.active_receipt_id);
  assert.ok(currentGoodsReceipt);
  assert.equal(currentGoodsReceipt.reference, 'CI reviewed goods receipt');
  assert.equal(currentGoodsReceipt.can_reverse, false, 'Active supplier bill blocks reversal of its receipt');
  assert.equal(currentGoodsReceipt.can_bill, false);
  assert.equal(currentGoodsReceipt.total, '180000.00');
  const accounts = await api('/accounting/accounts');
  const accrual = accounts.items.find(account => account.system_role === 'purchase_receipt_accrual');
  assert.ok(accrual);
  assert.equal(accrual.kind, 'liability');
  assert.equal(accrual.is_control, true);
  const accrualTrial = trial.items.find(account => account.code === accrual.code);
  assert.ok(accrualTrial);
  assert.equal(Number(accrualTrial.balance), 0);
  const payable = accounts.items.find(account => account.system_role === 'purchase_payable');
  assert.ok(payable);
  assert.equal(payable.kind, 'liability');
  assert.equal(payable.is_control, true);
  assert.equal(Number(trial.items.find(account => account.code === payable.code).credit), 180000);
  const bills = (await api('/purchase-bills')).items;
  assert.equal(bills.length, 7);
  const activeBill = bills.find(bill => bill.status === 'unpaid');
  const voidBill = bills.find(bill => bill.status === 'void');
  assert.ok(activeBill);
  assert.ok(voidBill);
  assert.equal(activeBill.supplier_invoice_number, 'CI-AP-002');
  assert.equal(activeBill.total, '180000.00');
  assert.equal(activeBill.outstanding, '180000.00');
  assert.equal(activeBill.paid, '0.00');
  assert.equal(activeBill.receipt_id, currentGoodsReceipt.id);
  assert.equal(currentGoodsReceipt.bill_id, activeBill.id);
  assert.equal(currentGoodsReceipt.bill_number, activeBill.number);
  assert.equal(voidBill.reversal.reason, 'CI corrects supplier invoice reference');
  assert.equal(voidBill.outstanding, '0.00');
  assert.deepEqual(await api(`/purchase-bills/${activeBill.id}`), activeBill);
  const billJournal = await api(`/accounting/journals/${activeBill.journal_id}`);
  assert.equal(billJournal.source_type, 'purchase_bill');
  assert.equal(Number(billJournal.lines.find(line => line.account_code === accrual.code).debit), 180000);
  assert.equal(Number(billJournal.lines.find(line => line.account_code === payable.code).credit), 180000);
  const billSummary = await api('/purchase-bills/summary');
  assert.equal(billSummary.counts.unpaid, 1);
  assert.equal(billSummary.counts.void, 1);
  assert.equal(Number(billSummary.outstanding_total), 180000);
  assert.equal(billSummary.suppliers.length, 1);
  assert.equal(Number(billSummary.suppliers[0].outstanding_total), 180000);
  assert.equal(billSummary.suppliers[0].supplier_name, 'CI Supplier');
  assert.equal(Number(billSummary.overdue_total) + Number(billSummary.due_today_total) + Number(billSummary.upcoming_total), 180000);
  const receiptJournal = await api(`/accounting/journals/${currentGoodsReceipt.journal_id}`);
  assert.equal(receiptJournal.source_type, 'purchase_receipt');
  assert.equal(Number(receiptJournal.lines.find(line => line.account_code === '1300').debit), 180000);
  assert.equal(Number(receiptJournal.lines.find(line => line.account_code === accrual.code).credit), 180000);
  const summary = await api('/purchasing/summary');
  assert.deepEqual(summary.counts, { draft: 0, confirmed: 0, partial_received: 0, received: 2, cancelled: 1 });
  assert.equal(Number(summary.pending_receipt_total), 0);
  assert.equal(Number(summary.received_unbilled_total), 0);
  assert.equal(Number(summary.supplier_payable_total), 180000);
  assert.equal(Number(summary.values.received), 360000);
  const purchaseDetail = await api(`/purchase-orders/${activePurchase.id}`);
  assert.deepEqual(purchaseDetail, activePurchase, 'PO detail and list retain the same snapshots after restore');
  await verifyCommunityFlows({ bills, purchases: purchases.items, orders: orders.items, movements: movements.items, opening, insights });
  console.log(`Disposable-instance ${mode} check passed: supplier payments, five aging buckets, customer/supplier returns, partial receiving/delivery/invoices, financial reports, and restored stock/cash/payable balances.`);
}
