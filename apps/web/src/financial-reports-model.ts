import { moneyCents } from "./dashboard-charts";

export type CashCategory =
  "operating" | "investing" | "financing" | "unclassified";
export const cashCategoryNames: Record<CashCategory, string> = {
  operating: "Operasi",
  investing: "Investasi",
  financing: "Pendanaan",
  unclassified: "Belum terklasifikasi / saldo awal",
};
export interface ReportAccount {
  code: string;
  name: string;
  kind: string;
  amount: string;
}
export interface CashClassification {
  category: Exclude<CashCategory, "unclassified"> | null;
  version: number;
  reason: string;
  updated_at: string;
  updated_by_name: string;
  history: {
    category: string | null;
    previous_category: string | null;
    version: number;
    reason: string;
    created_at: string;
    created_by_name: string;
  }[];
}
export interface CashEntry {
  journal_id: string;
  source_type: string;
  source_id: string;
  original_source_type: string;
  description: string;
  created_at: string;
  cash_in: string;
  cash_out: string;
  net_change: string;
  category: CashCategory;
  classification_reason: string;
  classification: CashClassification | null;
  counterparts: {
    code: string;
    name: string;
    kind: string;
    system_role: string | null;
    debit: string;
    credit: string;
  }[];
}
export interface FinancialReport {
  from: string;
  to: string;
  timezone: string;
  as_of: string;
  company_name: string;
  income_statement: {
    revenue_accounts: ReportAccount[];
    expense_accounts: ReportAccount[];
    revenue_total: string;
    expense_total: string;
    net_profit: string;
  };
  balance_sheet: {
    asset_accounts: ReportAccount[];
    liability_accounts: ReportAccount[];
    equity_accounts: ReportAccount[];
    retained_earnings_accounts: ReportAccount[];
    assets_total: string;
    liabilities_total: string;
    equity_total: string;
    retained_earnings: string;
    liabilities_and_equity_total: string;
    difference: string;
    balanced: boolean;
  };
  cash_flow: {
    account_code: string;
    beginning_balance: string;
    ending_balance: string;
    cash_in: string;
    cash_out: string;
    net_change: string;
    difference: string;
    reconciled: boolean;
    categories: {
      key: CashCategory;
      name: string;
      cash_in: string;
      cash_out: string;
      net_change: string;
    }[];
    entries: CashEntry[];
  };
}

export function validReportDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const [year, month, day] = value.split("-").map(Number);
  if (year < 1 || month < 1 || month > 12 || day < 1) return false;
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  return (
    day <=
    [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1]
  );
}
export function reportRangeError(from: string, to: string): string {
  if (!validReportDate(from) || !validReportDate(to))
    return "Isi tanggal awal dan akhir yang valid.";
  return from > to
    ? "Tanggal awal harus sebelum atau sama dengan tanggal akhir."
    : "";
}

// Only ratios become numbers for presentation; all labels and arithmetic stay exact.
export function reportBarWidth(value: string, values: string[]): number {
  const absolute = (n: bigint) => (n < 0n ? -n : n);
  const maximum = values.reduce((max, item) => {
    const v = absolute(moneyCents(item));
    return v > max ? v : max;
  }, 0n);
  return maximum === 0n
    ? 0
    : Number((absolute(moneyCents(value)) * 10000n) / maximum) / 100;
}

export function financialReportCsv(report: FinancialReport): unknown[][] {
  const income = report.income_statement,
    balance = report.balance_sheet,
    cash = report.cash_flow;
  const rows: unknown[][] = [
    ["Informasi", "Perusahaan", "", report.company_name],
    ["Informasi", "Dari", "", report.from],
    ["Informasi", "Sampai (inklusif)", "", report.to],
    ["Informasi", "Zona waktu", "", report.timezone],
    ["Informasi", "Snapshot", "", report.as_of],
    ["Informasi", "Mata uang", "", "IDR"],
  ];
  function accounts(section: string, list: ReportAccount[]) {
    list.forEach((a) => rows.push([section, a.name, a.code, a.amount]));
  }
  accounts("Laba rugi / pendapatan", income.revenue_accounts);
  accounts("Laba rugi / beban", income.expense_accounts);
  rows.push(
    ["Laba rugi", "Total pendapatan", "", income.revenue_total],
    ["Laba rugi", "Total beban", "", income.expense_total],
    ["Laba rugi", "Laba / rugi bersih", "", income.net_profit],
  );
  accounts("Neraca / aset", balance.asset_accounts);
  accounts("Neraca / liabilitas", balance.liability_accounts);
  accounts("Neraca / ekuitas", balance.equity_accounts);
  accounts("Rincian laba akumulasi", balance.retained_earnings_accounts);
  rows.push(
    ["Neraca", "Total aset", "", balance.assets_total],
    ["Neraca", "Total liabilitas", "", balance.liabilities_total],
    ["Neraca", "Total ekuitas akun", "", balance.equity_total],
    ["Neraca", "Laba / rugi akumulasi", "", balance.retained_earnings],
    [
      "Neraca",
      "Total liabilitas dan ekuitas termasuk laba",
      "",
      balance.liabilities_and_equity_total,
    ],
    ["Neraca", "Selisih", "", balance.difference],
  );
  rows.push(["Arus kas dan bank", "Saldo awal", cash.account_code, cash.beginning_balance]);
  cash.categories.forEach((c) =>
    rows.push(
      ["Arus kas", `${c.name} / masuk`, cash.account_code, c.cash_in],
      ["Arus kas", `${c.name} / keluar`, cash.account_code, c.cash_out],
      ["Arus kas", `${c.name} / neto`, cash.account_code, c.net_change],
    ),
  );
  rows.push(
    ["Arus kas dan bank", "Total masuk", cash.account_code, cash.cash_in],
    ["Arus kas dan bank", "Total keluar", cash.account_code, cash.cash_out],
    ["Arus kas dan bank", "Perubahan neto", cash.account_code, cash.net_change],
    ["Arus kas dan bank", "Saldo akhir", cash.account_code, cash.ending_balance],
    ["Arus kas dan bank", "Selisih rekonsiliasi", cash.account_code, cash.difference],
  );
  cash.entries.forEach((e) => {
    rows.push([
      "Sumber arus kas / masuk",
      e.description,
      e.journal_id,
      e.cash_in,
      e.created_at,
      cashCategoryNames[e.category],
      e.source_type,
      e.classification_reason,
      e.classification?.reason ?? "",
    ]);
    rows.push([
      "Sumber arus kas / keluar",
      e.description,
      e.journal_id,
      e.cash_out,
      e.created_at,
      cashCategoryNames[e.category],
      e.source_type,
      e.classification_reason,
      e.classification?.reason ?? "",
    ]);
    e.classification?.history.forEach((h) =>
      rows.push([
        "Riwayat klasifikasi",
        h.reason,
        e.journal_id,
        "",
        h.created_at,
        h.category ?? "Otomatis",
        `Versi ${h.version}`,
        h.created_by_name,
      ]),
    );
  });
  return rows;
}
