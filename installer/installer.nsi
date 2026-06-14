!define APP_NAME     "RoboLaunch"
!define APP_VERSION  "1.0.0"
!define APP_EXE      "robo-launch.exe"
!define INSTALL_DIR  "$PROGRAMFILES64\RoboLaunch"

Name "${APP_NAME} ${APP_VERSION}"
OutFile "RoboLaunch-Setup-1.0.0.exe"
InstallDir "${INSTALL_DIR}"
InstallDirRegKey HKLM "Software\RoboLaunch" "Install_Dir"
RequestExecutionLevel admin
SetCompressor /SOLID lzma

Page directory
Page instfiles

Section "Application" SecMain
  SetOutPath "$INSTDIR"
  File /r "..\src-tauri\target\release\*.exe"
  File /r "..\src-tauri\target\release\*.dll"
  File /r "..\src-tauri\resources\vm\"
  CreateDirectory "$INSTDIR\logs"
  WriteRegStr   HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RoboLaunch" "DisplayName"          "${APP_NAME}"
  WriteRegStr   HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RoboLaunch" "UninstallString"      "$INSTDIR\Uninstall.exe"
  WriteRegStr   HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RoboLaunch" "DisplayVersion"       "${APP_VERSION}"
  WriteRegStr   HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RoboLaunch" "Publisher"            "RoboLaunch"
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  CreateShortcut "$DESKTOP\RoboLaunch.lnk" "$INSTDIR\${APP_EXE}" "" "$INSTDIR\icons\icon.ico"
  CreateDirectory "$SMPROGRAMS\RoboLaunch"
  CreateShortcut  "$SMPROGRAMS\RoboLaunch\RoboLaunch.lnk" "$INSTDIR\${APP_EXE}"
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir /r "$INSTDIR"
  Delete "$DESKTOP\RoboLaunch.lnk"
  RMDir /r "$SMPROGRAMS\RoboLaunch"
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\RoboLaunch"
  DeleteRegKey HKLM "Software\RoboLaunch"
SectionEnd
