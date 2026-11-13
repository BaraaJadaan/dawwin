; Dawwin's installer hooks (tauri.conf.json: bundle > windows > nsis > installerHooks).

!ifndef DAWWIN_RUN_KEY
  !define DAWWIN_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!endif

!macro NSIS_HOOK_POSTUNINSTALL
  ; "Start with Windows" (1.8.0) goes with the app. Not on an update: the new version keeps it, and
  ; Dawwin writes it again at its next start anyway when it is turned on.
  ${If} $UpdateMode <> 1
    DeleteRegValue HKCU "${DAWWIN_RUN_KEY}" "Dawwin"
  ${EndIf}
!macroend
