// Solite's own file (decision D50), built from Sowel's `ConfigRow` and `Switch`: the Configuration
// card's "Enabled" row. Sowel's interface has no control for an equipment's `enabled`, its API
// takes the field. Opened, the row holds a switch; switching sends the field and waits for the
// answer (the switch shows the old state meanwhile); a refusal shows the host's text in the row
// and the switch stays where it was.
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Power } from "lucide-react";
import { useEquipments } from "../../store/useEquipments";
import { apiErrorText } from "../../lib/api-error";
import { formErrorText } from "../../lib/form-error";
import { Switch } from "../ui/Switch";
import { ConfigRow } from "./ConfigRow";

export function EnabledRow({ equipmentId, enabled }: { equipmentId: string; enabled: boolean }) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const change = async (next: boolean) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await useEquipments.getState().updateEquipment(equipmentId, { enabled: next });
    } catch (err) {
      // A 404 carries the host's own words (the equipment was deleted meanwhile).
      setError(formErrorText(err, t) || apiErrorText(err, t));
    } finally {
      setBusy(false);
    }
  };

  const label = enabled ? t("equipments.enabled") : t("common.disabled");
  return (
    <ConfigRow icon={Power} title={t("equipments.enabledRow")} summary={label}>
      <div className="flex items-center gap-3">
        <Switch
          checked={enabled}
          disabled={busy}
          label={t("equipments.enabledRow")}
          onChange={(next) => void change(next)}
        />
      </div>
      {error && (
        <p role="alert" className="text-caption text-error mt-2">
          {error}
        </p>
      )}
    </ConfigRow>
  );
}
