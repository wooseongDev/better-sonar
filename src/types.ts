export type ConnectionStatus =
  | 'connected'
  | 'gg_not_running'
  | 'sonar_disabled'
  | 'sonar_starting'
  | 'wrong_mode'
  | 'api_changed'
  | 'communication_error'

export interface AudioDevice {
  id: string
  name: string
  state: string
  channels: number
}

export interface AppSettings {
  headsetDeviceId: string | null
  speakerDeviceId: string | null
  shortcut: string
  autostart: boolean
  mediaKeysEnabled: boolean
}

export interface AppSnapshot {
  status: ConnectionStatus
  message: string
  mode: string | null
  devices: AudioDevice[]
  inputDevices: AudioDevice[]
  personalDeviceId: string | null
  personalDeviceName: string | null
  streamDeviceId: string | null
  streamDeviceName: string | null
  micDeviceId: string | null
  micDeviceName: string | null
  settings: AppSettings
  lastUpdatedAt: number
}

export interface UpdateInfo {
  currentVersion: string
  version: string
  notes: string | null
  publishedAt: string | null
}

export interface UpdateProgress {
  stage: 'downloading' | 'installing'
  downloaded: number
  total: number | null
}
