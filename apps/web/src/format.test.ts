import { describe, expect, test } from "bun:test";
import {
  costMoney,
  companyToday,
  date,
  dateTime,
  lineTotal,
  money,
  number,
  orderTotal,
  setCompanyTimezone,
  toMinorUnits,
} from "./format";

describe("decimal previews", () => {
  test("preserves fractional quantities and prices", () => {
    expect(lineTotal("1.5", "1000.25")).toBe("1500.38");
    expect(
      orderTotal([
        { quantity: "0.1", unit_price: "0.2" },
        { quantity: "3", unit_price: "1000.25" },
      ]),
    ).toBe("3000.77");
  });
  test("rounds each line before summing, including exact midpoints", () => {
    expect(
      orderTotal([
        { quantity: "1", unit_price: "0.005" },
        { quantity: "1", unit_price: "0.005" },
      ]),
    ).toBe("0.02");
    expect(lineTotal("0.001", "5")).toBe("0.01");
    expect(lineTotal("0.001", "4.99")).toBe("0.00");
  });
  test("handles large values without integer precision loss", () => {
    expect(toMinorUnits("9007199254740993.1234")).toBe(90071992547409931234n);
  });
  test("rejects invalid decimal previews", () => {
    expect(toMinorUnits("-1")).toBe(0n);
    expect(toMinorUnits("1e3")).toBe(0n);
    expect(toMinorUnits("1.23456")).toBe(0n);
  });
});

test("money preserves cents, cost preserves four decimals, quantity preserves three", () => {
  expect(money("1000.50").replaceAll("\u00a0", " ")).toBe("Rp 1.000,5");
  expect(costMoney("1000.1234").replaceAll("\u00a0", " ")).toBe(
    "Rp 1.000,1234",
  );
  expect(number("1.125")).toBe("1,125");
});

test("dates and period cutoff follow the company's timezone around midnight", () => {
  const instant = "2026-10-18T16:30:00Z";
  try {
    setCompanyTimezone("Asia/Jakarta");
    expect(companyToday(new Date(instant))).toBe("2026-10-18");
    expect(date(instant)).toContain("18");
    setCompanyTimezone("Asia/Jayapura");
    expect(companyToday(new Date(instant))).toBe("2026-10-19");
    expect(date(instant)).toContain("19");
    expect(dateTime(instant)).toContain("01.30");
    expect(dateTime("invalid")).toBe("—");
  } finally {
    setCompanyTimezone("Asia/Jakarta");
  }
});
