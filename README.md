# volt
 
volt is a terminal-native API client inspired by `lazygit` and `lazydocker`. Run it inside any project directory and it automatically discovers your routes, lets you edit and fire requests, and renders the response — all without leaving the terminal.
 
![License](https://img.shields.io/github/license/frypan05/Volt)
![Release](https://img.shields.io/github/v/release/frypan05/Volt)
![Platform](https://img.shields.io/badge/platform-linux%20%7C%20macos%20%7C%20windows-blue)
 
---
 
## Features
 
**Route discovery**
Volt scans your current working directory and automatically detects routes from:
 
| Backend | Frontend |
|---|---|
| `axum` | `next.js` (App Router + Pages Router) |
| `actix-web` | `react-router` |
| `express` / `fastify` | `vue-router` |
| `fastapi` | `sveltekit` |
| | `angular` |
 
**Request editing**
- Edit headers, query params, auth, and request body per route
- Supports `JSON`, `plain text`, and `form-urlencoded` body types
- Base URL editor with history — navigate previous URLs with `↑` / `↓`
- Per-route drafts are preserved across panes
 
**Response viewing**
- Syntax-aware highlighting for JSON, HTML, and XML responses
- JSON is automatically pretty-printed
- Status code, latency, and response size shown on every request
- Five view modes: Auto, JSON, HTML, Text, Raw — switch with `/`
- Clipboard copy with `y`
 
**Persistence**
- Custom routes saved to `.volt_routes.json` in your project root — routes and their base URLs survive restarts
- App config stored in `.volt.toml`
 
**Workflow**
- Works in directories with routes defined — opens the TUI with routes and you can add custom ones immediately (any directory functionality coming soon)
- Resizable panes via click-and-drag
- Mouse support throughout
 
---
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
