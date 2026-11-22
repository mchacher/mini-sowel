// Solite's own file: the pairing window as the page sees it (spec 017, R5, R6; its screen is
// Discovery, spec 019). The host sends no event for the window: the page counts down from the
// `remaining` of each answer and reads the state again at 0, one second after a `POST` (to show a refusal), and on every reconnection.
import { create } from "zustand";
import { getPermitJoin, setPermitJoin } from "../api";
import { apiErrorText } from "../lib/api-error";
import type { PermitJoinState } from "../types";

/** Seconds a `Start` asks for: the default of `--pair`. */
export const PAIRING_SECONDS = 180;
const CHECK_AFTER_MS = 1000;

interface PairingState {
  open: boolean;
  remaining: number;
  /** The radio's refusal of the last opening, or the failure of the last request. */
  error: string | null;
  /** The devices discovered since the screen was opened, marked "New". At most 64 (the device limit). */
  newIds: string[];
  /** The Discovery screen is on screen: only then is a discovered device marked "New". */
  pageOpen: boolean;

  refresh: () => Promise<void>;
  start: () => Promise<void>;
  stop: () => Promise<void>;
  setPageOpen: (open: boolean) => void;
  markNew: (deviceId: string) => void;
  clearNew: () => void;
}

let ticker: ReturnType<typeof setInterval> | null = null;
let checker: ReturnType<typeof setTimeout> | null = null;

function stopTicker(): void {
  if (ticker) clearInterval(ticker);
  ticker = null;
}

/** Shows an answer and counts down from it. */
function show(answer: PermitJoinState): void {
  stopTicker();
  const open = answer.permitJoin && answer.remaining > 0;
  usePairing.setState({
    open,
    remaining: open ? answer.remaining : 0,
    error: answer.error ?? null,
  });
  if (!open) return;
  ticker = setInterval(() => {
    const left = usePairing.getState().remaining - 1;
    if (left > 0) {
      usePairing.setState({ remaining: left });
      return;
    }
    stopTicker();
    usePairing.setState({ open: false, remaining: 0 });
    void usePairing.getState().refresh();
  }, 1000);
}

function checkSoon(): void {
  if (checker) clearTimeout(checker);
  checker = setTimeout(() => {
    checker = null;
    void usePairing.getState().refresh();
  }, CHECK_AFTER_MS);
}

export const usePairing = create<PairingState>((set) => ({
  open: false,
  remaining: 0,
  error: null,
  newIds: [],
  pageOpen: false,

  refresh: async () => {
    try {
      show(await getPermitJoin());
    } catch (err) {
      set({ error: apiErrorText(err) });
    }
  },

  start: async () => {
    try {
      show(await setPermitJoin(PAIRING_SECONDS));
      checkSoon();
    } catch (err) {
      set({ error: apiErrorText(err) });
    }
  },

  stop: async () => {
    try {
      show(await setPermitJoin(0));
    } catch (err) {
      set({ error: apiErrorText(err) });
    }
  },

  // Opening or leaving the page starts a new "since the page opened".
  setPageOpen: (pageOpen) => set({ pageOpen, newIds: [] }),

  markNew: (deviceId) => {
    set((s) =>
      !s.pageOpen || s.newIds.includes(deviceId)
        ? s
        : { newIds: [...s.newIds, deviceId].slice(-64) },
    );
  },

  clearNew: () => set({ newIds: [] }),
}));

/** Stops the timers (tests). */
export function resetPairingTimers(): void {
  stopTicker();
  if (checker) clearTimeout(checker);
  checker = null;
}
