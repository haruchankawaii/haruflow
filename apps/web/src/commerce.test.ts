import { describe, expect, test } from "bun:test";
import {
  isStockProduct,
  nextOrderAction,
  orderHasStockItems,
} from "./commerce";
import type { OrderLine } from "./types";

const line = (product_kind?: "stock" | "service"): OrderLine => ({
  product_id: "product",
  product_kind,
  quantity: "1",
  unit_price: "50000",
});

describe("goods and service fulfillment", () => {
  test("services bypass inventory while existing catalog entries remain stock", () => {
    expect(isStockProduct({ kind: "service" })).toBe(false);
    expect(isStockProduct({ kind: "stock" })).toBe(true);
    expect(isStockProduct({})).toBe(true);
    expect(orderHasStockItems({ items: [line("service")] })).toBe(false);
    expect(
      orderHasStockItems({ items: [line("service"), line("stock")] }),
    ).toBe(true);
    expect(orderHasStockItems({ items: [line()] })).toBe(true);
    expect(orderHasStockItems({})).toBe(true);
  });
  test("server fulfillment indicator remains authoritative", () => {
    expect(orderHasStockItems({ has_stock_items: false })).toBe(false);
    expect(
      orderHasStockItems({ has_stock_items: true, items: [line("service")] }),
    ).toBe(true);
  });
  test("pure service orders can be invoiced after confirmation by finance", () => {
    const order = { status: "confirmed", has_stock_items: false };
    expect(nextOrderAction(order, "finance")).toBe("invoice");
    expect(nextOrderAction(order, "admin")).toBe("invoice");
    expect(nextOrderAction(order, "warehouse")).toBeNull();
    expect(nextOrderAction(order, "sales")).toBeNull();
  });
  test("goods and mixed orders require shipment before invoice", () => {
    const order = {
      status: "confirmed",
      items: [line("stock"), line("service")],
    };
    expect(nextOrderAction(order, "warehouse")).toBe("ship");
    expect(nextOrderAction(order, "finance")).toBeNull();
    expect(nextOrderAction({ ...order, status: "shipped" }, "finance")).toBe(
      "invoice",
    );
    expect(
      nextOrderAction({ ...order, status: "shipped" }, "warehouse"),
    ).toBeNull();
  });
  test("drafts require confirmation and closed orders have no next action", () => {
    expect(
      nextOrderAction({ status: "draft", has_stock_items: false }, "sales"),
    ).toBe("confirm");
    expect(
      nextOrderAction({ status: "draft", has_stock_items: false }, "finance"),
    ).toBeNull();
    for (const status of ["invoiced", "cancelled"])
      expect(
        nextOrderAction({ status, has_stock_items: false }, "admin"),
      ).toBeNull();
  });
  test("partial progress permits another delivery and an invoice independently", () => {
    const partial = { status: "confirmed", can_ship: true, can_invoice: true };
    expect(nextOrderAction(partial, "warehouse")).toBe("ship");
    expect(nextOrderAction(partial, "finance")).toBe("invoice");
    expect(nextOrderAction(partial, "viewer")).toBeNull();
    expect(nextOrderAction({ ...partial, can_ship: false }, "admin")).toBe(
      "invoice",
    );
    expect(
      nextOrderAction(
        { ...partial, can_ship: false, can_invoice: false },
        "admin",
      ),
    ).toBeNull();
  });
});
