; Custom NSIS installer hooks for SessionMeter.

!macro NSIS_HOOK_POSTINSTALL
  ; The Windows shell does not reliably paint this exe's embedded icon onto its shortcuts -
  ; it falls back to a generic icon (most visibly after the rename from the old product
  ; name), even though the icon is present in the exe. Point the shortcuts at a standalone
  ; icon.ico installed next to the app, which the shell renders reliably, and add a desktop
  ; shortcut too. Then flush the shell icon cache (SHCNE_ASSOCCHANGED = 0x08000000) so the
  ; icon shows immediately.
  CreateShortcut "$SMPROGRAMS\SessionMeter.lnk" "$INSTDIR\sessionmeter.exe" "" "$INSTDIR\icon.ico" 0
  CreateShortcut "$DESKTOP\SessionMeter.lnk" "$INSTDIR\sessionmeter.exe" "" "$INSTDIR\icon.ico" 0
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; Remove the desktop shortcut we added and refresh icons so nothing lingers.
  Delete "$DESKTOP\SessionMeter.lnk"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, i 0, i 0)'
!macroend
