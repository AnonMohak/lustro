import i18n from "../i18n";
import type { DriveInfo } from "./api";

export function driveName(d: DriveInfo): string {
  const label = d.label || i18n.t("drive.localDisk");
  return i18n.t("drive.nameWithLetter", { label, letter: d.letter });
}

export function driveIcon(d: DriveInfo) {
  return d.kind === "removable" ? ("usb" as const) : ("drive" as const);
}
