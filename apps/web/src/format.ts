const moneyFormatter = new Intl.NumberFormat("id-ID", {
  style: "currency",
  currency: "IDR",
  minimumFractionDigits: 0,
  maximumFractionDigits: 2,
});
const costFormatter = new Intl.NumberFormat("id-ID", {
  style: "currency",
  currency: "IDR",
  minimumFractionDigits: 0,
  maximumFractionDigits: 4,
});
const numberFormatter = new Intl.NumberFormat("id-ID", {
  maximumFractionDigits: 3,
});
export const money = (value: string | number | undefined) =>
  moneyFormatter.format(Number(value ?? 0));
export const costMoney = (value: string | number | undefined) =>
  costFormatter.format(Number(value ?? 0));
export const number = (value: string | number | undefined) =>
  numberFormatter.format(Number(value ?? 0));
export const date = (value: string) =>
  value
    ? new Intl.DateTimeFormat("id-ID", {
        day: "numeric",
        month: "short",
        year: "numeric",
        timeZone: "Asia/Jakarta",
      }).format(new Date(value))
    : "—";
export const roleNames = {
  admin: "Administrator",
  sales: "Penjualan",
  warehouse: "Gudang",
  finance: "Keuangan",
  viewer: "Pengamat",
};
export const statusNames: Record<string, string> = {
  draft: "Draf",
  confirmed: "Dikonfirmasi",
  shipped: "Dikirim",
  invoiced: "Ditagihkan",
  unpaid: "Belum dibayar",
  partial: "Dibayar sebagian",
  partially_paid: "Dibayar sebagian",
  paid: "Lunas",
  open: "Belum dibayar",
  cancelled: "Dibatalkan",
  receipt: "Stok masuk",
  shipment: "Stok keluar",
  adjustment_in: "Penyesuaian masuk",
  adjustment_out: "Penyesuaian keluar",
  shipment_reversal: "Pengembalian pengiriman",
  void: "Dibatalkan",
};

// Decimal arithmetic stays in integer minor units for the preview. The server
// remains authoritative for amounts, stock valuation, and posting journals.
export function toMinorUnits(value: string): bigint {
  if (!/^\d+(\.\d{1,4})?$/.test(value)) return 0n;
  const [whole, fraction = ""] = value.split(".");
  return BigInt(whole) * 10000n + BigInt(fraction.padEnd(4, "0"));
}
export function lineTotal(quantity: string, price: string): string {
  // Match the server: round each positive line to two currency decimals,
  // with midpoint values rounded away from zero, before summing the order.
  const cents =
    (toMinorUnits(quantity) * toMinorUnits(price) + 500000n) / 1000000n;
  return `${cents / 100n}.${(cents % 100n).toString().padStart(2, "0")}`;
}
export function orderTotal(
  lines: { quantity: string; unit_price: string }[],
): string {
  const scaled = lines.reduce(
    (sum, line) =>
      sum + toMinorUnits(lineTotal(line.quantity, line.unit_price)),
    0n,
  );
  return `${scaled / 10000n}.${((scaled % 10000n) / 100n).toString().padStart(2, "0")}`;
}
