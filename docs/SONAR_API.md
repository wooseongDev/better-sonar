# Sonar 비공식 로컬 API

> 이 문서는 Better Sonar가 의존하는 비공식 로컬 API의 호환성 메모입니다. SteelSeries가 공개하거나 안정성을 보장하는 API 계약이 아닙니다.

## 확인 범위

다음 환경에서 동작을 확인했습니다.

- Windows 11
- SteelSeries GG 114
- SteelSeries Sonar 1.97
- 일반 사용자 권한으로 실행한 GG, Engine 및 Sonar

다른 버전에서는 경로, 필드 또는 동작이 달라질 수 있습니다.

## API 탐색

Better Sonar는 GG의 로컬 설정에서 GG API 주소를 확인한 뒤 `/subApps` 응답에 게시된 Sonar API 주소를 사용합니다. 주소를 고정하지 않으므로 GG가 재시작되어 Sonar의 포트가 변경되어도 다시 탐색할 수 있습니다.

탐색과 요청은 루프백 주소로 제한하며 인증서 본문이나 인증 가능성이 있는 메타데이터를 로그, UI 또는 설정 파일에 저장하지 않습니다.

## 사용하는 엔드포인트

| 목적                   | 메서드와 경로                                                        | 확인 항목                                                             |
| ---------------------- | -------------------------------------------------------------------- | --------------------------------------------------------------------- |
| Sonar 상태 및 API 탐색 | `GET /subApps` (GG HTTPS)                                            | `isEnabled`, `isReady`, `isRunning`, `metadata.webServerAddress`      |
| 모드 조회              | `GET /mode`                                                          | Streamer 모드는 JSON 문자열 `"stream"`                                |
| 장치 목록              | `GET /audioDevices`                                                  | 물리 장치는 `role=none`, `isVad=false`; 출력=`render`, 입력=`capture` |
| 믹스 출력·마이크 조회  | `GET /streamRedirections`                                            | Personal=`monitoring`, Stream=`streaming`, 마이크=`mic`               |
| Personal 출력 변경     | `PUT /streamRedirections/monitoring/deviceId/{URL-encoded deviceId}` | 응답 후 상태 재조회                                                   |
| Stream 출력 변경       | `PUT /streamRedirections/streaming/deviceId/{URL-encoded deviceId}`  | 응답 후 상태 재조회                                                   |
| 마이크 입력 변경       | `PUT /streamRedirections/mic/deviceId/{URL-encoded deviceId}`        | 활성 물리 캡처 장치인지 확인 후 상태 재조회                           |
| Streamer 음량 조회     | `GET /volumeSettings/streamer`                                       | `masters.stream.monitoring`이 Master - Personal                       |

각 리디렉션 변경 전후에 나머지 두 리디렉션의 전체 상태를 비교합니다. 선택하지 않은 믹스나 마이크가 함께 달라지면 전환 실패로 처리합니다.

미디어 키는 위 HTTP 쓰기 API를 직접 호출하지 않습니다. Sonar 프로세스가 GG에서 받은 `GG_WS_ENDPOINT`와 `GG_API_AUTH_TOKEN`을 같은 사용자 권한으로 읽어 GG 이벤트 소켓에 인증한 뒤, Sonar가 자체 단축키에 사용하는 `EVENT_KEYBOARD_SHORTCUT`을 전송합니다. Master - Personal 동작 ID는 증가 `22`, 감소 `23`, 음소거 토글 `24`입니다. 이 경로를 사용해야 Sonar가 값을 변경한 뒤 `SONAR_EVENT_VOLUME_DATA`를 발행하므로 GG 믹서 UI도 동기화됩니다. 적용 결과는 `GET /volumeSettings/streamer`로 다시 확인합니다.

## 검증한 동작

실제 Sonar 환경과 로컬 모의 서버를 사용해 다음 동작을 확인합니다.

- Streamer 모드 판별
- 활성 물리 재생 장치 필터링
- Personal Mix와 Stream Mix 상태 해석
- Personal Mix 변경 후 결과 재조회
- 비활성·가상·입력 장치로의 변경 거부
- 요청이 적용되지 않은 경우 감지
- Stream Mix 변경 감지
- GG 재시작 후 Sonar API 재탐색
- 변경한 Personal Mix의 원래 장치 복원
- Master - Personal 음량을 5% 단위로 증감하고 원래 값으로 복원
- Master - Personal 음소거 전환과 원래 상태 복원

검증 과정에서는 GG 파일이나 사용자 Sonar 설정 파일을 직접 수정하지 않습니다.

## 안전장치와 제한

- GG API의 자체 서명 인증서에 대한 예외는 GG 탐색 요청에만 적용합니다.
- GG와 Sonar API 주소가 루프백인지 확인하고 외부 호스트로의 요청을 거부합니다.
- 모든 요청에 제한 시간을 적용하고 실패하면 Sonar API를 다시 탐색합니다.
- 장치는 고유 `deviceId`로 식별하며 장치가 없거나 비활성 상태이면 변경을 거부합니다.
- 변경 요청 후 상태를 다시 조회해 실제 적용 여부를 확인합니다.
- Master - Personal 단축키 이벤트만 보내며 `masters.stream.streaming`에는 쓰기 요청을 보내지 않습니다.
- GG 업데이트로 API가 변경되면 Better Sonar도 업데이트가 필요할 수 있습니다.
