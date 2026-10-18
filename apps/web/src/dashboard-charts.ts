export interface InsightAmounts {
  revenue: string;
  cogs: string;
  gross_profit: string;
  cash_in: string;
  cash_out: string;
  cash_net: string;
}

export interface DashboardInsights {
  months: number;
  timezone: string;
  from: string;
  through: string;
  as_of: string;
  series: (InsightAmounts & { month: string })[];
  totals: InsightAmounts;
  receivables: {
    invoiced: string;
    paid: string;
    outstanding: string;
    unpaid_count: number;
    partial_count: number;
    paid_count: number;
  };
}

// Keep currency exact in labels and comparisons. Number conversions below are
// limited to SVG geometry; the server supplies all financial totals.
export function moneyCents(value: string): bigint {
  const match = /^(-?)(\d+)(?:\.(\d{1,2})0*)?$/.exec(value);
  if (!match) throw new Error(`Invalid money amount: ${value}`);
  const cents =
    BigInt(match[2]) * 100n + BigInt((match[3] || "").padEnd(2, "0"));
  return match[1] ? -cents : cents;
}

export function exactMoney(value: string | bigint): string {
  const cents = typeof value === "bigint" ? value : moneyCents(value);
  const absolute = cents < 0n ? -cents : cents;
  const whole = (absolute / 100n).toLocaleString("id-ID");
  const fraction = absolute % 100n;
  return `${cents < 0n ? "−" : ""}Rp${whole}${fraction ? `,${fraction.toString().padStart(2, "0")}` : ""}`;
}

export function compactMoney(cents: bigint): string {
  const absolute = cents < 0n ? -cents : cents;
  const units: [bigint, string][] = [
    [100000000000000n, "T"],
    [100000000000n, "M"],
    [100000000n, "jt"],
    [100000n, "rb"],
  ];
  const unit = units.find(([threshold]) => absolute >= threshold);
  if (!unit) return exactMoney(cents);
  const tenths = (absolute * 10n) / unit[0];
  return `${cents < 0n ? "−" : ""}Rp${tenths / 10n}${tenths % 10n ? `,${tenths % 10n}` : ""} ${unit[1]}`;
}

export function monthLabel(month: string, long = false): string {
  return new Intl.DateTimeFormat("id-ID", {
    month: long ? "long" : "short",
    year: long ? "numeric" : "2-digit",
    timeZone: "UTC",
  }).format(new Date(`${month}-01T12:00:00Z`));
}

export function calendarDate(value: string): string {
  return new Intl.DateTimeFormat("id-ID", {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  }).format(new Date(`${value}T12:00:00Z`));
}

const floorDivide = (a: bigint, b: bigint) =>
  a / b - (a < 0n && a % b !== 0n ? 1n : 0n);
const ceilDivide = (a: bigint, b: bigint) => -floorDivide(-a, b);

export interface MoneyScale {
  min: bigint;
  max: bigint;
  ticks: bigint[];
}

export function moneyScale(values: bigint[]): MoneyScale {
  const min = values.reduce((a, b) => (b < a ? b : a), 0n);
  const max = values.reduce((a, b) => (b > a ? b : a), 0n);
  if (min === 0n && max === 0n)
    return { min: 0n, max: 100n, ticks: [0n, 25n, 50n, 75n, 100n] };
  const desiredStep = ceilDivide(max - min, 4n);
  let magnitude = 1n;
  while (magnitude * 10n < desiredStep) magnitude *= 10n;
  const step = [1n, 2n, 5n, 10n]
    .map((factor) => magnitude * factor)
    .find((candidate) => candidate >= desiredStep)!;
  const low = floorDivide(min, step) * step;
  const high = ceilDivide(max, step) * step;
  const ticks: bigint[] = [];
  for (let tick = low; tick <= high; tick += step) ticks.push(tick);
  return { min: low, max: high, ticks };
}

export function scaleY(value: bigint, scale: MoneyScale, height: number): number {
  return (
    (Number(((scale.max - value) * 1000000n) / (scale.max - scale.min)) /
      1000000) *
    height
  );
}

export function paidShare(paid: string, invoiced: string): number {
  const total = moneyCents(invoiced);
  if (total <= 0n) return 0;
  const settled = moneyCents(paid);
  return Number((settled * 10000n) / total) / 100;
}

export function amountTone(amount: string): "positive" | "negative" | "neutral" {
  const cents = moneyCents(amount);
  return cents > 0n ? "positive" : cents < 0n ? "negative" : "neutral";
}
