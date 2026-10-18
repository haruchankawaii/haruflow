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
let companyTimezone = "Asia/Jakarta";
export function setCompanyTimezone(timezone: string) {
  companyTimezone = timezone;
}
export const getCompanyTimezone = () => companyTimezone;
export const date = (value: string) =>
  value
    ? new Intl.DateTimeFormat("id-ID", {
        day: "numeric",
        month: "short",
        year: "numeric",
        timeZone: companyTimezone,
      }).format(new Date(value))
    : "—";
export function dateTime(value: string) {
  const time = new Date(value);
  return Number.isNaN(time.getTime())
    ? "—"
    : new Intl.DateTimeFormat("id-ID", {
        day: "numeric",
        month: "short",
        year: "numeric",
        hour: "2-digit",
        minute: "2-digit",
        timeZone: companyTimezone,
        timeZoneName: "short",
      }).format(time);
}
export function companyToday(now = new Date()) {
  return new Intl.DateTimeFormat("en-CA", {
    timeZone: companyTimezone,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  }).format(now);
}
export const roleNames = {
  admin: "Administrator",
  sales: "Penjualan",
  warehouse: "Gudang",
  finance: "Keuangan",
  viewer: "Pengamat",
};
export const statusNames: Record<string, string> = {
  draft: "Draf",
  posted: "Diposting",
  confirmed: "Dikonfirmasi",
  received: "Diterima",
  shipped: "Dikirim",
  invoiced: "Ditagihkan",
  unpaid: "Belum dibayar",
  partial: "Dibayar sebagian",
  partially_paid: "Dibayar sebagian",
  paid: "Lunas",
  open: "Belum dibayar",
  cancelled: "Dibatalkan",
  receipt: "Stok masuk",
  receipt_reversal: "Pembatalan penerimaan",
  purchase_receipt: "Penerimaan PO",
  purchase_receipt_reversal: "Pembalikan penerimaan PO",
  shipment: "Stok keluar",
  adjustment_in: "Penyesuaian masuk",
  adjustment_out: "Penyesuaian keluar",
  shipment_reversal: "Pengembalian pengiriman",
  opening: "Stok awal",
  opening_reversal: "Pembalikan stok awal",
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
