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
  product_id: string;
  product_name?: string;
  sku?: string;
  quantity: string;
  unit_price: string;
  total?: string;
  product_kind?: "stock" | "service";
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
}
export interface Movement {
  id: string;
  product_name: string;
  sku?: string;
  kind: string;
  quantity: string;
  unit_cost: string;
  reference?: string;
  created_at: string;
}
export interface Account {
  id?: string;
  code: string;
  name: string;
  kind: string;
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
