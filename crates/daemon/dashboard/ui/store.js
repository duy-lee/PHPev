// A tiny shared state for parts of the page that are not parent and child
// (the toolbar and the page below it).
import { useState, useEffect } from '/vendor/htm-preact-standalone.mjs';

export function createStore(initial) {
  let value = initial;
  const listeners = new Set();
  return {
    get: () => value,
    set(patch) {
      value = { ...value, ...patch };
      listeners.forEach((f) => f(value));
    },
    use() {
      const [state, setState] = useState(value);
      useEffect(() => {
        listeners.add(setState);
        setState(value);
        return () => listeners.delete(setState);
      }, []);
      return state;
    },
  };
}
