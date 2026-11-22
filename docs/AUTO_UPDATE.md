# 자동 업데이트 운영

Better Sonar는 앱 시작 10초 후와 이후 6시간마다 GitHub Releases의 `latest.json`을 확인한다. 사용자가 **다운로드 및 설치**를 선택하면 해당 버전을 다시 확인하고 서명된 설치 파일을 내려받는다. 현재 설치 방식에 맞춰 NSIS 또는 MSI를 실행하고 설치 프로그램이 앱을 다시 연다.

## 릴리스 만들기

1. `better-sonar/Cargo.toml`의 앱 버전을 정하고 `Cargo.lock`을 갱신한다.
2. Windows에서 Rust 검사와 `./packaging/windows/build.ps1`을 실행한다. 빌드에 NSIS와 WiX 3이 필요하다. 도구가 PATH에 없으면 `BETTER_SONAR_NSIS_ROOT`와 `BETTER_SONAR_WIX_ROOT`에 실행 파일이 있는 폴더를 지정한다.
3. 깨끗한 Windows 가상 머신에서 `./packaging/windows/test-upgrade.ps1 -Isolated`를 실행한다. 스크립트는 공개 v0.0.6에서 새 설치 파일로 교체하고 설정 파일이 보존되는지 확인한다.
4. 앱 버전과 같은 `v` 태그를 푸시하면 Windows 워크플로가 검사를 실행하고, NSIS와 MSI에 서명한 뒤 릴리스 초안을 만든다.
5. 초안에서 설치 파일 두 개와 각각의 `.sig`, `latest.json`, `SHA256SUMS.txt`를 확인한다. `windows-x86_64`와 `windows-x86_64-nsis`는 NSIS 파일을, `windows-x86_64-msi`는 MSI 파일을 가리켜야 한다.

공개 키는 `packaging/windows/updater.pub`에 있다. 워크플로는 기존 릴리스 환경의 `TAURI_SIGNING_PRIVATE_KEY`와 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`를 Rust 서명 도구에 전달한다. 기존 설치본이 같은 공개 키로 새 설치 파일의 서명을 검증할 수 있다. GPUI 앱은 서명의 신뢰 가능한 코멘트에 기록된 버전까지 대조한다.

## 배포 전 실제 업데이트 검증

깨끗한 Windows 환경에서 서명된 테스트 피드를 사용한다. 테스트 빌드에서는 v0.0.6의 태그 소스에 있는 updater 주소와 공개 키를 테스트 값으로 바꾼다. GPUI 빌드에서도 `better-sonar/src/updates.rs`의 피드와 설치 파일 주소, `packaging/windows/updater.pub`의 공개 키를 같은 테스트 환경에 맞춘다. NSIS와 MSI 각각에서 다음을 확인한다.

- v0.0.6에서 업데이트가 나타난다. 사용자 승인 뒤 v0.0.7이 설치되고, 설정과 자동 시작이 유지된 채 창이 다시 열린다.
- v0.0.7에서 더 높은 테스트 버전을 설치할 수 있다. 설치 목록에 해당 방식의 새 버전만 남는다.
- 설치 파일을 변경하거나 서명의 버전을 다르게 하면 오류가 표시된다. 설치 프로그램이 실행되지 않는다.
