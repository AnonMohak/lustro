; Lustro installer hooks. The uninstaller removes the registry entries that Settings can add.

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Lustro"
  DeleteRegKey HKCU "Software\Classes\Directory\shell\Lustro"
  DeleteRegKey HKCU "Software\Classes\Directory\Background\shell\Lustro"
  DeleteRegKey HKCU "Software\Classes\Drive\shell\Lustro"
!macroend
