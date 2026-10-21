// HTTP-only settlement and restore proof. Run exclusively on a fresh disposable
// database. Database backup/restore and server lifecycle are owned by the caller.
import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';

const mode = process.argv[2];
assert.ok(['seed', 'mutate', 'verify'].includes(mode), 'Use settlements-smoke.mjs seed|mutate|verify');
assert.ok(process.env.HARUFLOW_SETTLEMENT_SMOKE_URL, 'Set HARUFLOW_SETTLEMENT_SMOKE_URL explicitly to a disposable instance');
const url = new URL(process.env.HARUFLOW_SETTLEMENT_SMOKE_URL);
assert.ok(url.protocol === 'http:' && ['127.0.0.1', 'localhost', '[::1]'].includes(url.hostname), 'Settlement smoke requires a loopback HTTP instance');
assert.ok(!url.username && !url.password && !url.search && !url.hash && url.pathname === '/', 'Use an origin URL without credentials, path, query or hash');
const base = url.origin;
const marker = 'CI Disposable Settlement Company';
const email = 'settlements-ci@example.test';
const password = process.env.HARUFLOW_SETTLEMENT_SMOKE_PASSWORD || 'haruflow-settlements-ci-password-2026';
const customerName = 'CI Settlement Customer';
const supplierCode = 'CI-SETTLEMENT-SUPPLIER';
let cookie = '';

async function response(path, method = 'GET', body, authenticated = true) {
  return fetch(`${base}/api${path}`, {
    method,
    headers: {
      'Content-Type': 'application/json', 'X-Haruflow-Request': '1',
      ...(authenticated && cookie ? { Cookie: cookie } : {}),
    },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
    signal: AbortSignal.timeout(15000),
  });
}

async function api(path, method = 'GET', body) {
  const result = await response(path, method, body);
  const text = await result.text();
  assert.ok(result.ok, `${method} ${path}: ${result.status} ${text}`);
  const session = result.headers.get('set-cookie');
  if (session) cookie = session.split(';')[0];
  return JSON.parse(text);
}

async function retried(path, body = {}, method = 'POST') {
  const input = { ...body, idempotency_key: randomUUID() };
  const result = await api(path, method, input);
  assert.deepEqual(await api(path, method, input), result, `${path} must not post twice on retry`);
  return result;
}

async function rejected(path, body, expected = 409, method = 'POST') {
  const result = await response(path, method, { ...body, idempotency_key: randomUUID() });
  assert.equal(result.status, expected, `${method} ${path}: ${await result.text()}`);
}

function cents(value) {
  assert.match(String(value), /^-?\d+(\.\d{1,2})?$/, `Invalid money: ${value}`);
  const [whole, fraction = ''] = String(value).replace(/^-/, '').split('.');
  const amount = BigInt(whole) * 100n + BigInt(fraction.padEnd(2, '0'));
  return String(value).startsWith('-') ? -amount : amount;
}

function money(value, expected, label = 'Amount') {
  assert.equal(cents(value), cents(expected), `${label}: ${value} != ${expected}`);
}

function one(items, predicate, description) {
  const matches = items.filter(predicate);
  assert.equal(matches.length, 1, `Expected exactly one ${description}, found ${matches.length}`);
  return matches[0];
}

function pdf() {
  const objects = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R >>',
    '<< /Length 0 >>\nstream\n\nendstream',
  ];
  let text = '%PDF-1.4\n';
  const offsets = [0];
  for (const [index, object] of objects.entries()) {
    offsets.push(Buffer.byteLength(text));
    text += `${index + 1} 0 obj\n${object}\nendobj\n`;
  }
  const start = Buffer.byteLength(text);
  text += `xref\n0 ${offsets.length}\n0000000000 65535 f \n`;
  text += offsets.slice(1).map(offset => `${String(offset).padStart(10, '0')} 00000 n \n`).join('');
  text += `trailer\n<< /Size ${offsets.length} /Root 1 0 R >>\nstartxref\n${start}\n%%EOF\n`;
  return Buffer.from(text);
}

async function upload(kind, id, filename) {
  const bytes = pdf();
  const path = `/documents/${kind}/${id}/attachments?${new URLSearchParams({ filename, idempotency_key: randomUUID() })}`;
  const send = async () => {
    const result = await fetch(`${base}/api${path}`, {
      method: 'POST', headers: { 'Content-Type': 'application/pdf', 'X-Haruflow-Request': '1', Cookie: cookie },
      body: bytes, signal: AbortSignal.timeout(15000),
    });
    const data = await result.json();
    assert.ok(result.ok, `Upload ${kind}: ${result.status} ${JSON.stringify(data)}`);
    return data;
  };
  const attachment = await send();
  assert.deepEqual(await send(), attachment, 'Attachment retry must not duplicate evidence');
  assert.equal(attachment.sha256, createHash('sha256').update(bytes).digest('hex'));
  assert.equal(attachment.byte_size, bytes.length);
  return attachment;
}

async function parties() {
  return {
    customer: one((await api('/customers')).items, item => item.name === customerName, 'smoke customer'),
    supplier: one((await api('/suppliers')).items, item => item.code === supplierCode, 'smoke supplier'),
  };
}

async function data() {
  const { customer, supplier } = await parties();
  const funds = (await api('/party-funds')).items;
  const targets = (await api('/party-targets?side=customer')).items;
  const ar = one(targets, item => item.number === 'CI-AR-OLD', 'opening AR');
  const invoices = (await api('/invoices')).items;
  const original = one(invoices, item => cents(item.total) === cents('180000'), 'original service invoice');
  const credit = one(funds, item => item.source_type === 'sales_return', 'return credit fund');
  const advance = one(funds, item => item.reference === 'CI-CUSTOMER-AR-SETTLEMENT', 'customer cash advance');
  return { customer, supplier, funds, ar, original, credit, advance };
}

async function proofTargets() {
  const { ar, advance, credit } = await data();
  assert.equal(credit.refunds.length, 1);
  return [['opening_item', ar.id], ['party_fund', advance.id], ['party_refund', credit.refunds[0].id]];
}

async function allocate(fund, target, amount) {
  return retried(`/party-funds/${fund.id}/allocate`, {
    version: fund.version,
    items: [{ target_type: target.target_type || 'invoice', target_id: target.id, version: target.version, amount }],
  });
}

async function invoice(customer, service, quantity, price) {
  const order = await retried('/orders', {
    customer_id: customer.id, items: [{ product_id: service.id, quantity, unit_price: price }],
  });
  await retried(`/orders/${order.id}/confirm`, {});
  return retried(`/orders/${order.id}/invoice`, {});
}

async function seed() {
  assert.equal((await api('/setup/status')).needs_setup, true, 'Seed requires a fresh disposable installation');
  const token = process.env.HARUFLOW_SETTLEMENT_SMOKE_SETUP_TOKEN;
  assert.ok(token, 'Set HARUFLOW_SETTLEMENT_SMOKE_SETUP_TOKEN explicitly');
  await api('/setup', 'POST', { token, company_name: marker, name: 'CI Settlement Administrator', email, password });
  const customer = await api('/customers', 'POST', { name: customerName });
  const supplier = await retried('/suppliers', { code: supplierCode, name: 'CI Settlement Supplier' });
  const csv = `kind;party_id;party;reference;document_date;due_date;debit;credit\r\nreceivable;;${customerName};CI-AR-OLD;2000-01-01;2000-02-01;2000000;0\r\npayable;;${supplierCode};CI-AP-OLD;2000-01-01;2000-02-01;0;3000000\r\ncustomer_credit;;${customerName};CI-CUSTOMER-OLD-CREDIT;2000-01-01;;0;300000\r\nsupplier_credit;;${supplierCode};CI-SUPPLIER-OLD-CREDIT;2000-01-01;;500000;0\r\n`;
  const preview = await api('/accounting/opening/parties/preview', 'POST', { csv, delimiter: 'semicolon' });
  assert.deepEqual(preview.errors, []);
  assert.equal(preview.parties.length, 4);
  assert.equal(preview.parties[0].party_id, customer.id);
  assert.equal(preview.parties[1].party_id, supplier.id);
  const invalid = await api('/accounting/opening/parties/preview', 'POST', {
    csv: csv.replace(';0;3000000', ';3000000;0'), delimiter: 'semicolon',
  });
  assert.ok(invalid.errors.length > 0);
  assert.deepEqual(invalid.parties, [], 'Invalid CSV must not retain valid partial rows');
  assert.equal((await api('/accounting/opening')).document, null, 'Preview must not save or post anything');
  assert.equal((await api('/accounting/journals')).items.length, 0);
  const opening = await api('/accounting/opening', 'PUT', {
    version: 0, description: 'CI settlement migration opening', items: [],
    lines: [{ account_code: '1100', debit: '2000000', credit: '0' }, { account_code: '3000', debit: '0', credit: '1200000' }],
    parties: preview.parties.map(({ kind, party_id, reference, document_date, due_date, amount }) => ({ kind, party_id, reference, document_date, due_date, amount })),
  });
  assert.equal(opening.document.balanced, true);
  await retried('/accounting/opening/post', { version: opening.document.version });
  for (const [side, party, targetType, reference, amount] of [
    ['customer', customer, 'opening_receivable', 'CI-CUSTOMER-AR-SETTLEMENT', '500000'],
    ['supplier', supplier, 'opening_payable', 'CI-SUPPLIER-AP-SETTLEMENT', '1000000'],
  ]) {
    const target = one((await api(`/party-targets?side=${side}&party_id=${party.id}`)).items, item => item.target_type === targetType, `${side} opening target`);
    const advance = await retried('/party-funds', { side, party_id: party.id, amount, reference, description: 'CI partial settlement of old balance' });
    const allocated = await allocate(advance, target, amount);
    money(allocated.available, '0', `${side} advance available`);
    const detail = await api(`/opening-items/${target.id}`);
    money(detail.outstanding, side === 'customer' ? '1500000' : '2000000');
    assert.equal(detail.status, 'partial');
  }
  const service = await api('/products', 'POST', { sku: 'CI-SETTLEMENT-SERVICE', name: 'CI Settlement Service', kind: 'service', unit: 'unit', price: '20000' });
  const original = await invoice(customer, service, '9', '20000');
  const paid = await retried(`/invoices/${original.id}/payments`, { amount: '120000', reference: 'CI-CUSTOMER-PAYMENT' });
  const returned = await retried(`/invoices/${original.id}/returns`, {
    version: paid.version, reason: 'CI deferred refund after partial payment', refund_mode: 'deferred', refund_confirmed: false,
    items: [{ invoice_line_id: paid.items[0].id, quantity: '4' }],
  });
  money(returned.net_total, '100000');
  money(returned.paid, '100000');
  const source = returned.returns[0];
  money(source.credit_total, '80000');
  money(source.refund, '0');
  money(source.settlement_credit, '20000');
  assert.equal(source.refund_mode, 'deferred');
  let credit = await api(`/party-funds/${source.credit_fund_id}`);
  money(credit.reserved, '20000');
  money(credit.available, '0');
  assert.equal(credit.refund_requests.length, 1);
  const statement = await api(`/parties/customer/${customer.id}/statement`);
  money(statement.summary.reserved_refunds, '20000');
  await rejected(`/party-funds/${credit.id}/allocate`, {
    version: credit.version, items: [{ target_type: 'invoice', target_id: original.id, amount: '1', version: returned.version }],
  });
  credit = await retried(`/party-refund-requests/${credit.refund_requests[0].id}/cancel`, { version: credit.version, reason: 'CI customer chooses allocation and smaller refund' });
  money(credit.reserved, '0');
  money(credit.available, '20000');
  const next = await invoice(customer, service, '1', '15000');
  credit = await allocate(credit, next, '15000');
  money(credit.available, '5000');
  credit = await retried(`/party-funds/${credit.id}/refunds`, {
    version: credit.version, amount: '5000', reference: 'CI-FINAL-REFUND', reason: 'CI actual remaining refund', confirmed: true,
  });
  money(credit.available, '0');
  money(credit.refunded, '5000');
  const currentOpening = await api('/accounting/opening');
  await rejected('/accounting/opening/reverse', { version: currentOpening.document.version, reason: 'CI dependent opening must remain posted' });
  await rejected(`/party-funds/${credit.id}/reverse`, { version: credit.version, reason: 'CI return source must reverse through original document' });
  for (const [kind, id] of await proofTargets()) await upload(kind, id, `CI-settlement-${kind}.pdf`);
  await verify();
}

async function verify() {
  assert.equal((await api('/company')).name, marker, 'Refuse an unmarked installation');
  const { customer, supplier, funds, ar, original, credit, advance } = await data();
  assert.equal(funds.length, 5);
  assert.ok(!funds.some(item => item.reference === 'CI-SETTLEMENT-AFTER-BACKUP'), 'Post-backup advance must disappear');
  money(advance.allocated, '500000');
  money(advance.available, '0');
  money(credit.amount, '20000');
  money(credit.allocated, '15000');
  money(credit.refunded, '5000');
  money(credit.reserved, '0');
  money(credit.available, '0');
  assert.equal(credit.refunds.length, 1);
  assert.equal(credit.refunds[0].reference, 'CI-FINAL-REFUND');
  assert.equal(credit.refund_requests.length, 1);
  assert.equal(credit.refund_requests[0].cancelled, true);
  const opening = await api('/accounting/opening');
  assert.equal(opening.document.status, 'posted');
  assert.equal(opening.document.parties.length, 4);
  assert.equal(opening.document.history.length, 1);
  assert.equal(opening.can_reverse, false);
  assert.equal((await api('/orders')).items.length, 2);
  assert.equal((await api('/invoices')).items.length, 2);
  assert.equal(original.returns.length, 1);
  assert.equal(original.terms_history.length, 0, 'Post-backup terms edit must disappear');
  money(original.cash_paid, '120000');
  money(original.credit_transferred, '20000');
  money(original.paid, '100000');
  const next = one((await api('/invoices')).items, item => cents(item.total) === cents('15000'), 'second service invoice');
  assert.equal(next.status, 'paid');
  money(next.cash_paid, '0');
  money(next.allocated_paid, '15000');
  for (const [side, party, outstanding, creditBalance] of [
    ['customer', customer, '1500000', '300000'], ['supplier', supplier, '2000000', '500000'],
  ]) {
    const statement = await api(`/parties/${side}/${party.id}/statement`);
    money(statement.summary.outstanding, outstanding, `${side} outstanding`);
    money(statement.summary.credit_balance, creditBalance, `${side} credits`);
    money(statement.summary.available_credit, creditBalance, `${side} unreserved credits`);
    money(statement.summary.reserved_refunds, '0');
    assert.equal(statement.reconciliation.balanced, true);
    assert.equal(statement.documents.length, 1);
    assert.equal(statement.documents[0].target_type, side === 'customer' ? 'opening_receivable' : 'opening_payable');
    money(one(statement.aging, item => item.key === 'over_90', `${side} aging over 90 days`).total, outstanding);
    const oldCredit = one(funds, item => item.side === side && item.kind === 'opening_credit', `${side} old credit`);
    money(oldCredit.available, creditBalance);
  }
  const detail = await api(`/opening-items/${ar.id}`);
  money(detail.amount, '2000000');
  money(detail.paid, '500000');
  money(detail.outstanding, '1500000');
  const reports = await api('/accounting/reports');
  money(reports.income_statement.revenue_total, '115000');
  money(reports.income_statement.expense_total, '0');
  money(reports.income_statement.net_profit, '115000');
  money(reports.balance_sheet.assets_total, '3615000');
  money(reports.balance_sheet.liabilities_total, '2300000');
  money(reports.balance_sheet.equity_total, '1200000');
  money(reports.balance_sheet.retained_earnings, '115000');
  money(reports.balance_sheet.liabilities_and_equity_total, '3615000');
  assert.equal(reports.balance_sheet.balanced, true);
  money(reports.balance_sheet.difference, '0');
  money(reports.cash_flow.ending_balance, '1615000');
  money(reports.cash_flow.cash_in, '2620000');
  money(reports.cash_flow.cash_out, '1005000');
  assert.equal(reports.cash_flow.reconciled, true);
  const journals = (await api('/accounting/journals')).items;
  assert.equal(journals.length, 11);
  for (const journal of journals) {
    const document = await api(`/accounting/journals/${journal.id}`);
    const debits = document.lines.reduce((sum, line) => sum + cents(line.debit), 0n);
    const credits = document.lines.reduce((sum, line) => sum + cents(line.credit), 0n);
    assert.equal(debits, credits, `Journal ${journal.id} must balance`);
  }
  for (const [kind, id] of await proofTargets()) {
    const list = await api(`/documents/${kind}/${id}/attachments`);
    assert.equal(list.items.length, 1, `${kind} post-backup proof must disappear`);
    assert.equal(list.active_count, 1);
    const proof = one(list.items, item => item.filename === `CI-settlement-${kind}.pdf`, `${kind} proof`);
    assert.equal(proof.archived_at, null, 'Post-backup archive must disappear');
    const download = await fetch(`${base}/api/attachments/${proof.id}/download`, { headers: { Cookie: cookie }, signal: AbortSignal.timeout(15000) });
    assert.equal(download.status, 200);
    assert.match(download.headers.get('content-disposition'), /^attachment;/);
    assert.equal(download.headers.get('x-content-type-options'), 'nosniff');
    const bytes = Buffer.from(await download.arrayBuffer());
    assert.deepEqual(bytes, pdf(), `${kind} proof bytes must survive backup/restore`);
    assert.equal(proof.sha256, createHash('sha256').update(bytes).digest('hex'));
    assert.equal(proof.byte_size, bytes.length);
    const anonymous = await response(`/attachments/${proof.id}/download`, 'GET', undefined, false);
    assert.equal(anonymous.status, 401, `${kind} proof requires authentication`);
  }
  assert.deepEqual((await api('/users')).usage, { active: 1, limit: 10, available: 9 });
}

async function mutate() {
  await verify();
  const { customer, original, advance } = await data();
  await retried('/party-funds', { side: 'customer', party_id: customer.id, amount: '7777', reference: 'CI-SETTLEMENT-AFTER-BACKUP', description: 'This cash and credit must disappear after restore' });
  const list = await api(`/documents/party_fund/${advance.id}/attachments`);
  const proof = one(list.items, item => item.filename === 'CI-settlement-party_fund.pdf', 'proof to archive after backup');
  await retried(`/attachments/${proof.id}/archive`, { version: proof.version, reason: 'CI mutation after settlement backup' });
  await upload('opening_item', (await data()).ar.id, 'CI-settlement-after-backup.pdf');
  await retried(`/invoices/${original.id}/terms`, { version: original.version, due_date: '2099-12-31', reason: 'CI terms mutation after backup' }, 'PATCH');
  const reports = await api('/accounting/reports');
  money(reports.cash_flow.ending_balance, '1622777');
  assert.ok((await api('/party-funds')).items.some(item => item.reference === 'CI-SETTLEMENT-AFTER-BACKUP'));
}

const health = await fetch(`${base}/health/ready`, { signal: AbortSignal.timeout(15000) });
assert.equal(health.status, 200, 'Disposable server and schema must be ready');
if (mode === 'seed') {
  await seed();
} else {
  await api('/auth/login', 'POST', { email, password });
  assert.equal((await api('/company')).name, marker, 'Refuse an unmarked installation');
  if (mode === 'mutate') await mutate();
  else await verify();
}
console.log(`Disposable settlement ${mode} passed: opening AR/AP/credits, partial allocations, deferred return, reservation cancellation, actual refund, balanced reports, and authenticated attachment bytes.`);
