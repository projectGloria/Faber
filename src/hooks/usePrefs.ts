import { useCallback, useEffect, useState } from "react";
import { LazyStore } from "@tauri-apps/plugin-store";
import { DEFAULT_OPTIONS, type Options } from "../lib/api";

// Module-level singleton: the store is a native resource; one handle for the app's lifetime.
const store = new LazyStore("faber.json", { autoSave: 200, defaults: {} });
const MAX_RECENTS = 8;

/** Recent projects + per-project option overrides, persisted in the app data dir. */
export function usePrefs() {
  const [recents, setRecents] = useState<string[]>([]);
  const [overrides, setOverrides] = useState<Record<string, Options>>({});
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    let alive = true; // StrictMode runs this twice; don't let the discarded pass set state
    Promise.all([store.get<string[]>("recents"), store.get<Record<string, Options>>("overrides")]).then(([r, o]) => {
      if (!alive) return;
      setRecents(r ?? []);
      setOverrides(o ?? {});
      setLoaded(true);
    });
    return () => {
      alive = false;
    };
  }, []);

  // Persisting is syncing to an external system, which is exactly what effects are for. Keeping
  // store writes out of the setState updaters keeps those pure (StrictMode double-invokes them).
  // The `loaded` gate stops the initial empty state from clobbering the file before it's read.
  useEffect(() => {
    if (loaded) store.set("recents", recents);
  }, [loaded, recents]);
  useEffect(() => {
    if (loaded) store.set("overrides", overrides);
  }, [loaded, overrides]);

  // Functional updates read the latest list, so rapid picks can't lose entries to a stale closure.
  const pushRecent = useCallback((path: string) => {
    setRecents((prev) => [path, ...prev.filter((p) => p.toLowerCase() !== path.toLowerCase())].slice(0, MAX_RECENTS));
  }, []);

  const removeRecent = useCallback((path: string) => {
    setRecents((prev) => prev.filter((p) => p.toLowerCase() !== path.toLowerCase()));
  }, []);

  const setOptionsFor = useCallback((path: string, opts: Options) => {
    setOverrides((prev) => ({ ...prev, [path]: opts }));
  }, []);

  const optionsFor = useCallback((path: string): Options => ({ ...DEFAULT_OPTIONS, ...overrides[path] }), [overrides]);

  return { loaded, recents, pushRecent, removeRecent, optionsFor, setOptionsFor };
}
