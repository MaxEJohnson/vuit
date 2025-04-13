### (V)im (U)ser (I)nterface (T)erminal

A buffer manager layer for Vim that provides a terminal-like interface to search for, open, and edit files.

<img width="1512" alt="image" src="https://github.com/user-attachments/assets/91ddc8e1-8f2f-4ed1-8350-d5bda123515a" />

## Installation (Mac/WSL/Linux)

```bash
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/MaxEJohnson/vuit/main/install.sh)"
```

## How-To
<pre>
ENTER      - On highlighted file, open Vim.

&lt;C-j&gt;      - Move down in "Files" window.

&lt;C-k&gt;      - Move up in "Files" window.

TAB        - Switch between the "Files" window and the "Recent" window.

&lt;C-r&gt;      - Refresh CWD file scan.

&lt;C-n&gt;      - Cycle through colorschemes.

&lt;C-t&gt;      - Toggle Terminal.

&lt;C-f&gt;      - Toggle String Search.

&lt;C-h&gt;      - Toggle Help Menu.

ESC        - Exit vuit.
</pre>
All other keystrokes will populate the "Search/Command Line" input window to either filter the "Files" window output or prep commands for the "Terminal" window.

## Configuration: `.vuitrc`

To add your own configurations that are static.

    1. Create ~/.vuit/.vuitrc
    2. Populate the three JSON attributes: { colorscheme, highlight_color, editor }

### Attribute: `colorscheme`

Select from the following colors to be the base text and window color:

    red, green, blue, cyan, yellow, lightred, lightgreen, lightblue, lightcyan, lightyellow.

### Attribute: `highlight_color`

Select from the following colors to be the selector color:

    red, green, blue, cyan, yellow, lightred, lightgreen, lightblue, lightcyan, lightyellow.

### Attribute: `editor`

Selection is up to the user. Examples: vim, nvim, ... nano 

### Example `.vuitrc`

```json
{
    "colorscheme": "Cyan",
    "highlight_color": "Blue",
    "editor": "vim"
}
```
