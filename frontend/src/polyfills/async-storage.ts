type StorageValue = string;

const store = new Map<string, StorageValue>();

const AsyncStorage = {
  async getItem(key: string): Promise<string | null> {
    return store.has(key) ? store.get(key)! : null;
  },

  async setItem(key: string, value: string): Promise<void> {
    store.set(key, value);
  },

  async removeItem(key: string): Promise<void> {
    store.delete(key);
  },

  async clear(): Promise<void> {
    store.clear();
  },

  async getAllKeys(): Promise<string[]> {
    return Array.from(store.keys());
  },

  async multiGet(keys: string[]): Promise<[string, string | null][]> {
    return keys.map((key) => [key, store.get(key) ?? null]);
  },

  async multiSet(entries: [string, string][]): Promise<void> {
    entries.forEach(([key, value]) => {
      store.set(key, value);
    });
  },
};

export default AsyncStorage;

export const { getItem, setItem, removeItem, clear, getAllKeys, multiGet, multiSet } = AsyncStorage;
