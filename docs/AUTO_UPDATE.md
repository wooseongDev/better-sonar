# GitHub Releases 자동 업데이트

Better Sonar는 Tauri 2 updater와 GitHub Releases의 정적 `latest.json`을 사용한다. 앱은 시작 10초 후와 실행 중 6시간마다 새 버전을 확인하지만, 사용자가 **다운로드 및 설치**를 선택하기 전에는 파일을 받거나 앱을 재시작하지 않는다.

## 신뢰 모델

- 앱에는 `src-tauri/tauri.conf.json`의 updater 공개 키가 내장된다.
- GitHub Actions는 `release` Environment의 `TAURI_SIGNING_PRIVATE_KEY`와 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`로 NSIS와 MSI에 updater 서명을 만든다.
- 앱은 다운로드가 끝난 뒤 서명을 검증하며, 올바르지 않은 파일은 설치하지 않는다.
- `SHA256SUMS.txt`는 수동 다운로드 확인용이다. updater의 서명 검증을 대신하지 않는다.
- Tauri updater 서명과 Windows Authenticode 코드 서명은 별개다. updater 서명은 필수이고, SmartScreen 평판을 위한 Authenticode는 별도로 추가해야 한다.

현재 생산용 개인 키의 로컬 원본은 다음 경로에 있으며 저장소에는 포함되지 않는다.

```text
/home/wooseong/.tauri/better-sonar.key
/home/wooseong/.tauri/better-sonar.key.pub
```

요청된 로컬 백업은 다음 경로에 있다.

```text
/home/wooseong/_bak/better-sonar-updater/better-sonar.key
/home/wooseong/_bak/better-sonar-updater/better-sonar.key.pub
/home/wooseong/_bak/better-sonar-updater/signing-password.txt
```

키와 비밀번호 파일의 권한은 `600`, 상위 폴더는 `700`이어야 한다. 이 백업은 같은 컴퓨터에 있으므로 디스크 고장에 대한 독립 백업이 아니다. 디렉터리 전체를 암호화된 외부 저장소에 추가로 복제하고, 가능하면 비밀번호는 별도 비밀번호 관리자에도 저장한다. 개인 키와 비밀번호를 함께 잃으면 후속 업데이트에 서명할 수 없다.

개인 키를 잃으면 이미 설치된 앱이 새 키로 만든 업데이트를 신뢰할 수 없다. 개인 키가 유출되면 공격자가 기존 앱이 신뢰하는 파일을 서명할 수 있으므로 다음을 수행한다.

1. GitHub의 `release` Environment에서 `TAURI_SIGNING_PRIVATE_KEY`를 교체한다.
2. 유출 전 키를 신뢰하는 앱 사용자에게는 별도의 신뢰 가능한 채널로 수동 재설치를 안내한다.
3. 새 키의 공개 키가 포함된 부트스트랩 버전부터 업데이트 체인을 다시 시작한다.

## 버전과 릴리스 절차

릴리스 전에 다음 세 버전이 모두 태그와 일치해야 한다.

```text
package.json
src-tauri/Cargo.toml
src-tauri/tauri.conf.json
```

안정 버전 릴리스 순서:

1. 위 세 파일의 SemVer와 `Cargo.lock`을 갱신한다.
2. `pnpm check:quality`, Rust 테스트, Windows 네이티브 번들 검증을 통과시킨다.
3. 변경을 커밋하고 `v<version>` 태그를 푸시한다.
4. `Release Windows installers` workflow가 완료될 때까지 기다린다.
5. 초안 Release에 NSIS, MSI, 두 `.sig`, `latest.json`, `SHA256SUMS.txt`가 있는지 확인한다.
6. `latest.json`의 `windows-x86_64.url`이 NSIS `-setup.exe`를 가리키고 `signature`가 비어 있지 않은지 확인한다.
7. 초안 Release를 게시한다. 게시 이후 immutable release의 asset을 교체하지 않는다.

GitHub Actions는 초안 생성 직후 `latest.json`을 다시 내려받아 버전, `windows-x86_64-nsis`와 `windows-x86_64`의 NSIS URL, 실제 `.sig`와 manifest 서명의 일치를 자동 검증한다. 하나라도 다르면 Release workflow를 실패 처리하고 초안을 게시하지 않는다.

워크플로는 `updaterJsonPreferNsis: true`를 사용한다. 일반 사용자의 자동 업데이트 경로는 NSIS로 고정하고 MSI는 수동·관리 배포용으로 유지한다.

잘못된 버전을 배포했을 때 Release asset을 덮어쓰거나 낮은 버전으로 되돌리지 않는다. 수정한 더 높은 patch 버전을 새 Release로 배포한다.

## 기존 사용자 부트스트랩

v0.0.1과 v0.0.2에는 updater와 공개 키가 없으므로 새 기능을 소급 적용할 수 없다.

- v0.0.3을 최초 updater 부트스트랩 버전으로 배포한다.
- 기존 사용자는 v0.0.3 NSIS를 한 번 수동 설치해야 한다.
- Release 설명과 README에 이 제약을 명시한다.
- v0.0.4를 검증용 후속 버전으로 만들어 v0.0.3에서 자동 업데이트 전체 경로를 확인한다.

기존 MSI 설치 사용자는 NSIS 기반 자동 업데이트로 전환되므로 v0.0.2 MSI → v0.0.3 NSIS 수동 설치와 v0.0.3 → v0.0.4 자동 업데이트를 별도로 시험한다. 프로그램이 중복 등록되거나 제거 항목이 남는다면 MSI 사용자는 기존 버전을 제거한 뒤 NSIS를 설치하도록 안내한다.

## Windows 검증 매트릭스

릴리스 전 로컬 번들 검증:

```bash
TAURI_SIGNING_PRIVATE_KEY="$(< /home/wooseong/.tauri/better-sonar.key)" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(tr -d '\r\n' < /home/wooseong/_bak/better-sonar-updater/signing-password.txt)" \
WSLENV="TAURI_SIGNING_PRIVATE_KEY:TAURI_SIGNING_PRIVATE_KEY_PASSWORD" \
./scripts/test-windows-from-wsl.sh --bundle
```

출력 폴더에서 NSIS, MSI와 각각의 `.sig`가 생성됐는지 확인한다.

```text
%LOCALAPPDATA%\BetterSonar\wsl-windows-test\source\target\release\bundle\nsis
%LOCALAPPDATA%\BetterSonar\wsl-windows-test\source\target\release\bundle\msi
```

실제 end-to-end 검증은 공개 직전의 테스트 Release 또는 별도 테스트 저장소에서 두 버전으로 수행한다.

| 시작 상태        | 설치/업데이트 경로                | 기대 결과                                          |
| ---------------- | --------------------------------- | -------------------------------------------------- |
| 미설치           | v0.0.3 NSIS 수동 설치             | 설정 화면에 현재 버전과 업데이트 확인 버튼 표시    |
| v0.0.2 NSIS      | v0.0.3 NSIS 수동 설치             | 기존 설정과 자동 시작 설정 유지                    |
| v0.0.2 MSI       | v0.0.3 NSIS 수동 설치             | 중복 설치·제거 항목 여부 확인                      |
| v0.0.3 NSIS      | v0.0.4 `latest.json`              | 새 버전 안내만 표시하고 자동 다운로드하지 않음     |
| v0.0.3 NSIS      | 사용자가 설치 승인                | 진행률 표시, 서명 검증, passive 설치, 자동 재시작  |
| v0.0.3 NSIS      | 손상된 설치 파일 또는 잘못된 서명 | 설치 거부, v0.0.3 실행 가능 상태 유지              |
| v0.0.3 NSIS      | 네트워크 단절                     | 오류 표시, 기존 앱과 설정 유지, 나중에 재시도 가능 |
| 트레이 자동 시작 | 새 버전 존재                      | 창을 강제로 띄우거나 작업 중 앱을 재시작하지 않음  |

검증 후 다음을 기록한다.

- 사용한 시작 버전과 대상 버전
- NSIS/MSI 시작 설치 형식
- Release URL과 workflow run URL
- 업데이트 확인, 다운로드, 서명 검증, 설치, 재시작 결과
- `%APPDATA%`의 Better Sonar 설정 유지 여부
- Windows 설치 앱 목록의 중복 여부

## GitHub 설정 점검

- `release` Environment에 `TAURI_SIGNING_PRIVATE_KEY`와 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`가 있어야 한다.
- 가능하면 `release` Environment에 required reviewer를 지정한다.
- 태그를 만들 수 있는 권한과 Release 게시 권한을 최소화한다.
- updater 키를 pull request나 일반 CI job에 전달하지 않는다.
- Release는 모든 asset이 준비된 초안 상태에서 검토한 뒤 게시한다.
