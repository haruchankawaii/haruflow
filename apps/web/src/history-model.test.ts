import { expect, test } from "bun:test";
import { emptyHistoryFilters, historyQuery } from "./history-model";
test("history export retains complete server filters and excludes current page", () => {
  const filters = {
    q: "  Toko %_ & A  ",
    status: "paid",
    from: "2026-01-01",
    to: "2026-10-20",
  };
  const list = new URLSearchParams(historyQuery(filters, 12));
  const exported = new URLSearchParams(historyQuery(filters, 12, true));
  expect(list.get("page")).toBe("12");
  expect(list.get("page_size")).toBe("50");
  expect(exported.has("page")).toBe(false);
  expect(exported.has("page_size")).toBe(false);
  for (const key of ["q", "status", "from", "to"])
    expect(exported.get(key)).toBe(list.get(key));
  expect(exported.get("q")).toBe("Toko %_ & A");
  expect(
    new URLSearchParams(historyQuery(emptyHistoryFilters)).get("q"),
  ).toBeNull();
});
