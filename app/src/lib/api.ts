/**
 * Typed wrappers over the Tauri commands in app/src-tauri/src/commands.rs.
 * The shapes mirror `starorg-strings channels --json` (schema 1).
 */
import { invoke } from '@tauri-apps/api/core';

export interface ChannelView {
  channel: string;
  branch: string | null;
  game_version: string | null;
  change: number | null;
  version: string | null;
  build_error: string | null;
}

export interface InstallView {
  root: string;
  prefix: string | null;
  found_by: string;
  found_by_label: string;
  channels: ChannelView[];
}

export interface ChannelsView {
  schema: number;
  installs: InstallView[];
}

export interface AppInfo {
  name: string;
  version: string;
}

export const listChannels = () => invoke<ChannelsView>('list_channels');
export const appInfo = () => invoke<AppInfo>('app_info');
