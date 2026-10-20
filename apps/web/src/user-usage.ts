import type { User } from "./types";

export interface UserUsage {
  active: number;
  limit: number;
  available: number;
}

export interface UsersResponse {
  items: User[];
  usage: UserUsage;
}

export function workspaceUserUsage(response: UsersResponse | null): UserUsage {
  return response?.usage ?? { active: 0, limit: 10, available: 0 };
}

export function userActivationAllowed(
  usage: UserUsage,
  currentlyActive: boolean,
  requestedActive: boolean,
) {
  return !requestedActive || currentlyActive || usage.available > 0;
}
