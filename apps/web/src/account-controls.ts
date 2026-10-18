import type { Account } from "./types";

export function isControlAccount(account: Account): boolean {
  return (
    account.is_control === true ||
    !!account.system_role ||
    ["1200", "1300"].includes(account.code)
  );
}
