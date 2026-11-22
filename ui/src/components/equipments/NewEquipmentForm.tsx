// Solite's own file: Sowel's EquipmentForm in create mode, wired to the stores. The Equipments
// page and a room's page open it (spec 018, R3, R5).
import { useZones } from "../../store/useZones";
import { useEquipments } from "../../store/useEquipments";
import { useTranslation } from "react-i18next";
import { EquipmentForm } from "./EquipmentForm";

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
      onSubmit={async (data) => {
        await useEquipments.getState().createEquipment(data);
      }}
      onClose={onClose}
    />
  );
}
