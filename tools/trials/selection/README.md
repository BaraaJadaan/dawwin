# Trial 1 for 1.8.0: the selected text, by a shortcut

The app's own capture code (`src-tauri/src/selection`) behind a real system-wide shortcut: UI
Automation first, Ctrl+C with the clipboard put back where a program gives no selection that way.

```bash
cargo build --release
target/release/trial-selection hotkeys        # which shortcuts other programs already hold
target/release/trial-selection run testwin    # the small test window, driven by itself
target/release/trial-selection watch          # by hand: select text anywhere, press Ctrl+Alt+Q
```

`run` with other targets (chrome, edge, chrome-pdf, edge-pdf, firefox, word, vscode, antigravity,
wt, conhost) opens each program in a throwaway profile with made-up text and drives it with keys:
it takes the screen and the keyboard for a few minutes, so it runs only when the maintainer says so.
`watch` prints the program, the way that worked, the number of characters and the time, never the text.

## Results so far (2026-11-12, the maintainer's PC)

**The test window**: a standard edit box and a box with no text pattern that copies on Ctrl+C.

| Case | Way | Result | Time | Clipboard after |
| --- | --- | --- | --- | --- |
| Edit box, first line selected | as the app does it | text, by UI Automation | 74 ms | untouched |
| | Ctrl+C forced | text, by Ctrl+C | 242 ms | put back, every format |
| | UI Automation skipped, Ctrl+C turned off | "copying is turned off" | 0 ms | untouched |
| Edit box, nothing selected | as the app does it | nothing selected (UI Automation) | 4 ms | untouched |
| | Ctrl+C forced | nothing selected (no copy came) | 699 ms | untouched |
| Box without a text pattern, selected | as the app does it | text, by Ctrl+C | 242 ms | put back, every format |
| | Ctrl+C turned off | "copying is turned off" | 0 ms | untouched |
| Box without a text pattern, nothing selected | as the app does it | nothing selected | 687 ms | untouched |

"Every format" is made-up text, a made-up picture and made-up HTML, compared byte for byte. The
user's own clipboard was kept at the start and put back the same at the end.

**Shortcuts other programs hold on this PC**: Ctrl+Alt+R (the spec's default), Ctrl+Alt+L,
Ctrl+Alt+Space, Alt+Shift+R, Win+Alt+R, Win+Shift+R. Free: Ctrl+Alt+S, Ctrl+Alt+Q, Ctrl+Alt+E,
Ctrl+Alt+F9, Ctrl+Shift+Space, Ctrl+Alt+Shift+R, Alt+Shift+S (and Ctrl+Shift+R, which browsers use
to reload). A second registration of held keys is refused with "Hot key is already registered".

**Not tried yet**: Chrome, Edge, their PDF viewers, Firefox, Word, VS Code, Antigravity (a VS Code
fork, as the other Electron program), Windows Terminal, the console. Not installed here: Acrobat,
SumatraPDF, Telegram. Not tried by the trial because they hold the maintainer's own data: new
Outlook, WhatsApp, Notepad (it reopens its saved tabs). Firefox here is 120, older than the version
that turned UI Automation on, so it is expected to need the Ctrl+C way.
