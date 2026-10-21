import { moneyCents } from "./dashboard-charts";

export function documentSettlement(
  settled: string,
  payments: { amount: string; reversed?: boolean; reversal?: unknown }[],
  returns: { refund: string; settlement_credit?: string; reversal?: unknown }[],
) {
  const activeReturns = returns.filter((row) => !row.reversal);
  const cash =
    payments
      .filter((row) => !row.reversed && !row.reversal)
      .reduce((sum, row) => sum + moneyCents(row.amount), 0n) -
    activeReturns.reduce((sum, row) => sum + moneyCents(row.refund), 0n);
  const transferred = activeReturns.reduce(
    (sum, row) => sum + moneyCents(row.settlement_credit ?? "0"),
    0n,
  );
  return {
    cash,
    transferred,
    allocated: moneyCents(settled) - cash + transferred,
    settled: moneyCents(settled),
  };
}
