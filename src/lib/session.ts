import { useEffect, useState } from 'react';

import type { Conversation } from './contracts';

type Snapshot = { list: Conversation[]; currentId: string | null };
type Listener = () => void;

let snap: Snapshot = { list: [], currentId: null };
const listeners = new Set<Listener>();

export function publishSession(list: Conversation[], currentId: string | null): void {
  snap = { list, currentId };
  listeners.forEach((l) => l());
}

export function requestOpen(id: string): void {
  snap = { ...snap, currentId: id };
  listeners.forEach((l) => l());
}

export function useSession(): Snapshot {
  const [s, setS] = useState(snap);
  useEffect(() => {
    const l = (): void => setS(snap);
    listeners.add(l);
    return () => {
      listeners.delete(l);
    };
  }, []);
  return s;
}
