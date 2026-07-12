# Better Sonar

Better Sonar는 SteelSeries GG Sonar를 더 간편하게 사용할 수 있도록 도와주는 비공식 Windows 응용프로그램입니다.

## 주요 기능

- GG/Sonar 및 Streamer 모드 자동 감지
- 활성 물리 재생 장치와 현재 Personal Mix 표시
- 헤드셋·스피커 저장 및 원클릭 전환
- 트레이 메뉴와 사용자 지정 글로벌 단축키 지원
- 설정 저장, Windows 자동 시작 및 단일 인스턴스 실행
- GG 재시작이나 일시적인 통신 실패 후 자동 재연결
- 출력 전환 후 결과 확인 및 Stream Mix 불변 검증

## 설치 및 사용

1. SteelSeries GG에서 Sonar를 켜고 Mixer의 **Sonar for Streamers**를 활성화합니다.
2. GitHub Releases에서 NSIS `.exe` 또는 MSI 설치 패키지를 내려받아 실행합니다.
3. 헤드셋과 스피커를 각각 선택합니다.
4. 글로벌 단축키와 자동 시작 여부를 설정하고 **설정 저장**을 누릅니다.
5. 앱 버튼, 트레이 메뉴 또는 단축키로 출력을 전환합니다.

창의 닫기 버튼은 앱을 종료하지 않고 숨깁니다. 완전히 종료하려면 트레이 메뉴의 **종료**를 사용하세요.

## 문제 해결

- **GG가 실행되지 않음**: GG가 실행 중인지 확인합니다.
- **Sonar가 꺼짐/연결 중**: GG 설정에서 Sonar가 활성화되어 있는지 확인하고 잠시 기다립니다.
- **Streamer 모드가 아님**: Mixer에서 Sonar for Streamers를 활성화합니다. Classic 모드의 출력은 변경하지 않습니다.
- **저장된 장치 연결 해제**: 장치를 다시 연결하거나 설정에서 다시 선택합니다.
- **API 호환성/통신 오류**: GG 업데이트나 일시적인 통신 실패일 수 있습니다. 앱은 자동으로 재연결을 시도합니다.

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

실제 Sonar 환경을 읽기 전용으로 점검하려면 Windows에서 다음 명령을 실행합니다.

```powershell
cargo run -p sonar-probe
```

장치를 지정해 전환과 원복을 검증할 수도 있습니다. 이 명령은 Personal Mix를 전환한 뒤 원래 장치로 복원하며 각 단계에서 Stream Mix가 유지되는지 확인합니다.

```powershell
cargo run -p sonar-probe -- --switch-to "{Sonar device ID}"
```

## 구조

- `src/` — React UI
- `src-tauri/` — Tauri 앱과 Sonar 연동
- `sonar-probe/` — 실제 Sonar API 검증 도구
- `docs/SONAR_API.md` — 앱이 의존하는 비공식 로컬 API의 호환성 메모

## 알려진 제한 사항

- Windows만 지원합니다.
- SteelSeries의 공식 공개 API가 아닌 내부 루프백 API에 의존하므로 GG 업데이트 후 호환되지 않을 수 있습니다.
- Windows 오디오 장치 재설치나 드라이버 변경 후 저장된 장치를 다시 선택해야 할 수 있습니다.
- GG의 자동 장치 전환이 동시에 동작하면 선택이 다시 바뀔 수 있습니다.
- 앱은 Personal Mix에 해당하는 `monitoring` 출력만 변경하며 Stream Mix는 변경하지 않습니다.

자세한 기술적 호환성 정보는 [Sonar 비공식 로컬 API](docs/SONAR_API.md)를 참고하세요.

## 라이선스

[MIT](LICENSE) 라이선스로 배포합니다.
