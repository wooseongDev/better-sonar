# WSL에서 Windows 네이티브 테스트

Better Sonar는 WSL의 개발 편의성과 Windows 네이티브 MSVC·Tauri 검증을 함께 사용할 수 있도록 두 단계 테스트 스크립트를 제공합니다.

- `scripts/test-windows-from-wsl.sh`: WSL 배포판과 저장소 경로를 확인하고 Windows PowerShell 준비 단계를 호출한다.
- `scripts/test-windows.ps1`: 필수 Windows 도구를 검사하고 소스를 Windows 로컬 작업 폴더로 안전하게 동기화한 뒤 네이티브 테스트 드라이버를 만든다.
- 생성된 Windows 배치 드라이버: MSVC 환경을 적용하고 pnpm, Cargo, Tauri 명령을 Windows 프로세스로 실행한다.

PowerShell 5.1과 PowerShell 7에서 읽을 수 있도록 `.ps1` 파일은 UTF-8 BOM 형식으로 저장한다.

## 사전 요구사항

WSL 쪽에는 다음 기능이 필요하다.

- Windows Interop이 활성화된 WSL 1 또는 WSL 2
- `bash`, `wslpath`, `powershell.exe`, `cmd.exe`

Windows 쪽에는 다음 도구가 필요하다.

- Visual Studio 2022 Build Tools
  - Desktop development with C++ 워크로드
  - Windows SDK
- Rust stable MSVC 호스트 도구체인
- Git for Windows
- 프로젝트 `.nvmrc` 이상의 Node.js
- pnpm
- Tauri 실행에 필요한 WebView2

스크립트는 `vswhere.exe`로 Visual Studio 설치를 찾고 `VsDevCmd.bat`으로 x64 MSVC 환경을 구성한다. 도구가 없거나 Rust 호스트가 `*-pc-windows-msvc`가 아니면 소스 빌드 전에 이해 가능한 오류를 출력한다.

## 사용법

기본 검사는 다음 명령으로 실행한다.

```bash
./scripts/test-windows-from-wsl.sh
```

이는 다음 작업을 순서대로 수행한다.

1. WSL과 Windows Interop 확인
2. Windows 네이티브 도구 및 Node.js 버전 확인
3. WSL 저장소를 Windows 로컬 작업 폴더로 동기화
4. `pnpm install --frozen-lockfile`
5. `pnpm check:quality`
6. `pnpm build`
7. `cargo test --workspace --locked`
8. `pnpm tauri build --debug --no-bundle -- --locked`

### 앱 실행

```bash
./scripts/test-windows-from-wsl.sh --run
```

공통 검사 후 `pnpm tauri dev --no-watch -- --locked`를 실행한다. Windows 앱을 종료할 때까지 터미널 명령도 실행 상태를 유지한다. 원본 WSL 파일의 후속 변경은 실행 중인 격리 작업 폴더에 자동 반영되지 않으므로, 변경 후 명령을 다시 실행한다.

### 설치 패키지 생성

```bash
./scripts/test-windows-from-wsl.sh --bundle
```

공통 검사 후 NSIS와 MSI 릴리스 번들을 생성한다. 결과는 기본 설정에서 다음 위치에 저장된다.

```text
%LOCALAPPDATA%\BetterSonar\wsl-windows-test\source\target\release\bundle
```

### 기타 옵션

```bash
./scripts/test-windows-from-wsl.sh --skip-install
./scripts/test-windows-from-wsl.sh --clean
./scripts/test-windows-from-wsl.sh --work-root 'C:\Temp\better-sonar-test'
./scripts/test-windows-from-wsl.sh --work-root /mnt/c/Temp/better-sonar-test
```

- `--skip-install`: 기존 Windows `node_modules`를 재사용하고 설치 단계를 생략한다.
- `--clean`: 보호 마커가 있는 테스트 작업 폴더만 삭제하고 처음부터 동기화한다.
- `--work-root`: 기본 작업 폴더 대신 Windows 또는 WSL 형식의 경로를 사용한다.

## 동기화 및 안전성

기본 작업 폴더는 다음과 같다.

```text
%LOCALAPPDATA%\BetterSonar\wsl-windows-test
```

Windows 빌드 도구가 UNC 경로에서 보이는 호환성·성능 문제를 피하기 위해 `robocopy /MIR`로 `source` 하위 폴더를 동기화한다. 다음 항목은 복사하거나 미러 삭제하지 않으므로 Windows 캐시를 재사용한다.

- `.git`
- `node_modules`
- `target`
- `dist`
- `artifacts`
- `*.log`

격리된 `source` 폴더에는 Lefthook 설치가 원본 저장소에 영향을 주지 않도록 별도의 Git 메타데이터를 초기화한다.

작업 루트에는 `.better-sonar-wsl-windows-test` 보호 마커를 둔다. `--clean`은 이 마커가 없는 폴더를 삭제하지 않는다. 사용자가 지정한 작업 폴더에 기존 파일이 있고 보호 마커가 없다면 동기화를 거부한다.

## 로그와 실패 처리

PowerShell 준비 로그는 다음 위치에 남는다.

```text
%LOCALAPPDATA%\BetterSonar\wsl-windows-test\logs
```

Windows 품질 검사, 테스트, 빌드 출력은 호출한 WSL 터미널에 실시간으로 표시된다. 어느 단계에서든 종료 코드가 0이 아니면 이후 단계를 실행하지 않고 WSL 진입 스크립트도 같은 실패를 보고한다.

실제 Fn 미디어 키, 트레이 상태와 Sonar Master - Personal 동작은 `--run`으로 앱을 실행한 뒤 [Windows 미디어 키 검증](WINDOWS_MEDIA_KEYS.md)의 수동 절차로 확인해야 한다. 자동 Windows 빌드 성공만으로 하드웨어 검증이 완료된 것으로 간주하지 않는다.
