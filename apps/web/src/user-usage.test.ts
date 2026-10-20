import { describe, expect, test } from "bun:test";
import {
  userActivationAllowed,
  workspaceUserUsage,
  type UsersResponse,
} from "./user-usage";

describe("Community active user capacity", () => {
  test("allows a new activation at nine and blocks at ten or above", () => {
    for (const active of [9, 10, 12]) {
      const usage = { active, limit: 10, available: Math.max(0, 10 - active) };
      expect(userActivationAllowed(usage, false, true)).toBe(active < 10);
      expect(userActivationAllowed(usage, true, true)).toBe(true);
      expect(userActivationAllowed(usage, true, false)).toBe(true);
      expect(userActivationAllowed(usage, false, false)).toBe(true);
    }
  });

  test("workspace usage comes from the full server snapshot rather than filtered rows", () => {
    const response: UsersResponse = {
      items: [
        {
          id: "a",
          name: "Admin",
          email: "a@example.test",
          role: "admin",
          active: true,
        },
        {
          id: "b",
          name: "Pengamat",
          email: "b@example.test",
          role: "viewer",
          active: true,
        },
      ],
      usage: { active: 10, limit: 10, available: 0 },
    };
    const searched = {
      ...response,
      items: response.items.filter((user) => user.name === "Admin"),
    };
    expect(workspaceUserUsage(searched)).toEqual({
      active: 10,
      limit: 10,
      available: 0,
    });
    expect(
      userActivationAllowed(workspaceUserUsage(searched), false, true),
    ).toBe(false);
    expect(workspaceUserUsage(null).available).toBe(0);
  });
});
