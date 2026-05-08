# volt

![Release](https://img.shields.io/github/v/release/frypan05/Volt)
![Platform](https://img.shields.io/badge/platform-linux%20%7C%20macos%20%7C%20windows-blue)

volt is a terminal-native API client inspired by `lazygit` and `lazydocker`. Run it inside any project directory and it automatically discovers your routes, lets you edit and fire requests, and renders the response — all without leaving the terminal.

<https://github.com/user-attachments/assets/444c725f-4e14-4de6-ae8a-56db1ec275c5>



---
<img width="1919" height="1079" alt="zed-editor" src="https://github.com/user-attachments/assets/75ef8697-94c9-4389-aff1-06eb83eeb645" />

<img width="1233" height="466" alt="zed" src="https://github.com/user-attachments/assets/9afd990d-d78e-4e94-9a20-4215414df264" />

<div align="center">
  <a href="https://star-history.com/#frypan05/Volt&Date">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=frypan05/Volt&type=Date&theme=dark">
      <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=frypan05/Volt&type=Date">
      <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=frypan05/Volt&type=Date" width="700">
    </picture>
  </a>
</div>

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

**Customization**

- **Themes**: Personalize Volt's look with built-in themes: `Vesper` (default), `Dracula`, `Gruvbox`, and `Tokyo-Night`.
- **Interactive Selector**: Run `volt --themes` to open an interactive menu and switch themes on the fly.
- **Dynamic UI**: UI accents and the `VOLT` header automatically adapt to your chosen theme's primary color.
- **Version Check**: Run `volt --version` or `volt -V` to check your current version.
- **Auto-Update**: Run `volt update` to automatically check for and install the latest version from GitHub.

**Broad Directory Protection**

- Volt automatically detects when it's opened in a high-level directory (like a User Home or Root) and pauses recursive scanning to prevent lag, guiding you to open a specific project instead.

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

## Run in dev mode

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

## In Queue Features:
- Graph QL support
- Graph based route discovery
- SSH Remote Execution Mode (From inside VMs, Containers, staging nodes, pods, etc.)
-
