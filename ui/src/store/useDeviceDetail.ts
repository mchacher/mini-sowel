// Solite's own file (spec 030, R7): the data and the orders of the one device whose page is open.
// Sowel's `DeviceDetailPage` keeps them in component state and takes the live values from
// `useDevices`; Solite's `useDevices` holds no data (spec 017), so the page's own store follows the
// `device.data.updated` events. The name, status and last seen stay in `useDevices` (a rename and
// `device.status_changed` update them there). One device at a time: the store does not grow.
import { create } from "zustand";
import type { DeviceData, DeviceOrder } from "../types";
import { getDevice } from "../api";
import { apiErrorStatus, apiErrorText } from "../lib/api-error";

interface DeviceDetailState {
  /** The device the page shows; `null` when no page is open. */
  deviceId: string | null;
  data: DeviceData[];
  orders: DeviceOrder[];
  loading: boolean;
  /** The device does not exist (404) or was removed. */
  notFound: boolean;
  /** The text of a failed read other than a 404. */
  error: string | null;

  load: (id: string) => Promise<void>;
  /** Reads the open device again without the spinner (after a reconnection); no-op with no page open. */
  reload: () => Promise<void>;
  clear: () => void;
  handleDataUpdated: (event: DataUpdatedEvent) => void;
  handleRemoved: (deviceId: string) => void;
}

/** The fields of Sowel's `device.data.updated` event that the page reads. */
export interface DataUpdatedEvent {
  deviceId: string;
  dataId: string;
  key: string;
  value: unknown;
  timestamp: string;
}

const EMPTY = {
  deviceId: null,
  data: [] as DeviceData[],
  orders: [] as DeviceOrder[],
  loading: false,
  notFound: false,
  error: null,
};

export const useDeviceDetail = create<DeviceDetailState>((set, get) => {
  let silentBusy = false;
  let rereadWanted = false;
  /** Reads the device; a page opened on another device meanwhile ignores the answer. */
  const read = async (id: string, showLoading: boolean): Promise<void> => {
    if (showLoading) {
      set({ ...EMPTY, deviceId: id, loading: true });
      silentBusy = false;
      rereadWanted = false;
    } else {
      // A burst of unknown data ids: one silent read in flight, and one more after it if asked.
      if (silentBusy) {
        rereadWanted = true;
        return;
      }
      silentBusy = true;
    }
    try {
      const device = await getDevice(id);
      if (get().deviceId !== id) return;
      set({
        data: device.data ?? [],
        orders: device.orders ?? [],
        loading: false,
        notFound: false,
        error: null,
      });
    } catch (err) {
      if (get().deviceId !== id) return;
      if (apiErrorStatus(err) === 404) set({ loading: false, notFound: true, error: null });
      else set({ loading: false, error: apiErrorText(err) });
    } finally {
      if (!showLoading) {
        silentBusy = false;
        if (rereadWanted) {
          rereadWanted = false;
          if (get().deviceId === id) void read(id, false);
        }
      }
    }
  };

  return {
    ...EMPTY,

    load: (id) => read(id, true),

    reload: async () => {
      const id = get().deviceId;
      if (id) await read(id, false);
    },

    clear: () => set({ ...EMPTY }),

    handleDataUpdated: (event) => {
      const state = get();
      if (state.deviceId !== event.deviceId || state.loading || state.notFound) return;
      if (!state.data.some((d) => d.id === event.dataId)) {
        // A data the page does not hold (a device set up after the read): read the device again.
        void read(event.deviceId, false);
        return;
      }
      set({
        data: state.data.map((d) =>
          d.id === event.dataId ? { ...d, value: event.value, lastUpdated: event.timestamp } : d,
        ),
      });
    },

    handleRemoved: (deviceId) => {
      if (get().deviceId === deviceId) set({ notFound: true, loading: false, error: null });
    },
  };
});
