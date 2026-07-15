import { invoke } from '@tauri-apps/api/core'
import type { AppSettings, AppSnapshot, UpdateInfo } from './types'

export const getSnapshot = () => invoke<AppSnapshot>('get_snapshot')
export const refresh = () => invoke<AppSnapshot>('refresh_state')
export const toggleOutput = () => invoke<AppSnapshot>('toggle_output')
export const setOutput = (deviceId: string) => invoke<AppSnapshot>('set_output', { deviceId })
export const saveSettings = (settings: AppSettings) => invoke<AppSnapshot>('save_settings', { settings })
export const checkForUpdate = () => invoke<UpdateInfo | null>('check_for_update')
export const installUpdate = (expectedVersion: string) => invoke<void>('install_update', { expectedVersion })
