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

## By hand, on the maintainer's PC

What the trial cannot do without taking the keyboard, from a written list. Use the app built from
this branch (Settings, "Read anything on the screen", turn the shortcut on), or
`trial-selection watch`, which says which way worked without printing the text. Use made-up text.

1. **Each program**: select a sentence and press the shortcut. Chrome, Edge, a PDF in Chrome or
   Edge, Firefox, Word, Outlook (a new mail, not sent), VS Code, Antigravity, Windows Terminal,
   WhatsApp (a draft, not sent), Notepad. Note which way worked, and press again to stop.
2. **Nothing selected**: in VS Code's editor and in Windows Terminal with a program running (for
   example `ping -t 127.0.0.1`), press the shortcut with nothing selected. Expected: "Nothing is
   selected", no line copied, and the program still running. The same in VS Code's own terminal.
3. **Administrator**: open Notepad with "Run as administrator", select, press. Expected: "Dawwin
   cannot read in a window that runs as administrator."
4. **A taken shortcut**: Ctrl+Alt+R is held by another program on this PC: turning the shortcut on
   says so and offers free ones. Choosing one must work at once.
5. **Arabic keyboard**: switch to Arabic (101) and repeat 1 in Notepad and Chrome.
6. **The clipboard**: copy a picture (a screenshot), then read a selection in Firefox (it needs the
   Ctrl+C way). Paste afterwards: the picture must still be there.
7. **The small player** (once the voice of 1.7.0 is in): while it reads, keep typing in another
   program and click the player's buttons. Expected: the buttons work and the typing never stops
   going to the program you are in.
8. **Closing and starting again**: with the shortcut on, close Dawwin's window; its icon stays in
   the notification area and the shortcut works. Start Dawwin again: its window opens. Turn on
   Start with Windows, sign out and in: Dawwin waits in the notification area, without its window.
