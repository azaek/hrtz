import { Button } from "@fluentui/react-components";
import { PersonFeedback20Regular } from "@fluentui/react-icons";
import type { DeviceState } from "@/api";
import { SettingCard } from "@/components";
import { t } from "@/i18n";
import type { Support } from "@/models";
import { setInspecting } from "@/store";

/**
 * In Settings, for a model hrtz hasn't been tried on or doesn't know yet: the way into its check,
 * which lives in Inspect a mouse (`MouseCheck`), opened at this mouse.
 */
export function ParticipateCard({ device, support }: { device: DeviceState; support: Exclude<Support, "tested"> }) {
  return (
    <SettingCard
      icon={<PersonFeedback20Regular />}
      title={support === "untested" ? t("Help test your {{model}}", { model: device.model }) : t("Help add your mouse")}
      description={
        support === "untested"
          ? t("A 2-minute check of what works on this model, saved as a report for GitHub.")
          : t("hrtz doesn't know this model yet. A 1-minute check saves what it needs to add it.")
      }
    >
      <Button onClick={() => setInspecting(true, device.key)}>{t("Open check")}</Button>
    </SettingCard>
  );
}
