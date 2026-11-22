Unicode true
RequestExecutionLevel user
SetCompressor /SOLID lzma

!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "MUI2.nsh"

!define PRODUCT "Better Sonar"
!define UNINSTALL "Software\Microsoft\Windows\CurrentVersion\Uninstall\Better Sonar"
!define REPLACE_EXISTING_WRITE_THROUGH 9

Name "${PRODUCT}"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\${PRODUCT}"
VIProductVersion "${VERSION}.0"
VIAddVersionKey "ProductName" "${PRODUCT}"
VIAddVersionKey "ProductVersion" "${VERSION}"
VIAddVersionKey "FileDescription" "${PRODUCT} installer"
VIAddVersionKey "FileVersion" "${VERSION}"
VIAddVersionKey "LegalCopyright" "Better Sonar contributors"
!define MUI_ICON "${ICON}"
!define MUI_UNICON "${ICON}"
!define MUI_ABORTWARNING

!define MUI_PAGE_CUSTOMFUNCTION_PRE SkipDirectoryInPassiveMode
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "Korean"

Var UpdateMode
Var PassiveMode

Function .onInit
  ${GetParameters} $0
  ${GetOptions} $0 "/P" $1
  ${IfNot} ${Errors}
    StrCpy $PassiveMode 1
    SetAutoClose true
  ${EndIf}
  ClearErrors
  ${GetOptions} $0 "/UPDATE" $1
  IfErrors +2
  StrCpy $UpdateMode 1

  ReadRegStr $0 HKCU "${UNINSTALL}" "InstallLocation"
  ${If} $0 != ""
    StrCpy $1 $0 1
    ${If} $1 == '"'
      StrCpy $0 $0 -1 1
    ${EndIf}
    StrCpy $INSTDIR $0
  ${EndIf}
FunctionEnd

Function SkipDirectoryInPassiveMode
  ${If} $PassiveMode == 1
    Abort
  ${EndIf}
FunctionEnd

Section "Better Sonar"
  SetShellVarContext current
  SetOutPath "$INSTDIR"
  File /oname=better-sonar.exe.new "${BINARY}"
  StrCpy $1 30
replace_app:
  System::Call 'kernel32::MoveFileExW(w "$INSTDIR\better-sonar.exe.new", w "$INSTDIR\better-sonar.exe", i ${REPLACE_EXISTING_WRITE_THROUGH}) i .r0'
  ${If} $0 == 0
    IntOp $1 $1 - 1
    ${If} $1 > 0
      Sleep 200
      Goto replace_app
    ${EndIf}
    Delete "$INSTDIR\better-sonar.exe.new"
    MessageBox MB_OK|MB_ICONSTOP "Exit Better Sonar from the system tray and run setup again." /SD IDOK
    SetErrorLevel 1
    Abort
  ${EndIf}
  FileOpen $0 "$INSTDIR\installer-kind" w
  FileWrite $0 "nsis"
  FileClose $0
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${UNINSTALL}" "DisplayName" "${PRODUCT}"
  WriteRegStr HKCU "${UNINSTALL}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTALL}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTALL}" "DisplayIcon" "$INSTDIR\better-sonar.exe"
  WriteRegStr HKCU "${UNINSTALL}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKCU "${UNINSTALL}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTALL}" "NoRepair" 1
  CreateShortCut "$SMPROGRAMS\${PRODUCT}.lnk" "$INSTDIR\better-sonar.exe"
SectionEnd

Function .onInstSuccess
  ${If} $UpdateMode == 1
    Exec '"$INSTDIR\better-sonar.exe"'
  ${EndIf}
FunctionEnd

Section "Uninstall"
  SetShellVarContext current
  Delete "$INSTDIR\better-sonar.exe"
  Delete "$INSTDIR\installer-kind"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$SMPROGRAMS\${PRODUCT}.lnk"
  Delete "$DESKTOP\${PRODUCT}.lnk"
  DeleteRegKey HKCU "${UNINSTALL}"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${PRODUCT}"
  RMDir "$INSTDIR"
SectionEnd
