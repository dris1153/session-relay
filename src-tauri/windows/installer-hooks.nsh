; Removes the auto-save hooks from Claude's settings.json before the exe is deleted, or Claude
; would report a failing hook after every turn. Skipped when an update reinstalls the app.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\session-relay.exe" hook-uninstall'
  ${EndIf}
!macroend
