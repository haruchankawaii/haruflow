export type Role = "admin" | "sales" | "warehouse" | "finance" | "viewer";
export interface User {
  id: string;
  name: string;
  email: string;
  role: Role;
  active?: boolean;
}
export interface Session {
  user: User;
  company: Company;
}
export interface Company {
  id: string;
  name: string;
  currency: string;
  timezone: string;
  address?: string | null;
  email?: string | null;
  phone?: string | null;
  tax_id?: string | null;
  order_prefix?: string;
  invoice_prefix?: string;
}
export interface Customer {
  id: string;
  name: string;
  email?: string;
  phone?: string;
  address?: string;
  created_at?: string;
}
export interface Product {
  id: string;
  sku: string;
  name: string;
  unit: string;
  price: string;
  quantity: string;
  average_cost: string;
  inventory_value?: string;
  kind?: "stock" | "service";
}
export interface OrderLine {
  id?: string;
  line_id?: string;
  product_id: string;
  product_name?: string;
  sku?: string;
  quantity: string;
  unit_price: string;
  total?: string;
  product_kind?: "stock" | "service";
  shipped_quantity?: string;
  unshipped_quantity?: string;
  invoiced_quantity?: string;
  invoiced_total?: string;
  invoiceable_quantity?: string;
  uninvoiced_quantity?: string;
}
export interface SalesShipment {
  id: string;
  created_at: string;
  reversed: boolean;
  reversal_reason?: string;
  can_reverse: boolean;
  items: {
    line_id: string;
    sku: string;
    name: string;
    quantity: string;
    value: string;
  }[];
}
export interface Order {
  id: string;
  number: string;
  customer_id?: string;
  customer_name: string;
  status: string;
  total: string;
  paid?: string;
  created_at: string;
  items?: OrderLine[];
  has_stock_items?: boolean;
  version?: number;
  can_ship?: boolean;
  can_invoice?: boolean;
  shipments?: SalesShipment[];
  invoices?: Invoice[];
}
export interface Invoice {
  id: string;
  number: string;
  order_id?: string;
  customer_id?: string;
  customer_name: string;
  status: string;
  total: string;
  paid: string;
  created_at: string;
  outstanding?: string;
  version?: number;
  credit_total?: string;
  net_total?: string;
  items?: OrderLine[];
}
export interface Movement {
  id: string;
  order_id?: string | null;
  product_name: string;
  sku?: string;
  kind: string;
  quantity: string;
  unit_cost: string;
  value: string;
  reference?: string;
  created_at: string;
  can_reverse?: boolean;
  reversal?: DocumentReversalInfo | null;
  reversal_blocked_reason?: string | null;
  reverses_movement_id?: string | null;
  purchase_receipt_id?: string | null;
  purchase_order_id?: string | null;
  purchase_order_number?: string | null;
  shipment_id?: string | null;
  sales_return_id?: string | null;
  sales_return_number?: string | null;
  sales_return_invoice_id?: string | null;
  supplier_return_id?: string | null;
  supplier_return_number?: string | null;
  supplier_return_bill_id?: string | null;
}
export interface DocumentReversalInfo {
  id: string;
  reason: string;
  created_at: string;
  journal_id: string;
}
export interface Account {
  id?: string;
  code: string;
  name: string;
  kind: string;
  is_control?: boolean;
  system_role?: string | null;
}
export interface JournalLine {
  account_code: string;
  account_name: string;
  debit: string;
  credit: string;
}
export interface Journal {
  id: string;
  number?: string;
  description: string;
  reference?: string;
  source_type?: string;
  source_id?: string;
  created_at: string;
  lines: JournalLine[];
  can_reverse?: boolean;
  reversal?: DocumentReversalInfo | null;
  reversed_source?: { source_type: string; source_id: string } | null;
  reversal_blocked_reason?: string | null;
}
export interface TrialBalance {
  code: string;
  name: string;
  kind?: string;
  debit: string;
  credit: string;
  balance?: string;
}
export interface Dashboard {
  revenue: string;
  outstanding: string;
  inventory_value: string;
  cash: string;
  orders_count: number;
  products_count: number;
  customers_count: number;
  recent_orders: Order[];
  monthly_sales: { month: string; total: string }[];
}
