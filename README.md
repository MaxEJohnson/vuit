### (V)im (U)ser (I)nterface (T)erminal

A buffer manager layer for Vim that provides a terminal-like interface to search for, open, and edit files.

<img width="1512" alt="image" src="https://github.com/user-attachments/assets/5c355911-d516-4b1d-b709-e0b55b0b48ac" />

## Installation (Mac/WSL/Linux)

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/MaxEJohnson/vuit/main/install.sh)"
```

## How-To

ENTER - On highlighted file, open Vim.

\<C-j\> - Move down in "Files" window.

\<C-k\> - Move up in "Files" window.

TAB - Switch between the "Files" window and the "Recent" window.

\<C-r\> - Refresh CWD file scan.

ESC - Exit vuit.

All other keystrokes will populate the "Search" input window to filter the "Files" window output.
