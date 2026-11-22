; Uninstall hooks for the NSIS installer.
;
; The app stages ronron-hook.exe into %LOCALAPPDATA%\Ronron\bin at launch, so the
; installer never recorded it and the default uninstaller leaves it behind. The
; inbox and the log live in the same place and are ours too.
;
; Claude Code's own settings.json is deliberately NOT touched here: it belongs to
; the user, it may contain hooks from other tools, and rewriting somebody's
; config from an uninstaller with no diff and no consent is exactly what the rest
; of this app goes out of its way not to do. A relay that is gone exits 0 without
; printing anything, so a leftover entry costs nothing beyond a dead path.

; "Delete the application data", ticked on the uninstall page (never on an update):
; before the files go, ronron.exe itself clears every key Ronron put in the
; Credential Manager (Microsoft, Google, API keys, WhatsApp…) and removes
; %APPDATA%\Ronron (settings, reminders, local tasks, mail sorting), with no window.
; Once the program is gone, what is left of %LOCALAPPDATA%\Ronron (the per-user
; install folder: relay, voice models, log) goes too. Unticked, all of that stays
; for a reinstall.

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --forget-everything'
  ${EndIf}
  RMDir /r "$LOCALAPPDATA\Ronron\bin"
  RMDir /r "$LOCALAPPDATA\Ronron\inbox"
  Delete "$LOCALAPPDATA\Ronron\ronron.log"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    RMDir /r "$APPDATA\Ronron"
    RMDir /r "$LOCALAPPDATA\Ronron"
  ${EndIf}
!macroend
