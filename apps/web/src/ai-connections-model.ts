import type { Role } from "./types";

export type AiScope = "finance" | "sales" | "inventory";
export type AiAccess = {
  user_id: string;
  enabled: boolean;
  scopes: AiScope[];
  allowed_scopes: AiScope[];
};
export type AiConnection = {
  id: string;
  name: string;
  scopes: AiScope[];
  created_at: string;
  expires_at: string;
  last_used_at: string | null;
  revoked_at: string | null;
};
export type AiUser = Omit<AiAccess, "user_id"> & {
  id: string;
  name: string;
  email: string;
  role: Role;
  active: boolean;
};
export type AiAccessResponse = {
  access: AiAccess;
  tokens: AiConnection[];
  scope_catalog: { id: AiScope; label: string }[];
  users?: AiUser[];
};

export function usableAiScopes(access: AiAccess): AiScope[] {
  return access.enabled
    ? [...new Set(access.scopes)].filter((scope) =>
        access.allowed_scopes.includes(scope),
      )
    : [];
}

export function aiConnectionStatus(
  connection: AiConnection,
  access: AiAccess,
  now = Date.now(),
): { label: string; usable: boolean } {
  if (connection.revoked_at) return { label: "Dicabut", usable: false };
  const expires = Date.parse(connection.expires_at);
  if (!Number.isFinite(expires) || expires <= now)
    return { label: "Kedaluwarsa", usable: false };
  const scopes = usableAiScopes(access);
  const usable = connection.scopes.filter((scope) => scopes.includes(scope));
  if (!usable.length) return { label: "Akses dinonaktifkan", usable: false };
  return {
    label:
      usable.length < connection.scopes.length ? "Akses terbatas" : "Aktif",
    usable: true,
  };
}
