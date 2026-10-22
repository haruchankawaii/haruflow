import { describe, expect, test } from "bun:test";
import {
  aiConnectionStatus,
  usableAiScopes,
  type AiAccess,
  type AiConnection,
} from "./ai-connections-model";

const access: AiAccess = {
  user_id: "owner",
  enabled: true,
  scopes: ["finance", "inventory"],
  allowed_scopes: ["finance", "inventory"],
};
const token: AiConnection = {
  id: "connection",
  name: "Codex PC saya",
  scopes: ["finance", "inventory"],
  created_at: "2026-10-21T00:00:00Z",
  expires_at: "2026-11-21T00:00:00Z",
  last_used_at: null,
  revoked_at: null,
};
const now = Date.parse("2026-10-22T00:00:00Z");

describe("AI connection access display", () => {
  test("only offers grants still allowed by the user's role", () => {
    expect(
      usableAiScopes({ ...access, allowed_scopes: ["inventory"] }),
    ).toEqual(["inventory"]);
    expect(usableAiScopes({ ...access, enabled: false })).toEqual([]);
    expect(usableAiScopes({ ...access, allowed_scopes: [] })).toEqual([]);
  });

  test("distinguishes revoked, expired, blocked, and partially usable tokens", () => {
    expect(aiConnectionStatus(token, access, now)).toEqual({
      label: "Aktif",
      usable: true,
    });
    expect(
      aiConnectionStatus(
        token,
        { ...access, allowed_scopes: ["inventory"] },
        now,
      ),
    ).toEqual({ label: "Akses terbatas", usable: true });
    expect(
      aiConnectionStatus(token, { ...access, enabled: false }, now).usable,
    ).toBe(false);
    expect(
      aiConnectionStatus({ ...token, revoked_at: "2026-10-22" }, access, now),
    ).toEqual({ label: "Dicabut", usable: false });
    expect(
      aiConnectionStatus(token, access, Date.parse(token.expires_at)),
    ).toEqual({ label: "Kedaluwarsa", usable: false });
    expect(
      aiConnectionStatus({ ...token, expires_at: "invalid" }, access, now),
    ).toEqual({ label: "Kedaluwarsa", usable: false });
  });
});
