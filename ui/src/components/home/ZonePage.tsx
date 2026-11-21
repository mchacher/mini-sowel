// mini-Sowel's own file: the page of one zone. Sowel's HomePage (451 lines) pulls about 35 modules,
// the 11-store WebSocket and a router; this one keeps its parts (the title, Sowel's
// ZoneHeroLead and ZoneAggregationPills, Sowel's Panel with the equipments grouped by family) and
// nothing else.
import {
  Box,
  CircleDot,
  Droplet,
  DoorClosed,
  Gauge,
  Lightbulb,
  Monitor,
  Thermometer,
  ToggleRight,
  Tv,
  WashingMachine,
  Zap,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { EquipmentWithDetails, ZoneAggregatedData, ZoneWithChildren } from "../../types";
import { SECTIONS, sectionOf, type SectionId } from "../../lib/equipment-family";
import { EquipmentRow } from "./EquipmentRow";
import { Panel, SliceHeader } from "./Panel";
import { ZoneAggregationPills } from "./ZoneAggregationPills";
import { ZoneHeroLead } from "./ZoneHeroLead";

// One icon for each section of Sowel's ZoneEquipmentsView (its openings icon is a shutter, which
// the tool has not).
const SECTION_ICONS: Record<SectionId, React.ReactNode> = {
  lighting: <Lightbulb size={16} strokeWidth={1.5} />,
  openings: <DoorClosed size={16} strokeWidth={1.5} />,
  climate: <Thermometer size={16} strokeWidth={1.5} />,
  water: <Droplet size={16} strokeWidth={1.5} />,
  energy: <Zap size={16} strokeWidth={1.5} />,
  sensors: <Gauge size={16} strokeWidth={1.5} />,
  media: <Tv size={16} strokeWidth={1.5} />,
  appliances: <WashingMachine size={16} strokeWidth={1.5} />,
  plugs: <ToggleRight size={16} strokeWidth={1.5} />,
  controls: <CircleDot size={16} strokeWidth={1.5} />,
  displays: <Monitor size={16} strokeWidth={1.5} />,
  other: <Box size={16} strokeWidth={1.5} />,
};

export function ZonePage({
  zone,
  aggregate,
  equipments,
  onExecuteOrder,
  empty,
}: {
  zone: ZoneWithChildren;
  aggregate: ZoneAggregatedData | undefined;
  /** The equipments of this zone, not of its sub-zones. */
  equipments: EquipmentWithDetails[];
  onExecuteOrder: (equipmentId: string, alias: string, value: unknown) => Promise<void>;
  /** The text shown when the tool has no room yet (only the root zone). */
  empty?: boolean;
}) {
  const { t } = useTranslation();
  const groups = SECTIONS.map((section) => ({
    ...section,
    equipments: equipments.filter((eq) => sectionOf(eq) === section.id),
  })).filter((g) => g.equipments.length > 0);

  return (
    <div className="space-y-4">
      <header>
        <h1 className="text-display font-semibold text-text">{zone.name}</h1>
        <ZoneHeroLead zone={zone} aggData={aggregate} />
      </header>
      {aggregate && <ZoneAggregationPills data={aggregate} zoneId={zone.id} />}
      {empty ? (
        <p className="text-body text-text-secondary">{t("app.noRoom")}</p>
      ) : groups.length === 0 ? (
        <p className="text-body text-text-secondary">{t("app.noEquipment")}</p>
      ) : (
        <Panel title={t("app.equipments")} count={equipments.length}>
          {groups.map((group) => (
            <div key={group.id}>
              <SliceHeader
                icon={SECTION_ICONS[group.id]}
                label={t(group.labelKey)}
                count={group.equipments.length}
                family={group.family}
              />
              <div className="slice-rows">
                {group.equipments.map((eq) => (
                  <EquipmentRow key={eq.id} equipment={eq} onExecuteOrder={onExecuteOrder} />
                ))}
              </div>
            </div>
          ))}
        </Panel>
      )}
    </div>
  );
}
