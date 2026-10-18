import { invoke } from "@tauri-apps/api/core";

export interface AppInfo {
  version: string;
  isAdmin: boolean;
}

export type DriveKind = "ssd" | "hdd" | "removable" | "unknown";

export interface DriveInfo {
  letter: string;
  root: string;
  label: string;
  fileSystem: string;
  kind: DriveKind;
  totalBytes: number;
  freeBytes: number;
  supportsFastScan: boolean;
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  listDrives: () => invoke<DriveInfo[]>("list_drives"),
  restartAsAdmin: () => invoke<void>("restart_as_admin"),
};
