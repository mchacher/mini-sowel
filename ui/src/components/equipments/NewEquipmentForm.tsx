// Solite's own file: Sowel's EquipmentForm in create mode, wired to the stores. The Equipments
// page and a room's page open it (spec 018, R3, R5).
// On submit: the equipment, then one request per binding of the plan computed from the devices
// ticked, the equipment deleted again when one is refused (decision D30). With no device ticked
// the equipment is made alone and its page opens (spec 030, R8).
import { useZones } from "../../store/useZones";
import { useEquipments } from "../../store/useEquipments";
import { useTranslation } from "react-i18next";
import { EquipmentForm } from "./EquipmentForm";
import { hashForDetail } from "../layout/admin-nav-items";
import { computeBindingPlan } from "./bindingUtils";

export function NewEquipmentForm({
  defaultZoneId,
  onClose,
}: {
  defaultZoneId?: string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const tree = useZones((s) => s.tree);
  return (
    <EquipmentForm
      title={t("equipments.createEquipment")}
      zones={tree}
      defaultZoneId={defaultZoneId}
      onSubmit={async ({ name, type, zoneId, deviceIds, devices }) => {
        const created = await useEquipments
          .getState()
          .createWithBindings({ name, type, zoneId }, computeBindingPlan(devices, type));
        // Nothing to bind: its page opens, where the devices are added.
        if (deviceIds.length === 0) window.location.hash = hashForDetail("equipments", created.id);
      }}
      onClose={onClose}
    />
  );
}
