import i18n from "../i18n";
import type { DriveInfo } from "./api";
import { IS_WINDOWS } from "./platform";

export function driveName(d: DriveInfo): string {
  const label = d.label || i18n.t("drive.localDisk");
  // Other platforms have no drive letters: `letter` holds the mount point, for example "/home".
  if (!IS_WINDOWS) return i18n.t("drive.nameWithMount", { label, mount: d.letter });
  return i18n.t("drive.nameWithLetter", { label, letter: d.letter });
}

export function driveIcon(d: DriveInfo) {
  return d.kind === "removable" ? ("usb" as const) : ("drive" as const);
}
