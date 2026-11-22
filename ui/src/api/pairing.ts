// Solite's own file: the pairing window of the host (spec 017, decision D28). Sowel has no
// pairing route; `time` is Zigbee2MQTT's `permit_join` field (seconds, 0 closes).
import type { PermitJoinState } from "../types";
import { fetchJSON, API_BASE } from "./client";

const ROUTE = `${API_BASE}/integrations/solite/permit-join`;

export async function getPermitJoin(): Promise<PermitJoinState> {
  return fetchJSON<PermitJoinState>(ROUTE);
}

export async function setPermitJoin(time: number): Promise<PermitJoinState> {
  return fetchJSON<PermitJoinState>(ROUTE, { method: "POST", body: JSON.stringify({ time }) });
}
