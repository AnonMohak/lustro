; Lustro installer hooks. The uninstaller removes the registry entries that Settings can add.

; Lustro Setup (setup/) copies itself into the install folder as the themed uninstaller.
; The NSIS install writes its own uninstaller into the uninstall entry, so point it back after
; every install and in-app update.
!macro NSIS_HOOK_POSTINSTALL
  IfFileExists "$INSTDIR\lustro-setup.exe" 0 +2
    WriteRegStr SHCTX "${UNINSTKEY}" "UninstallString" "$\"$INSTDIR\lustro-setup.exe$\" --uninstall"
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Lustro"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\Lustro"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\Lustro"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\Lustro"
!macroend
