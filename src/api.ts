import { invoke } from '@tauri-apps/api/core'
import type { AppSettings, AppSnapshot } from './types'

export const getSnapshot = () => invoke<AppSnapshot>('get_snapshot')
export const refresh = () => invoke<AppSnapshot>('refresh_state')
export const toggleOutput = () => invoke<AppSnapshot>('toggle_output')
export const setOutput = (deviceId: string) => invoke<AppSnapshot>('set_output', { deviceId })
export const saveSettings = (settings: AppSettings) => invoke<AppSnapshot>('save_settings', { settings })
