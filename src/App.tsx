import { useEffect, useMemo, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { getSnapshot, refresh, saveSettings, setOutput, toggleOutput } from './api'
import type { AppSettings, AppSnapshot, ConnectionStatus } from './types'

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
  personalDeviceId: null,
  personalDeviceName: null,
  streamDeviceId: null,
  settings: {
    headsetDeviceId: null,
    speakerDeviceId: null,
    shortcut: 'Ctrl+Alt+F9',
    autostart: false,
  },
  lastUpdatedAt: Date.now(),
}

function App() {
  const [snapshot, setSnapshot] = useState<AppSnapshot>(emptySnapshot)
  const [draft, setDraft] = useState<AppSettings>(emptySnapshot.settings)
  const [busy, setBusy] = useState(false)
  const [notice, setNotice] = useState<string | null>(null)

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

    Promise.all([
      listen<AppSnapshot>('sonar-state', ({ payload }) => {
        if (!alive) return
        setSnapshot(payload)
      }),
      listen<string>('sonar-error', ({ payload }) => setNotice(payload)),
    ]).then((values) => unlisteners.push(...values))
    return () => {
      alive = false
      unlisteners.forEach((unlisten) => unlisten())
    }
  }, []) // 이벤트 구독은 앱 수명 동안 한 번만 필요하다.

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
