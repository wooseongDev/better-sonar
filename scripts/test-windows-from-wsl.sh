#!/usr/bin/env bash

set -Eeuo pipefail

usage() {
  cat <<'EOF'
WSL에서 Better Sonar의 Windows 네이티브 검사와 실행을 시작합니다.

사용법:
  scripts/test-windows-from-wsl.sh [옵션]

모드:
  --check          품질 검사, 테스트, 디버그 앱 빌드(기본값)
  --run            공통 검사 후 Windows Tauri 개발 앱 실행
  --bundle         공통 검사 후 NSIS/MSI 릴리스 번들 생성

옵션:
  --work-root PATH Windows 테스트 작업 폴더. Windows 또는 WSL 경로 사용 가능
  --skip-install   pnpm install 생략
  --clean          보호된 테스트 작업 폴더를 지운 뒤 다시 동기화
  -h, --help       도움말 표시
EOF
}

fail() {
  printf '[windows-test] 오류: %s\n' "$*" >&2
  exit 1
}

mode='Check'
work_root=''
skip_install=false
clean=false

while (($# > 0)); do
  case "$1" in
    --check)
      mode='Check'
      ;;
    --run)
      mode='Run'
      ;;
    --bundle)
      mode='Bundle'
      ;;
    --work-root)
      (($# >= 2)) || fail '--work-root 뒤에 경로를 지정해야 합니다.'
      work_root=$2
      shift
      ;;
    --skip-install)
      skip_install=true
      ;;
    --clean)
      clean=true
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      fail "알 수 없는 옵션입니다: $1"
      ;;
  esac
  shift
done

if [[ -z ${WSL_DISTRO_NAME:-} ]] && ! grep -qi microsoft /proc/sys/kernel/osrelease 2>/dev/null; then
  fail '이 스크립트는 Windows Interop이 활성화된 WSL에서 실행해야 합니다.'
fi
command -v wslpath >/dev/null 2>&1 || fail 'wslpath를 찾을 수 없습니다.'
command -v powershell.exe >/dev/null 2>&1 || fail 'powershell.exe를 찾을 수 없습니다. WSL Interop 설정을 확인하세요.'

script_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
repo_root=$(cd -- "$script_dir/.." && pwd -P)
power_shell_script="$script_dir/test-windows.ps1"
[[ -f $repo_root/package.json && -f $repo_root/src-tauri/tauri.conf.json ]] || fail 'Better Sonar 저장소 루트를 확인하지 못했습니다.'
[[ -f $power_shell_script ]] || fail "Windows 실행 스크립트가 없습니다: $power_shell_script"

source_windows=$(wslpath -w "$repo_root")
script_windows=$(wslpath -w "$power_shell_script")

if [[ -n $work_root && $work_root == /* ]]; then
  work_root=$(wslpath -w "$work_root")
fi

printf '[windows-test] WSL 배포판: %s\n' "${WSL_DISTRO_NAME:-unknown}"
printf '[windows-test] 원본 저장소: %s\n' "$repo_root"
printf '[windows-test] 실행 모드: %s\n' "$mode"

arguments=(
  -NoLogo
  -NoProfile
  -ExecutionPolicy Bypass
  -File "$script_windows"
  -SourceRoot "$source_windows"
  -Mode "$mode"
)
result_file=$(mktemp)
trap 'rm -f -- "$result_file"' EXIT
result_windows=$(wslpath -w "$result_file")
arguments+=(-ResultPath "$result_windows")
[[ -n $work_root ]] && arguments+=(-WorkRoot "$work_root")
[[ $skip_install == true ]] && arguments+=(-SkipInstall)
[[ $clean == true ]] && arguments+=(-Clean)

powershell.exe "${arguments[@]}"

driver_windows=$(tr -d '\r\n' <"$result_file")
[[ -n $driver_windows ]] || fail 'PowerShell이 Windows 테스트 드라이버 경로를 반환하지 않았습니다.'
printf '[windows-test] Windows 테스트 드라이버 실행: %s\n' "$driver_windows"

set +e
cmd.exe /d /c "$driver_windows"
status=$?
set -e
if ((status != 0)); then
  fail "Windows 네이티브 테스트가 종료 코드 $status 로 실패했습니다."
fi
