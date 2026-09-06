import type { RemixiconComponentType } from "@remixicon/react";
import { RiRecordCircleLine, RiSettings4Line } from "@remixicon/react";

export type PopupFeatureId = "record" | "connection";

export type PopupView = "main" | "features" | PopupFeatureId;

export type PopupFeature = {
  id: PopupFeatureId;
  icon: RemixiconComponentType;
  titleKey: "popup.record.sectionTitle" | "popup.connection.sectionTitle";
  descKey: "popup.record.cardDesc" | "popup.connection.cardDesc";
};

export const POPUP_FEATURES: PopupFeature[] = [
  {
    id: "record",
    icon: RiRecordCircleLine,
    titleKey: "popup.record.sectionTitle",
    descKey: "popup.record.cardDesc",
  },
  {
    id: "connection",
    icon: RiSettings4Line,
    titleKey: "popup.connection.sectionTitle",
    descKey: "popup.connection.cardDesc",
  },
];
