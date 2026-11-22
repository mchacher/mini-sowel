// Solite's own file: the integrations Discovery has a screen for (spec 019, R2). Sowel asks its API
// (`GET /api/v1/integrations`); Solite lists them here, one entry each: the id (the devices'
// `integrationId` and the API's `/integrations/:id`), the label's key and the screen. A second
// integration is one more entry.
import type { ComponentType } from "react";
import { SOLITE_INTEGRATION_ID, ZigbeeDiscovery } from "./ZigbeeDiscovery";

export interface Integration {
  id: string;
  labelKey: string;
  Screen: ComponentType;
}

export const INTEGRATIONS: Integration[] = [
  { id: SOLITE_INTEGRATION_ID, labelKey: "discovery.integration.solite", Screen: ZigbeeDiscovery },
];

const PREFIX = "#/discovery/";

/** The integration a hash asks for; the first one for `#/discovery` or an id nobody knows. */
export function integrationFromHash(hash: string): Integration {
  const id = hash.startsWith(PREFIX) ? decodeURIComponent(hash.slice(PREFIX.length)) : "";
  return INTEGRATIONS.find((i) => i.id === id) ?? INTEGRATIONS[0];
}
