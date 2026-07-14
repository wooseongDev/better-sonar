# Windows 미디어 키와 Personal Mix 음량 제어

## 동작

Better Sonar는 Windows 전용으로 다음 글로벌 미디어 키를 등록합니다.

| Windows 이벤트                       | 동작                                 |
| ------------------------------------ | ------------------------------------ |
| `AudioVolumeMute` / `VK_VOLUME_MUTE` | Sonar Master - Personal 음소거 전환  |
| `AudioVolumeDown` / `VK_VOLUME_DOWN` | Sonar Master - Personal 음량 5% 감소 |
| `AudioVolumeUp` / `VK_VOLUME_UP`     | Sonar Master - Personal 음량 5% 증가 |

일반 `F10`, `F11`, `F12`는 등록하지 않는다. 노트북의 `Fn` 조합이 위 가상 키를 Windows에 전달할 때만 동작한다.

각 입력 시 `GET /volumeSettings/streamer`로 현재 `masters.stream.monitoring` 값을 조회한 뒤 GG 이벤트 소켓을 통해 Sonar의 Master - Personal 단축키 이벤트를 전달한다. 증가 `22`, 감소 `23`, 음소거 토글 `24`는 Sonar 자체 단축키 핸들러를 실행하며, 핸들러가 `SONAR_EVENT_VOLUME_DATA`를 발행하므로 GG 믹서 UI도 함께 갱신된다. Sonar의 기본 단축키와 같은 5% 단위를 사용하며 변경 후 값을 다시 조회해 실제 적용 여부를 확인한다. Windows 물리 출력 장치의 `IAudioEndpointVolume`은 변경하지 않으므로 Sonar 마스터와 물리 장치가 동시에 감쇠되는 문제를 피한다.

GG 이벤트 소켓 연결은 재사용한다. 인증 정보는 실행 중인 Sonar 프로세스 환경에서 읽으며 로그나 설정 파일에 저장하지 않는다. 연결이 끊어지면 다음 입력에서 한 번 재연결한다.

## 등록 및 반복 입력

현재 의존성인 `tauri-plugin-global-shortcut 2.3.2`와 `global-hotkey 0.8.0`은 세 키를 서로 다른 `Code`와 `VK_VOLUME_*` 값으로 매핑한다. Windows 구현은 `RegisterHotKey`에 `MOD_NOREPEAT`를 사용하므로 Press 이벤트 자체는 반복되지 않는다.

Better Sonar는 Down/Up의 Press 직후 한 번 실행하고, 키가 계속 눌려 있으면 350 ms 후 90 ms 간격으로 반복한다. Release 이벤트에서 반복을 중단한다. Mute는 길게 눌러도 한 번만 전환한다.

미디어 키는 사용자 지정 출력 전환 단축키와 별도로 등록한다. 세 키 중 하나라도 충돌하거나 등록에 실패하면 이미 등록한 미디어 키를 되돌리고 `sonar-error` 이벤트로 알린다. 기존 출력 전환 단축키는 유지되며, 사용자 단축키 변경도 미디어 키 등록 상태에 영향을 주지 않는다.

앱 설정의 **Fn 미디어 키 액션** 토글은 세 키를 한 번에 제어한다. 끈 상태로 저장하면 글로벌 등록을 즉시 해제해 Windows나 다른 앱이 처리할 수 있게 하고, 켠 상태로 저장하면 앱 재시작 없이 다시 등록한다. 기존 설정 파일에 해당 값이 없으면 이전 동작을 유지하기 위해 켬으로 마이그레이션한다.

## 실제 Windows 검증 절차

1. SteelSeries GG에서 Sonar for Streamers를 활성화하고 헤드셋과 스피커를 Better Sonar에 저장한다.
2. 터미널에서 `pnpm tauri dev`를 실행하고 `[media-key] ... 전역 등록 완료` 로그 세 개를 확인한다.
3. 앱 창을 닫아 트레이에 숨긴다.
4. `Fn + F10`을 누르고 Sonar Master - Personal 음소거 상태가 전환되는지 확인한다.
5. `Fn + F11/F12`를 짧게 누른 뒤 길게 눌러 단일 단계와 연속 단계 조절을 확인한다.
6. 각 입력에서 GG의 Master - Personal 볼륨 또는 음소거 오버레이가 표시되는지 확인한다.
7. 설정한 사용자 지정 출력 전환 단축키가 계속 동작하는지 확인한다.
8. 실패 시 `[media-key]` 수신·오류 로그를 기록한다. 등록 로그는 있으나 수신 로그가 없다면 노트북 펌웨어 또는 제조사 유틸리티가 표준 `VK_VOLUME_*` 이벤트를 보내는지 확인한다.

## 검증 범위와 하드웨어 제한

2026-07-14에 Windows MSVC 환경에서 다음 자동·반자동 검증을 완료했다.

- 전체 Rust 테스트와 Clippy
- Tauri Windows 디버그 애플리케이션 빌드
- 실제 실행 중인 Sonar for Streamers와 Personal Mix 물리 장치 조회
- `keybd_event`로 생성한 `VK_VOLUME_MUTE`, `VK_VOLUME_DOWN`, `VK_VOLUME_UP`의 글로벌 Press/Release 수신
- 수신한 각 이벤트에서 Sonar 자체 단축키 핸들러 실행과 Master - Personal 재조회 성공
- 직접 HTTP 쓰기와 달리 Sonar가 `SONAR_EVENT_VOLUME_DATA`를 발행하는 경로임을 확인
- 창을 닫아 트레이에 숨긴 상태에서 프로세스와 미디어 키 처리가 계속 동작함을 확인
- 기존 사용자 단축키로 Personal Mix를 스피커에서 헤드셋으로 전환하고 다시 원래 장치로 복구
- 출력 전환 직후에도 Master - Personal 미디어 키 처리 성공
- 실제 노트북의 `Fn + F10/F11/F12` 입력 수신과 음소거·음량 제어 성공

합성 입력은 Windows 가상 키 등록부터 Sonar API 제어까지의 애플리케이션 경로를 검증한다. 실제 키보드 입력 결과는 검증한 노트북에 해당하며, 제조사 펌웨어가 표준 `VK_VOLUME_*` 이벤트를 보내지 않는 다른 기종에서는 동작하지 않을 수 있다.

다음 상황은 사용자에게 진단 오류로 보고된다.

- 다른 프로세스가 선점한 글로벌 미디어 키
- Sonar 비실행, 비활성화, 시작 중 또는 Streamer 모드가 아닌 상태
- Sonar 내부 API, GG 이벤트 소켓 또는 단축키 이벤트 규격 변경
- 요청 후 Master - Personal 값이 변경되지 않은 경우
