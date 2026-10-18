import { describe, expect, test } from "bun:test";
import {
  amountTone,
  calendarDate,
  compactMoney,
  exactMoney,
  moneyCents,
  moneyScale,
  monthLabel,
  paidShare,
  scaleY,
} from "./dashboard-charts";

describe("financial chart presentation", () => {
  test("currency labels retain negative amounts, cents, and large integers", () => {
    expect(moneyCents("-1234567.89")).toBe(-123456789n);
    expect(moneyCents("0.0100")).toBe(1n);
    expect(exactMoney("9007199254740993.01")).toBe(
      "Rp9.007.199.254.740.993,01",
    );
    expect(exactMoney("-1500.50")).toBe("−Rp1.500,50");
    expect(compactMoney(-250000000n)).toBe("−Rp2,5 jt");
    expect(amountTone("-0.01")).toBe("negative");
    expect(amountTone("0.00")).toBe("neutral");
  });

  test("mixed and negative-only chart domains retain a visible zero baseline", () => {
    for (const values of [
      [-200000n, 350000n, 0n],
      [-1n, -3n],
      [900719925474099301n, -1n],
    ]) {
      const scale = moneyScale(values);
      expect(scale.min <= 0n).toBe(true);
      expect(scale.max >= 0n).toBe(true);
      expect(scale.ticks.includes(0n)).toBe(true);
      for (const value of values) {
        expect(scale.min <= value && scale.max >= value).toBe(true);
        expect(scaleY(value, scale, 180)).toBeGreaterThanOrEqual(0);
        expect(scaleY(value, scale, 180)).toBeLessThanOrEqual(180);
      }
      expect(scaleY(scale.max, scale, 180)).toBe(0);
      expect(scaleY(scale.min, scale, 180)).toBe(180);
    }
  });

  test("zero periods and empty receivables do not fabricate financial values", () => {
    const scale = moneyScale([0n, 0n, 0n]);
    expect(scaleY(0n, scale, 180)).toBe(180);
    expect(paidShare("0.00", "0.00")).toBe(0);
    expect(paidShare("500.00", "2000.00")).toBe(25);
    expect(paidShare("1.01", "1.01")).toBe(100);
    expect(paidShare("9007199254740993.01", "9007199254740993.01")).toBe(100);
  });

  test("month and range dates preserve the server's company calendar", () => {
    expect(monthLabel("2026-01", true)).toBe("Januari 2026");
    expect(monthLabel("2025-12", true)).toBe("Desember 2025");
    expect(calendarDate("2026-01-01")).toBe("1 Jan 2026");
  });
});
