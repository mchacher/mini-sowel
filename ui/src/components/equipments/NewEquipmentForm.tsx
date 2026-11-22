// Solite's own file: Sowel's EquipmentForm in create mode, wired to the stores. The Equipments
// page, a room's page and a device's "Create equipment" open it (spec 018, R3, R5; spec 019, R4).
// On submit: the equipment, then one request per binding of the plan computed from the devices
// ticked, the equipment deleted again when one is refused (decision D30).
import { useZones } from "../../store/useZones";
import { useEquipments } from "../../store/useEquipments";
import { useTranslation } from "react-i18next";
import type { EquipmentType } from "../../types";
import { EquipmentForm } from "./EquipmentForm";
import { computeBindingPlan } from "./bindingUtils";

export function NewEquipmentForm({
  defaultZoneId,
  initialType,
  typeIsGuess,
  initialName,
  initialDeviceIds,
  onClose,
}: {
  defaultZoneId?: string;
  initialType?: EquipmentType;
  typeIsGuess?: boolean;
  initialName?: string;
  initialDeviceIds?: string[];
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const tree = useZones((s) => s.tree);
  return (
    <EquipmentForm
      title={t("equipments.createEquipment")}
      zones={tree}
      defaultZoneId={defaultZoneId}
      initialType={initialType}
      typeIsGuess={typeIsGuess}
      initialName={initialName}
      initialDeviceIds={initialDeviceIds}
      onSubmit={async ({ name, type, zoneId, devices }) => {
        await useEquipments
          .getState()
          .createWithBindings({ name, type, zoneId }, computeBindingPlan(devices, type));
      }}
      onClose={onClose}
    />
  );
}
