# volt

volt is a terminal-native API client inspired by `lazygit` and `lazydocker`.

## Features

- 3-pane TUI for route discovery, request editing, and response viewing
- Local route scanning for `axum`, `actix-web`, `express`, `fastify`, and `fastapi`
- Async request execution with latency and payload metrics
- JSON pretty-printing and syntax-aware response rendering
- Clipboard copy for response bodies and lightweight `.volt.toml` persistence

## Installation

### Linux / macOS
```bash
curl https://raw.githubusercontent.com/frypan05/Volt/main/scripts/install.sh | bash
```

The script installs to `$HOME/.local/bin` by default. Change it with the `DIR` variable:
```bash
curl https://raw.githubusercontent.com/frypan05/Volt/main/scripts/install.sh | DIR=/usr/local/bin bash
```

### Windows (PowerShell)
```powershell
irm https://raw.githubusercontent.com/frypan05/Volt/main/scripts/install.ps1 | iex
```

### Homebrew
```bash
brew tap frypan05/volt
brew install frypan05/volt/volt
```

### After installing
Navigate to any project and run:
```bash
cd your-project
volt
```

## Run in dev mode:

```bash
cargo run
```

volt scans the current working directory and lists detected routes. By default requests target `http://localhost:3000`; edit the base URL with `u`.

## Keybindings

- `Tab`: rotate focus across panes
- `Shift+Tab`: reverse focus
- `j/k` or arrows: move selection and switch tabs
- `1-4`: jump to `Headers`, `Body`, `Params`, `Auth`
- `i`: edit the active tab buffer
- `u`: edit base URL
- `r`: execute current request
- `c`: copy response body
- `Esc`: leave insert mode
- `q`: quit

## Input formats

- Headers: `Header-Name: value`
- Params: `key=value`
- Auth: raw `Authorization` header value, for example `Bearer <token>`
- Body: JSON is validated automatically when the content starts with `{` or `[`.

## Limitations

- Route discovery uses fast heuristics instead of full AST parsing.
- The insert mode is intentionally lightweight and optimized for request payload editing.
- Large responses are rendered inline; future versions should add paging and streaming.
