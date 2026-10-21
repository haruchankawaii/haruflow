import { useResource } from "./hooks";
import { exactMoney } from "./dashboard-charts";
import type { TreasuryAccount } from "./treasury-model";

export function CashAccountSelect({ value = "1100", onChange, disabled = false, label = "Akun kas / bank" }: {
  value?: string; onChange: (value: string) => void; disabled?: boolean; label?: string;
}) {
  const accounts = useResource<{ items: TreasuryAccount[] }>("/treasury/accounts");
  return <label className="form-field"><span>{label}</span><select aria-label={label} value={value} onChange={e => onChange(e.target.value)} disabled={disabled || accounts.loading || !!accounts.error}>
    {(accounts.data?.items ?? [{ account_code: "1100", name: "Kas", balance: "0" }]).map(account => <option key={account.account_code} value={account.account_code}>{account.account_code} · {account.name}{accounts.data ? ` · ${exactMoney(account.balance)}` : ""}</option>)}
  </select>{accounts.error && <small role="alert">Akun gagal dimuat: {accounts.error}</small>}</label>;
}
