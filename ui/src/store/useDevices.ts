// Ported from Sowel ui/src/store/useDevices.ts (Sowel commit 5da84dc1, UI 1.87.0). Cuts: the device
// data and heartbeat (the devices page shows none, spec 017) and `updateDeviceName`'s siblings.
// Added: `deleteDevice`. A device has no room of its own: nothing here writes `zoneId` (spec 019).
import { create } from "zustand";
import type { Device, DeviceStatus } from "../types";
import { getDevices, updateDevice, deleteDevice as apiDeleteDevice } from "../api";
import { apiErrorText } from "../lib/api-error";

interface DevicesState {
  /** All devices indexed by id, in pairing order (the API's order, then the discoveries). */
  devices: Record<string, Device>;
  loading: boolean;
  error: string | null;

  fetchDevices: () => Promise<void>;
  addDevice: (device: Device) => void;
  removeDevice: (deviceId: string) => void;
  updateDeviceStatus: (deviceId: string, status: DeviceStatus) => void;
  updateDeviceName: (deviceId: string, name: string) => Promise<void>;
  deleteDevice: (deviceId: string) => Promise<void>;
}

export const useDevices = create<DevicesState>((set, get) => ({
  devices: {},
  loading: false,
  error: null,

  fetchDevices: async () => {
    set({ loading: true, error: null });
    try {
      const result = await getDevices();
      const devices: Record<string, Device> = {};
      for (const d of result) {
        // eslint-disable-next-line @typescript-eslint/no-unused-vars
        const { data: _data, ...device } = d;
        devices[device.id] = device;
      }
      set({ devices, loading: false });
    } catch (err) {
      set({ loading: false, error: apiErrorText(err) });
    }
  },

  addDevice: (device) => {
    set((state) => ({ devices: { ...state.devices, [device.id]: device } }));
  },

  removeDevice: (deviceId) => {
    set((state) => {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const { [deviceId]: _removed, ...devices } = state.devices;
      return { devices };
    });
  },

  updateDeviceStatus: (deviceId, status) => {
    set((state) => {
      const device = state.devices[deviceId];
      if (!device) return state;
      const now = new Date().toISOString();
      return {
        devices: {
          ...state.devices,
          [deviceId]: { ...device, status, lastSeen: now, updatedAt: now },
        },
      };
    });
  },

  updateDeviceName: async (deviceId, name) => {
    await change(set, get, deviceId, { name });
  },

  deleteDevice: async (deviceId) => {
    await apiDeleteDevice(deviceId);
    get().removeDevice(deviceId);
  },
}));

/** Optimistic update; on failure the old device is back and the caller gets the original error (#1124). */
async function change(
  set: (partial: Partial<DevicesState> | ((s: DevicesState) => Partial<DevicesState>)) => void,
  get: () => DevicesState,
  deviceId: string,
  patch: { name: string },
): Promise<void> {
  const device = get().devices[deviceId];
  if (!device) return;
  set((s) => ({ devices: { ...s.devices, [deviceId]: { ...device, ...patch } } }));
  try {
    await updateDevice(deviceId, patch);
  } catch (err) {
    set((s) => ({ devices: { ...s.devices, [deviceId]: device } }));
    throw err;
  }
}
