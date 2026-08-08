; NSIS installer for shotr. Built by packaging/build-windows.sh when makensis
; is available; the zip is produced either way so a release never depends on it.
!define APP "shotr"
Name "${APP}"
OutFile "..\..\dist\shotr-${VERSION}-setup.exe"
Unicode true
InstallDir "$LOCALAPPDATA\${APP}"
; Per-user install: no UAC prompt, and screen capture needs no elevation.
RequestExecutionLevel user
ShowInstDetails show

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
    SetOutPath "$INSTDIR"
    File "${SOURCE}\shotr.exe"
    CreateShortcut "$SMPROGRAMS\${APP}.lnk" "$INSTDIR\shotr.exe"
    CreateShortcut "$DESKTOP\${APP}.lnk" "$INSTDIR\shotr.exe"
    WriteUninstaller "$INSTDIR\uninstall.exe"

    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}" \
        "DisplayName" "${APP}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}" \
        "UninstallString" "$INSTDIR\uninstall.exe"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}" \
        "DisplayVersion" "${VERSION}"
SectionEnd

Section "Uninstall"
    Delete "$INSTDIR\shotr.exe"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"
    Delete "$SMPROGRAMS\${APP}.lnk"
    Delete "$DESKTOP\${APP}.lnk"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APP}"
SectionEnd
