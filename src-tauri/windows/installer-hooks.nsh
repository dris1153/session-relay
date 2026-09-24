; Up to 0.2 the product was named "session-relay" and installed into its own folder. Remove that
; install first, in update mode: its uninstaller then keeps Claude's auto-save hooks, the app data
; and the sign-in; post-install below points the hooks at the new exe.
!macro NSIS_HOOK_PREINSTALL
  ReadRegStr $R8 HKCU "Software\sessionrelay\session-relay" ""
  ${IfThen} $R8 == "" ${|} StrCpy $R8 "$LOCALAPPDATA\session-relay" ${|}
  ${If} ${FileExists} "$R8\uninstall.exe"
    ; Asks before closing the old app; the silent uninstaller would kill it without asking.
    !insertmacro CheckIfAppIsRunning "${MAINBINARYNAME}.exe" "${PRODUCTNAME}"
    ExecWait '"$R8\uninstall.exe" /S /UPDATE _?=$R8'
    ${If} ${FileExists} "$R8\${MAINBINARYNAME}.exe"
      MessageBox MB_ICONSTOP|MB_OK "The previous version in $R8 could not be removed. Close it and run this installer again." /SD IDOK
      Abort
    ${EndIf}
    Delete "$R8\uninstall.exe"
    RMDir "$R8"
    DeleteRegKey HKCU "Software\sessionrelay\session-relay"
    ; Update mode keeps shortcuts and pins; these point at the removed exe.
    !insertmacro UnpinShortcut "$SMPROGRAMS\session-relay.lnk"
    !insertmacro UnpinShortcut "$DESKTOP\session-relay.lnk"
    Delete "$DESKTOP\session-relay.lnk"
    Delete "$SMPROGRAMS\session-relay.lnk"
  ${EndIf}
!macroend

; Auto-save hooks and the start-with-Windows entry may still name the old exe: fix them now,
; before Claude's next turn or the next logon, not only when the app is next opened.
!macro NSIS_HOOK_POSTINSTALL
  ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" post-install'
!macroend

; Removes the auto-save hooks from Claude's settings.json before the exe is deleted, or Claude
; would report a failing hook after every turn. Skipped when an update reinstalls the app.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    ExecWait '"$INSTDIR\session-relay.exe" hook-uninstall'
    ; The template only removes a Run value named after the product; ours keeps the 0.1–0.2 name.
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "session-relay"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "session-relay"
  ${EndIf}
!macroend
