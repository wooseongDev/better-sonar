import { useCallback, useEffect, useMemo, useState } from 'react'
import { getVersion } from '@tauri-apps/api/app'
import { listen } from '@tauri-apps/api/event'
import { checkForUpdate, getSnapshot, installUpdate, refresh, saveSettings, setOutput, toggleOutput } from './api'
import type { AppSettings, AppSnapshot, ConnectionStatus, UpdateInfo, UpdateProgress } from './types'

const UPDATE_CHECK_DELAY_MS = 10_000
const UPDATE_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1_000

const statusLabels: Record<ConnectionStatus, string> = {
  connected: '연결됨',
  gg_not_running: 'GG가 실행되지 않음',
  sonar_disabled: 'Sonar가 꺼짐',
  sonar_starting: 'Sonar 연결 중',
  wrong_mode: 'Streamer 모드가 아님',
  api_changed: 'API 호환성 오류',
  communication_error: '통신 오류',
}

const emptySnapshot: AppSnapshot = {
  status: 'sonar_starting',
  message: 'Sonar 연결을 확인하고 있습니다',
  mode: null,
  devices: [],
  inputDevices: [],
  personalDeviceId: null,
  personalDeviceName: null,
  streamDeviceId: null,
  streamDeviceName: null,
  micDeviceId: null,
  micDeviceName: null,
  settings: {
    headsetDeviceId: null,
    speakerDeviceId: null,
    shortcut: 'Ctrl+Alt+F9',
    autostart: false,
    mediaKeysEnabled: true,
  },
  lastUpdatedAt: Date.now(),
}

function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot>(emptySnapshot)
  const [draft, setDraft] = useState<AppSettings>(emptySnapshot.settings)
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)
  const [currentVersion, setCurrentVersion] = useState('')
  const [availableUpdate, setAvailableUpdate] = useState<UpdateInfo | null>(null)
  const [checkingUpdate, setCheckingUpdate] = useState(false)
  const [installingUpdate, setInstallingUpdate] = useState(false)
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress | null>(null)

  const checkUpdates = useCallback(async (showUpToDate: boolean) => {
    setCheckingUpdate(true)
    try {
      const update = await checkForUpdate()
      setAvailableUpdate(update)
      if (update) {
        setNotice(`Better Sonar ${update.version} 업데이트를 사용할 수 있습니다`)
      } else if (showUpToDate) {
        setNotice('최신 버전을 사용하고 있습니다')
      }
    } catch (error) {
      if (showUpToDate) setNotice(`업데이트 확인에 실패했습니다: ${String(error)}`)
    } finally {
      setCheckingUpdate(false)
    }
  }, [])

  useEffect(() => {
    let alive = true
    const unlisteners: Array<() => void> = []
    getSnapshot()
      .then((value) => {
        if (!alive) return
        setSnapshot(value)
        setDraft(value.settings)
      })
      .catch((error) => setNotice(String(error)))
    getVersion()
      .then(setCurrentVersion)
      .catch(() => undefined)

    Promise.all([
      listen<AppSnapshot>('sonar-state', ({ payload }) => {
        if (!alive) return
        setSnapshot(payload)
      }),
      listen<string>('sonar-error', ({ payload }) => setNotice(payload)),
      listen<UpdateProgress>('update-progress', ({ payload }) => setUpdateProgress(payload)),
    ]).then((values) => unlisteners.push(...values))

    const initialCheck = window.setTimeout(() => void checkUpdates(false), UPDATE_CHECK_DELAY_MS)
    const periodicCheck = window.setInterval(() => void checkUpdates(false), UPDATE_CHECK_INTERVAL_MS)
    return () => {
      alive = false
      window.clearTimeout(initialCheck)
      window.clearInterval(periodicCheck)
      unlisteners.forEach((unlisten) => unlisten())
    }
  }, [checkUpdates]) // 이벤트 구독과 업데이트 스케줄은 앱 수명 동안 한 번만 필요하다.

  const activeDevices = useMemo(
    () => snapshot.devices.filter((device) => device.state === 'active'),
    [snapshot.devices],
  )
  const settingsChanged = JSON.stringify(draft) !== JSON.stringify(snapshot.settings)
  const configured = Boolean(draft.headsetDeviceId && draft.speakerDeviceId)
  const isHeadset = snapshot.personalDeviceId === draft.headsetDeviceId
  const nextLabel = isHeadset ? '스피커로 전환' : '헤드셋으로 전환'

  async function run(action: () => Promise<AppSnapshot>, success?: string) {
    setBusy(true)
    setNotice(null)
    try {
      const value = await action()
      setSnapshot(value)
      setDraft(value.settings)
      if (success) setNotice(success)
    } catch (error) {
      setNotice(String(error))
    } finally {
      setBusy(false)
    }
  }

  async function applyUpdate() {
    if (!availableUpdate) return
    setInstallingUpdate(true)
    setUpdateProgress({ stage: 'downloading', downloaded: 0, total: null })
    setNotice(null)
    try {
      await installUpdate(availableUpdate.version)
    } catch (error) {
      setNotice(String(error))
      setInstallingUpdate(false)
      setUpdateProgress(null)
    }
  }

  const progressPercent =
    updateProgress?.total && updateProgress.total > 0
      ? Math.min(100, Math.round((updateProgress.downloaded / updateProgress.total) * 100))
      : null

  return (
    <main className="shell">
      <header className="hero">
        <div className="brand-mark" aria-hidden="true">
          <span />
        </div>
        <div>
          <p className="eyebrow">PERSONAL MIX SWITCHER</p>
          <h1>Better Sonar</h1>
          <p className="subtitle">스트림은 그대로, 내가 듣는 장치만 빠르게.</p>
        </div>
      </header>

      <section className={`status-card status-${snapshot.status}`} aria-live="polite">
        <div className="status-line">
          <span className="status-dot" />
          <strong>{statusLabels[snapshot.status]}</strong>
          <button className="icon-button" onClick={() => run(refresh)} disabled={busy} aria-label="새로 고침">
            ↻
          </button>
        </div>
        <p>{snapshot.message}</p>
        <div className="status-meta">
          <span>
            모드 <b>{snapshot.mode === 'stream' ? 'Sonar for Streamers' : '—'}</b>
          </span>
          <span>
            장치 <b>{activeDevices.length}</b>
          </span>
        </div>
      </section>

      <section className="current-card">
        <p className="section-label">현재 PERSONAL MIX</p>
        <div className="device-now">
          <div className="device-icon">{isHeadset ? '◖' : '◒'}</div>
          <div>
            <h2>{snapshot.personalDeviceName ?? '장치 확인 불가'}</h2>
            <p>
              {isHeadset
                ? '헤드셋'
                : snapshot.personalDeviceId === draft.speakerDeviceId
                  ? '스피커'
                  : '선택되지 않은 장치'}
            </p>
          </div>
        </div>
        <button
          className="switch-button"
          onClick={() => run(toggleOutput, 'Personal Mix 출력을 전환했습니다')}
          disabled={busy || snapshot.status !== 'connected' || !configured || settingsChanged}
        >
          <span>{busy ? '확인 중…' : nextLabel}</span>
          <kbd>↔</kbd>
        </button>
        {settingsChanged && <p className="hint">장치 설정을 저장한 뒤 전환할 수 있습니다.</p>}
      </section>

      <section className="panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">출력 장치</p>
            <h3>전환할 두 장치 선택</h3>
          </div>
        </div>
        <label>
          <span>헤드셋</span>
          <select
            value={draft.headsetDeviceId ?? ''}
            onChange={(event) => setDraft({ ...draft, headsetDeviceId: event.target.value || null })}
          >
            <option value="">장치를 선택하세요</option>
            {activeDevices.map((device) => (
              <option value={device.id} key={device.id}>
                {device.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          <span>스피커</span>
          <select
            value={draft.speakerDeviceId ?? ''}
            onChange={(event) => setDraft({ ...draft, speakerDeviceId: event.target.value || null })}
          >
            <option value="">장치를 선택하세요</option>
            {activeDevices.map((device) => (
              <option value={device.id} key={device.id}>
                {device.name}
              </option>
            ))}
          </select>
        </label>
        <div className="quick-actions">
          <button
            disabled={busy || !draft.headsetDeviceId || snapshot.status !== 'connected'}
            onClick={() => run(() => setOutput(draft.headsetDeviceId!), '헤드셋으로 전환했습니다')}
          >
            헤드셋 테스트
          </button>
          <button
            disabled={busy || !draft.speakerDeviceId || snapshot.status !== 'connected'}
            onClick={() => run(() => setOutput(draft.speakerDeviceId!), '스피커로 전환했습니다')}
          >
            스피커 테스트
          </button>
        </div>
      </section>

      <section className="panel settings-panel">
        <div className="panel-heading">
          <div>
            <p className="section-label">앱 설정</p>
            <h3>빠른 실행</h3>
          </div>
        </div>
        <label>
          <span>글로벌 단축키</span>
          <input
            value={draft.shortcut}
            onChange={(event) => setDraft({ ...draft, shortcut: event.target.value })}
            placeholder="Ctrl+Alt+F9"
            spellCheck={false}
          />
          <small>예: Ctrl+Alt+F9, Super+Shift+S</small>
        </label>
        <label className="toggle-row">
          <span>
            <b>Fn 미디어 키 액션</b>
            <small>Fn + F10/F11/F12로 음소거와 Personal 음량을 제어합니다.</small>
          </span>
          <input
            type="checkbox"
            checked={draft.mediaKeysEnabled}
            onChange={(event) => setDraft({ ...draft, mediaKeysEnabled: event.target.checked })}
          />
        </label>
        <label className="toggle-row">
          <span>
            <b>Windows 시작 시 실행</b>
            <small>로그인하면 트레이에서 자동으로 시작합니다.</small>
          </span>
          <input
            type="checkbox"
            checked={draft.autostart}
            onChange={(event) => setDraft({ ...draft, autostart: event.target.checked })}
          />
        </label>
        <button
          className="save-button"
          disabled={busy || !settingsChanged}
          onClick={() => run(() => saveSettings(draft), '설정을 저장했습니다')}
        >
          설정 저장
        </button>
      </section>

      <section className="panel update-panel" aria-live="polite">
        <div className="panel-heading update-heading">
          <div>
            <p className="section-label">앱 업데이트</p>
            <h3>{availableUpdate ? `새 버전 ${availableUpdate.version}` : 'Better Sonar 최신 상태'}</h3>
          </div>
          {currentVersion && <span className="version-chip">현재 {currentVersion}</span>}
        </div>

        {availableUpdate ? (
          <>
            <p className="update-message">
              설치가 끝나면 앱이 자동으로 재시작됩니다. 스트리밍 중이라면 작업이 끝난 뒤 설치하세요.
            </p>
            {availableUpdate.notes && <div className="release-notes">{availableUpdate.notes}</div>}
            {installingUpdate && (
              <div className="update-progress">
                <div className="progress-track">
                  <span style={{ width: `${progressPercent ?? 12}%` }} />
                </div>
                <small>
                  {updateProgress?.stage === 'installing'
                    ? '서명을 검증하고 업데이트를 설치하는 중…'
                    : progressPercent === null
                      ? '업데이트를 다운로드하는 중…'
                      : `업데이트 다운로드 ${progressPercent}%`}
                </small>
              </div>
            )}
            <div className="update-actions">
              <button disabled={checkingUpdate || installingUpdate} onClick={() => void checkUpdates(false)}>
                다시 확인
              </button>
              <button className="install-button" disabled={installingUpdate} onClick={() => void applyUpdate()}>
                {installingUpdate ? '업데이트 중…' : '다운로드 및 설치'}
              </button>
            </div>
          </>
        ) : (
          <>
            <p className="update-message">시작 후와 실행 중 6시간마다 새 버전을 자동으로 확인합니다.</p>
            <button
              className="save-button"
              disabled={checkingUpdate || installingUpdate}
              onClick={() => void checkUpdates(true)}
            >
              {checkingUpdate ? '확인 중…' : '업데이트 확인'}
            </button>
          </>
        )}
      </section>

      {notice && (
        <div className="toast" role="status" onClick={() => setNotice(null)}>
          {notice}
        </div>
      )}
      <footer>창을 닫아도 시스템 트레이에서 계속 실행됩니다.</footer>
    </main>
  )
}

export default App
