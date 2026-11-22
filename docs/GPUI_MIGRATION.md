# Better Sonar GPUI 전환 계획

Better Sonar의 Windows 앱을 GPUI로 옮긴다. 기존 Sonar 동작, 사용자 설정, 트레이 사용 방식, 앱 내 업데이트를 유지한다. 첫 GPUI 릴리스는 `v0.0.7`을 목표로 한다. Windows x64에서 NSIS와 MSI의 `v0.0.6` → `v0.0.7` 앱 내 업데이트가 모두 통과하면 배포한다.

## 목표 구조

| 영역 | 소유하는 상태와 책임 |
| --- | --- |
| `better-sonar/src/state.rs`, `sonar_client.rs`, `discovery.rs`, `gg_shortcuts.rs` | 기존 `AppRuntime`이 Sonar 연결, 장치 변경, 저장된 설정의 최신 상태를 관리한다. Tokio가 네트워크 요청, 재연결, 미디어 키 반복 처리를 실행한다. |
| `better-sonar/src/app.rs` | 창, 트레이, 글로벌 단축키에서 들어온 동작을 Rust 타입으로 받아 같은 실행 경로로 보낸다. 한 작업 큐가 상태 갱신과 장치 변경의 순서를 정한다. 화면과 트레이는 같은 `AppSnapshot`을 받고, 오류와 업데이트 진행은 별도 이벤트로 받는다. |
| `better-sonar/src/ui.rs` | GPUI가 한국어 상태 화면과 설정을 그린다. 저장 전 입력값은 화면이 관리한다. 백그라운드 갱신은 표시 상태만 바꾼다. |
| `better-sonar/src/windows.rs` | Win32 메시지 루프가 있는 스레드에서 트레이와 글로벌 키를 등록한다. 자동 시작, 단일 인스턴스, 창 표시와 숨김을 연결한다. |
| `better-sonar/src/updates.rs`, `packaging/windows/` | Rust 업데이트 모듈과 NSIS, WiX 설치 프로그램이 버전 확인, 서명 검증, 교체 설치, 재시작을 맡는다. |

기존 Rust 앱 크레이트를 GPUI 앱으로 옮기고 `sonar-probe`는 현재처럼 별도 도구로 둔다. `sonar-core`를 별도 크레이트로 만드는 안도 검토했다. 지금은 `AppRuntime`을 사용하는 앱이 하나이고, `sonar-probe`는 실제 Sonar API를 독립적으로 확인한다. 앱 크레이트 안의 모듈 경계면 Sonar 로직을 UI 없이 테스트할 수 있다.

동작 타입은 `Refresh`, `TogglePersonalOutput`, `SetPersonalOutput`, `SetStreamOutput`, `SetMicInput`, `SaveSettings`, `ControlPersonalVolume`으로 시작한다. 출력과 입력의 선택값은 각 목록에서 확인한다. 주기 갱신과 장치 변경은 기존 `AppRuntime`의 직렬화 규칙을 따른다. 설정 저장은 단축키, 자동 시작, 미디어 키 적용과 파일 쓰기를 한 작업으로 처리한다. 중간에 실패하면 이전 값으로 복구하고, 복구에도 실패하면 실제 등록 상태를 다시 읽어 오류를 표시한다.

## 구현 순서

PR 1은 기본 브랜치에서 시작한다. PR 2는 PR 1을 바탕으로 하고, 이후 PR은 직전 PR을 바탕으로 한다. 전환 브랜치에서 마지막 PR까지 검증한 뒤 전체를 통합한다. 기존 앱의 배포 태그는 그대로 유지한다. 창과 트레이 동작이 바뀌는 PR 2와 PR 3은 Windows에서 직접 사용해 보고 검토한다.

### PR 1. Windows에서 GPUI 창 수명과 입력 확인

- [ ] 임시 Cargo 프로젝트에서 화면 열기, 닫아서 숨기기, 트레이로 다시 열기, 창 없이 시작하기, 트레이에서 종료하기를 실행한다. 창을 숨긴 상태에서 글로벌 단축키와 세 음량 키의 이벤트가 도착하는지 확인한다.
- [ ] 같은 스레드의 Windows 메시지 루프에서 `tray-icon`과 `global-hotkey`를 먼저 검증한다. 트레이 메뉴가 열린 동안 이벤트가 막히거나 창의 닫기 동작을 지원하기 어렵다면 Win32 어댑터에 필요한 동작을 맡기는 안을 검증한다.
- [ ] MSVC 로컬과 CI에서 통과한 GPUI 버전이나 Git 리비전을 고정한다. 저장소 변경은 의존성과 Windows 스모크 검사에 한정한다. 한국어 표시, 키보드 포커스, DPI 변경을 확인한다. [GPUI 공식 문서](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md) [tray-icon 스레드 요구 사항](https://docs.rs/tray-icon/latest/tray_icon/) [global-hotkey 스레드 요구 사항](https://docs.rs/global-hotkey/latest/global_hotkey/)

### PR 2. 기존 Sonar 로직에 GPUI 화면 연결

- [ ] `src-tauri/src/{state,sonar_client,discovery,gg_shortcuts,models,settings}.rs`의 도메인 코드를 옮긴다. `commands.rs`의 작업을 Rust 타입 호출로 연결하고 `src/App.tsx`의 연결 상태, 현재 장치, 헤드셋과 스피커 선택, 새로고침, 전환, 설정 저장을 GPUI에서 구현한다.
- [ ] 로컬 HTTP fixture에서 Classic 모드의 쓰기 차단, 연결 끊김과 재연결, Personal Mix 전환 시 Stream Mix 보존을 확인한다. 실제 Windows 앱에서 설정을 편집하는 동안 2초 주기 갱신이 입력값을 덮어쓰지 않는지 확인한다.
- [ ] 실제 GG에서 헤드셋과 스피커를 전환한다. 앱과 GG가 같은 Personal Mix 장치를 표시하고, 전환 전후 Stream Mix가 동일해야 한다. 필요할 때 `sonar-probe`로 실제 API 상태를 독립적으로 비교한다.

### PR 3. 트레이, 단축키, 자동 시작 복원

- [ ] `tray.rs`, `shortcuts.rs`, `lib.rs`의 Windows 동작을 새 앱에 연결한다. 트레이에서 Personal Mix, Stream Mix, 마이크를 각각 선택하고, 전환 후 대상 항목에만 선택 표시가 붙는지 확인한다. 메뉴를 2초 이상 열어 둔 상태에서 표시 내용이 같으면 메뉴가 유지돼야 한다.
- [ ] Windows의 Roaming AppData에서 기존 `dev.wooseong.better-sonar/settings.json`을 같은 형식으로 읽는다. 기존 파일에서 `mediaKeysEnabled`가 빠진 경우 기본값은 `true`다. 자동 시작 항목의 기존 등록 이름과 사용자 활성 상태를 유지하고, 활성 항목의 실행 경로와 `--autostart` 인자를 갱신한다.
- [ ] 이전 설정을 넣고 실제 앱을 실행한다. 장치 ID, 단축키, 미디어 키 옵션, 자동 시작 상태가 유지돼야 한다. 자동 시작으로는 트레이만 뜨고 일반 실행을 한 번 더 하면 기존 창이 나타나야 한다. 같은 사용자 세션에서 프로세스가 하나만 남는지 확인한다.
- [ ] 창을 숨긴 상태에서 사용자 단축키, 음량 5% 증감, 음소거, 누르고 떼기, 미디어 키 옵션을 껐다 다시 켜기를 검증한다. 단축키 충돌 시 기존 키가 유지되고, 미디어 키가 일부 등록에 실패해도 가능한 키와 출력 전환이 동작하는지 확인한다.

### PR 4. 기존 설치본에서 GPUI로 업데이트

- [ ] `packaging/windows/`, `updates.rs`, `.github/workflows/release-windows.yml`에 NSIS와 WiX 빌드를 연결한다. v0.0.6 MSI에서 확인한 `UpgradeCode`는 `{CFCC0653-D654-55D1-B502-F37C4A5F8523}`, 언어는 `1042`, 설치 범위는 `ALLUSERS=1`이다. NSIS는 기존 설치 식별자, 설치 범위와 제거 등록을 실제 설치본에서 확인한다. [Microsoft의 MSI 업그레이드 문서](https://learn.microsoft.com/en-us/windows/win32/msi/preparing-an-application-for-future-major-upgrades)
- [ ] 기존 공개 키로 검증할 수 있는 설치 파일 서명과 GitHub Releases의 `latest.json`을 유지한다. `windows-x86_64-nsis`와 `windows-x86_64-msi`는 각각 같은 방식의 설치 파일을 가리키고, 일반 `windows-x86_64`는 기존처럼 NSIS를 가리킨다. 새 앱도 설치 방식을 알아내 같은 파일 형식을 선택한다. 서명, 버전, 파일 형식을 확인한 뒤 사용자 승인에 따라 설치하고 재시작한다. [v0.0.6 배포본](https://github.com/wooseongDev/better-sonar/releases/tag/v0.0.6) [기존 업데이트 규격](https://v2.tauri.app/plugin/updater/)
- [ ] 앱 시작 10초 후와 실행 중 6시간마다 업데이트를 확인한다. 창을 다시 열면 현재 확인 결과와 다운로드 진행 상태를 표시한다. 설치 중에는 추가 설치 요청을 받지 않는다. 릴리스 작업은 서명, 배포 파일 경로, 체크섬을 검증한 뒤 초안을 만든다.
- [ ] Windows 테스트 환경에서 NSIS와 MSI 각각 `v0.0.7` 신규 설치, `v0.0.6` → `v0.0.7` 앱 내 업데이트, `v0.0.7` → 상위 테스트 버전 앱 내 업데이트, 제거를 검증한다. 업데이트 후 설치 목록에 앱이 하나만 보이고, 설정과 자동 시작이 보존되며, 새 버전이 재시작돼야 한다. 서명이 틀리거나 확인한 버전이 바뀌면 설치 프로그램 실행 없이 오류가 표시돼야 한다. 공개 릴리스 게시 전에 테스트 피드에서 완료한다.

### PR 5. GPUI 빌드로 교체

- [ ] GPUI를 기본 실행 경로로 전환한다. `src-tauri/`의 Tauri 전용 코드, React UI, JavaScript 브리지, Vite, pnpm, Tauri 릴리스 액션을 제거한다. Windows 아이콘을 새 패키징 경로로 옮긴다. Cargo 버전, 설치 파일, 업데이트 manifest가 모두 `0.0.7`이고 릴리스 태그가 `v0.0.7`인지 확인한다.
- [ ] `README.md`, `lefthook.yml`, Windows 및 WSL 테스트 스크립트와 관련 문서를 새 Cargo 명령과 설치 경로에 맞춘다. 깨끗한 Windows 환경에서 저장소를 빌드하고 설치 파일을 실행한다. Tauri, Wry, WebView2가 앱의 런타임 의존성에서 빠졌는지 확인한다.
- [ ] `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`, `cargo test --locked --workspace`를 실행한다. 새 설치본으로 닫기, 트레이, 단축키와 업데이트까지 다시 확인한다.

## 성능과 남은 검증

동일한 Windows 환경에서 v0.0.6의 창이 뜰 때까지 걸리는 시간, 트레이 대기 메모리, 단축키 입력부터 전환 확인까지의 시간, NSIS와 MSI 파일 크기를 먼저 기록한다. PR 1 ~ 3과 최종 설치본의 실행 수치를 비교한다. PR 4에서는 설치 파일 크기와 설치 완료 시간을 비교한다. 중앙값이 v0.0.6의 120%를 넘으면 원인을 확인하는 검토 기준으로 삼는다. 이 비율은 제안값이며 실제 성능 측정 결과는 아니다.

## 구현 후 확인한 동작

Windows에서 GPUI 앱을 실행했다. 트레이 시작, 단일 인스턴스, 창 숨김과 재표시를 확인했다. 실제 Sonar에서 장치 목록을 읽고 GPUI 화면에서 선택한 장치를 임시 설정 파일에 저장했다. 로컬 테스트는 Personal Mix 변경에서 Stream Mix를 보존하는지, 서명된 파일의 버전과 내용이 바뀌면 설치를 거부하는지 확인한다.

NSIS와 MSI의 `v0.0.7` 설치 파일을 만들었다. MSI를 관리 이미지로 추출했을 때 GPUI 바이너리와 설치 방식 파일이 일치했다. 기존 설치본과 실제 사용 중인 설정을 보존하기 위해, 서명된 `v0.0.6` → `v0.0.7` 교체 설치와 물리 미디어 키 검증은 격리된 Windows에서 진행한다.
