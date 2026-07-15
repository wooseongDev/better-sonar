# Better Sonar

Better Sonar는 SteelSeries GG Sonar를 더 간편하게 사용할 수 있도록 도와주는 비공식 Windows 응용프로그램입니다.

## 주요 기능

- GG/Sonar 및 Streamer 모드 자동 감지
- 활성 물리 재생 장치와 현재 Personal Mix 표시
- 헤드셋·스피커 저장 및 원클릭 전환
- 트레이 메뉴와 사용자 지정 글로벌 단축키 지원
- Sonar Master - Personal을 직접 제어하는 음소거·음량 미디어 키 지원
- 설정 저장, Windows 자동 시작 및 단일 인스턴스 실행
- GG 재시작이나 일시적인 통신 실패 후 자동 재연결
- 출력 전환 후 결과 확인 및 Stream Mix 불변 검증

## 설치 및 사용

1. SteelSeries GG에서 Sonar를 켜고 Mixer의 **Sonar for Streamers**를 활성화합니다.
2. GitHub Releases에서 NSIS `.exe` 또는 MSI 설치 패키지를 내려받아 실행합니다.
3. 헤드셋과 스피커를 각각 선택합니다.
4. 글로벌 단축키와 자동 시작 여부를 설정하고 **설정 저장**을 누릅니다.
5. 앱 버튼, 트레이 메뉴 또는 단축키로 출력을 전환합니다.

Windows가 `VK_VOLUME_MUTE`, `VK_VOLUME_DOWN`, `VK_VOLUME_UP`으로 전달하는 키는 각각 Sonar Master - Personal의 음소거 전환, 5% 음량 감소, 5% 음량 증가로 동작합니다. 노트북에서는 보통 `Fn + F10/F11/F12`가 이 이벤트를 생성하지만, 일반 `F10/F11/F12` 자체는 등록하지 않습니다. 창을 닫아 트레이에 숨긴 뒤에도 미디어 키와 사용자 단축키는 계속 동작합니다.

앱 설정의 **Fn 미디어 키 액션**을 끄고 저장하면 세 미디어 키 등록을 즉시 해제합니다. 다시 켜고 저장하면 재등록되며 앱을 다시 시작할 필요가 없습니다.

창의 닫기 버튼은 앱을 종료하지 않고 숨깁니다. 완전히 종료하려면 트레이 메뉴의 **종료**를 사용하세요.

v0.0.3부터 앱이 시작된 뒤 새 버전을 자동으로 확인합니다. 업데이트가 있으면 앱 설정 화면에서 내용을 확인하고 **다운로드 및 설치**를 선택할 수 있으며, 설치가 끝나면 앱이 재시작됩니다. v0.0.1과 v0.0.2에는 updater가 없으므로 v0.0.3은 GitHub Releases에서 한 번 수동 설치해야 합니다.

## 문제 해결

- **GG가 실행되지 않음**: GG가 실행 중인지 확인합니다.
- **Sonar가 꺼짐/연결 중**: GG 설정에서 Sonar가 활성화되어 있는지 확인하고 잠시 기다립니다.
- **Streamer 모드가 아님**: Mixer에서 Sonar for Streamers를 활성화합니다. Classic 모드의 출력은 변경하지 않습니다.
- **저장된 장치 연결 해제**: 장치를 다시 연결하거나 설정에서 다시 선택합니다.
- **API 호환성/통신 오류**: GG 업데이트나 일시적인 통신 실패일 수 있습니다. 앱은 자동으로 재연결을 시도합니다.
- **미디어 키 등록 실패**: 다른 앱이 같은 글로벌 미디어 키를 선점했을 수 있습니다. 터미널의 `[media-key]` 진단 로그를 확인하고 충돌하는 앱의 글로벌 키 설정을 해제하세요. 일부 키만 실패해도 앱과 사용자 지정 출력 전환 단축키는 계속 동작합니다.
- **Fn 키가 동작하지 않음**: 제조사 펌웨어가 Windows에 표준 음량 가상 키를 보내는지 확인하세요. Better Sonar는 `Fn` 상태나 일반 기능 키를 직접 감시하지 않습니다.

## 개발

필수 도구:

- Node.js 24 이상 (`nvm use`로 프로젝트 권장 버전 적용)
- pnpm 11.9.0
- Rust stable
- Tauri 2의 [Windows 사전 요구 사항](https://v2.tauri.app/start/prerequisites/)

```bash
pnpm install
pnpm build
pnpm check:quality
cargo test -p better-sonar --lib
cargo test -p sonar-probe
pnpm tauri build
```

`pnpm install`은 Lefthook의 `pre-commit` 훅을 설치합니다. 전체 품질 검사는 `pnpm check:quality`, 프런트엔드는 `pnpm check:frontend`, Rust는 `pnpm check:rust`로 실행할 수 있습니다.

WSL에서 Windows 네이티브 MSVC 검사와 Tauri 빌드를 한 번에 실행할 수 있습니다.

```bash
./scripts/test-windows-from-wsl.sh
```

Windows 앱 실행은 `--run`, NSIS/MSI 생성은 `--bundle`을 사용합니다. 스크립트는 WSL 소스를 `%LOCALAPPDATA%` 아래의 격리된 Windows 작업 폴더로 동기화하므로 UNC 경로에서 직접 빌드하지 않습니다. 요구사항, 옵션, 캐시 및 안전 동작은 [WSL Windows 네이티브 테스트](docs/WINDOWS_TESTING_FROM_WSL.md)를 참고하세요.

실제 Sonar 환경을 읽기 전용으로 점검하려면 Windows에서 다음 명령을 실행합니다.

```powershell
cargo run -p sonar-probe
```

장치를 지정해 전환과 원복을 검증할 수도 있습니다. 이 명령은 Personal Mix를 전환한 뒤 원래 장치로 복원하며 각 단계에서 Stream Mix가 유지되는지 확인합니다.

```powershell
cargo run -p sonar-probe -- --switch-to "{Sonar device ID}"
```

미디어 키의 구현 및 실제 Windows 검증 절차는 [Windows 미디어 키 검증](docs/WINDOWS_MEDIA_KEYS.md)을 참고하세요.

GitHub Releases 자동 업데이트의 키 관리, 릴리스 및 실제 2버전 검증 절차는 [자동 업데이트 운영](docs/AUTO_UPDATE.md)을 참고하세요.

## 구조

- `src/` — React UI
- `src-tauri/` — Tauri 앱과 Sonar 연동
- `sonar-probe/` — 실제 Sonar API 검증 도구
- `docs/SONAR_API.md` — 앱이 의존하는 비공식 로컬 API의 호환성 메모

## 알려진 제한 사항

- Windows만 지원합니다.
- SteelSeries의 공식 공개 API가 아닌 내부 루프백 API에 의존하므로 GG 업데이트 후 호환되지 않을 수 있습니다.
- 미디어 키는 Sonar의 내부 단축키 이벤트 경로를 사용해 Master - Personal 값과 GG 믹서 UI를 함께 갱신합니다. 이 비공개 이벤트 규격은 GG 업데이트로 바뀔 수 있습니다.
- Windows 오디오 장치 재설치나 드라이버 변경 후 저장된 장치를 다시 선택해야 할 수 있습니다.
- 제조사 펌웨어가 표준 `VK_VOLUME_*` 이벤트 대신 전용 HID/유틸리티 경로를 사용하는 Fn 키는 인식하지 못할 수 있습니다.
- GG의 자동 장치 전환이 동시에 동작하면 선택이 다시 바뀔 수 있습니다.
- 앱은 Personal Mix에 해당하는 `monitoring` 출력만 변경하며 Stream Mix는 변경하지 않습니다.

자세한 기술적 호환성 정보는 [Sonar 비공식 로컬 API](docs/SONAR_API.md)를 참고하세요.

## 라이선스

[MIT](LICENSE) 라이선스로 배포합니다.
