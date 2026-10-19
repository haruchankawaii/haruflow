import type { Order, Product, Role } from "./types";

// Existing catalog entries without a kind remain stock products.
export const isStockProduct = (product: Pick<Product, "kind">) =>
  product.kind !== "service";

export function orderHasStockItems(
  order: Pick<Order, "has_stock_items" | "items">,
) {
  if (typeof order.has_stock_items === "boolean") return order.has_stock_items;
  // A response without lines cannot prove that the order contains only services.
  return (
    !order.items?.length ||
    order.items.some((item) => item.product_kind !== "service")
  );
}

export function nextOrderAction(
  order: Pick<
    Order,
    "status" | "has_stock_items" | "items" | "can_ship" | "can_invoice"
  >,
  role: Role,
): "confirm" | "ship" | "invoice" | null {
  if (order.status === "draft" && ["admin", "sales"].includes(role))
    return "confirm";
  if (
    typeof order.can_ship === "boolean" ||
    typeof order.can_invoice === "boolean"
  ) {
    if (order.can_ship && ["admin", "warehouse"].includes(role)) return "ship";
    if (order.can_invoice && ["admin", "finance"].includes(role))
      return "invoice";
    return null;
  }
  if (
    order.status === "confirmed" &&
    orderHasStockItems(order) &&
    ["admin", "warehouse"].includes(role)
  )
    return "ship";
  if (
    (order.status === "shipped" ||
      (order.status === "confirmed" && !orderHasStockItems(order))) &&
    ["admin", "finance"].includes(role)
  )
    return "invoice";
  return null;
}
