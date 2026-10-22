import { beforeEach, describe, expect, mock, test } from "bun:test";

/**
 * Session persistence is what keeps a page reload from silently signing the
 * user out, so exercise it directly. Each test re-imports the module so its
 * in-memory token state starts clean.
 */

interface TestStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

class MemoryStorage implements TestStorage {
  readonly data = new Map<string, string>();
  getItem(k: string) {
    return this.data.get(k) ?? null;
  }
  setItem(k: string, v: string) {
    this.data.set(k, v);
  }
  removeItem(k: string) {
    this.data.delete(k);
  }
  clear() {
    this.data.clear();
  }
}

/** localStorage throws outright in some privacy modes. */
class ThrowingStorage implements TestStorage {
  getItem(): string | null {
    throw new Error("access denied");
  }
  setItem(): void {
    throw new Error("access denied");
  }
  removeItem(): void {
    throw new Error("access denied");
  }
}

const KEY = "stacks.refresh_token";

let storage_: TestStorage;

/** The module reads `window.localStorage`; there is no DOM under bun:test. */
function installStorage(storage: TestStorage) {
  storage_ = storage;
  (globalThis as unknown as { window: { localStorage: TestStorage } }).window =
    {
      localStorage: storage,
    };
}

function installFetch(impl: () => Promise<unknown>) {
  (globalThis as unknown as { fetch: unknown }).fetch = mock(impl);
}

async function freshAuth() {
  // Bust the module cache so module-level token state is reset per test.
  return await import(`./auth.ts?t=${Math.random()}`);
}

describe("token persistence", () => {
  beforeEach(() => {
    installStorage(new MemoryStorage());
  });

  test("setTokens persists only the refresh token", async () => {
    const auth = await freshAuth();
    auth.setTokens("access-1", "refresh-1");

    expect(auth.getAccessToken()).toBe("access-1");
    expect(auth.isAuthenticated()).toBe(true);
    // The access token must not be written to storage.
    const stored = storage_ as MemoryStorage;
    expect(stored.getItem(KEY)).toBe("refresh-1");
    expect(JSON.stringify([...stored.data])).not.toContain("access-1");
  });

  test("clearTokens removes the persisted refresh token", async () => {
    const auth = await freshAuth();
    auth.setTokens("access-1", "refresh-1");
    auth.clearTokens();

    expect(auth.getAccessToken()).toBeNull();
    expect(auth.isAuthenticated()).toBe(false);
    expect(storage_.getItem(KEY)).toBeNull();
  });

  test("restoreSession is a no-op with nothing stored", async () => {
    const auth = await freshAuth();
    expect(await auth.restoreSession()).toBe(false);
    expect(auth.isAuthenticated()).toBe(false);
  });

  test("restoreSession exchanges a stored refresh token for a session", async () => {
    storage_.setItem(KEY, "stored-refresh");
    installFetch(async () => ({
      ok: true,
      status: 200,
      json: async () => ({
        access_token: "new-access",
        refresh_token: "rotated-refresh",
      }),
    }));

    const auth = await freshAuth();
    expect(await auth.restoreSession()).toBe(true);
    expect(auth.getAccessToken()).toBe("new-access");
    // Refresh tokens rotate, so the stored value must be replaced.
    expect(storage_.getItem(KEY)).toBe("rotated-refresh");
  });

  test("restoreSession clears a refresh token the server rejects", async () => {
    storage_.setItem(KEY, "expired");
    installFetch(async () => ({ ok: false, status: 401 }));

    const auth = await freshAuth();
    expect(await auth.restoreSession()).toBe(false);
    expect(auth.isAuthenticated()).toBe(false);
    // A token that will never work again must not be retried on every load.
    expect(storage_.getItem(KEY)).toBeNull();
  });

  test("unavailable storage degrades to an in-memory session", async () => {
    installStorage(new ThrowingStorage());
    const auth = await freshAuth();

    // Must not throw.
    expect(() => auth.setTokens("access-1", "refresh-1")).not.toThrow();
    expect(auth.isAuthenticated()).toBe(true);
    expect(await auth.restoreSession()).toBe(false);
  });
});
