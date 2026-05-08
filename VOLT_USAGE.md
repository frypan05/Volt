# VOLT_USAGE

## What Volt is

Volt is a terminal-native API client inspired by `lazygit` and `lazydocker`. You run it inside a project directory and it automatically discovers routes, lets you edit and fire requests, and renders responses in the terminal.

## Core capabilities

### Route discovery
Volt scans your current working directory and detects routes from common backend and frontend frameworks.

Supported backend frameworks:
- `axum`
- `actix-web`
- `express`
- `fastify`
- `fastapi`

Supported frontend frameworks:
- `next.js` (App Router and Pages Router)
- `react-router`
- `vue-router`
- `sveltekit`
- `angular`

### Request editing
You can edit request details per route:
- Headers
- Query parameters
- Auth
- Request body
- Base URL

Supported body types:
- JSON
- plain text
- form-urlencoded

Useful editing behavior:
- Base URL editor keeps history with `↑` and `↓`
- Per-route drafts are preserved across panes

### Response viewing
Volt shows responses with useful formatting:
- Syntax highlighting for JSON, HTML, and XML
- Automatic JSON pretty-printing
- Status code, latency, and response size
- Multiple view modes: Auto, JSON, HTML, Text, Raw
- Copy response body with `y`

### UI and workflow
- Resizable panes via click-and-drag
- Mouse support throughout
- Works in project directories with detected routes
- Add custom routes manually
- Broad directory protection pauses scanning in high-level directories like home or root

### Customization and persistence
- Themes: `Vesper`, `Dracula`, `Gruvbox`, `Tokyo-Night`
- Interactive theme selector with `volt --themes`
- Version check with `volt --version` or `volt -V`
- Auto-update with `volt update`
- Custom routes saved in `.volt_routes.json`
- App config stored in `.volt.toml`

## Installation

### Linux / macOS

```bash
curl https://raw.githubusercontent.com/frypan05/Volt/main/scripts/install.sh | bash
```

To change the install directory:

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

## Basic usage

After installing:

```bash
cd your-project
volt
```

By default, Volt targets `http://localhost:3000`. You can edit the base URL with `u`.

## Development

Run in dev mode:

```bash
cargo run
```

Useful build commands:

```bash
cargo build --release
cargo check
cargo test
cargo fmt
cargo clippy
```

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
- Body: JSON is validated automatically when content starts with `{` or `[`.

## Remote execution usage

Volt supports SSH-based remote execution for APIs that are only reachable from internal networks.

### What remote execution does
- Runs HTTP requests from a remote machine via SSH
- Helps you reach VPC-only APIs, private staging systems, and Kubernetes services
- Keeps the UI and request editing workflow the same

### Remote setup in `.volt.toml`

```toml
base_url = "http://localhost:3000"

[remote.production]
host = "prod-bastion.example.com"
user = "ubuntu"
port = 22
identity = "~/.ssh/volt_prod"

[remote.staging]
host = "staging-internal.company.com"
user = "deploy"
port = 22
identity = "~/.ssh/staging_key"
```

### Remote CLI usage

```bash
volt --remote-list
volt --remote production
volt --remote staging
```

If you select a remote profile, Volt uses the remote executor and shows it in the UI status bar.

### Expected workflow
1. Add remote profiles to `.volt.toml`
2. Run `volt --remote production`
3. Type an internal URL, such as `http://internal-api.svc.cluster.local/users`
4. Press Enter to execute the request from the remote host
5. Review the response in the TUI

## Remote execution details

### Executor abstraction
Volt uses an executor abstraction so the application can switch between local and remote execution without changing the UI or HTTP request flow.

Two executor types are supported:
- `LocalExecutor`: runs requests locally
- `RemoteExecutor`: runs requests via SSH

### Protocol
Remote execution uses JSON messages over SSH stdin/stdout.

Controller to agent example:

```json
{
  "Execute": {
    "request_id": "req-123",
    "method": "POST",
    "url": "http://internal-api/users",
    "headers": {"Authorization": "Bearer token"},
    "query_params": {},
    "body": "{\"name\": \"John\"}",
    "timeout_ms": 5000
  }
}
```

Agent to controller example:

```json
{
  "ExecutionResult": {
    "request_id": "req-123",
    "status": 201,
    "headers": {"content-type": "application/json"},
    "body": "{\"id\": 42, \"name\": \"John\"}",
    "duration_ms": 125,
    "size_bytes": 456,
    "timestamp": 1234567890
  }
}
```

### Security model
Safe behavior:
- Uses SSH encryption in transit
- Does not execute shell commands
- Uses a minimal agent for HTTP requests only

Be careful with:
- SSH key permissions
- Secrets in request bodies
- Host verification in production

## Executor and app behavior

Volt stores the active executor in application state so requests always go through the selected backend. The UI can display the active executor name, for example:
- `Local`
- `SSH:user@host`

## Input and behavior limitations

- Route discovery uses heuristics instead of full AST parsing
- Insert mode is intentionally lightweight for request editing
- Large responses are rendered inline
- Some advanced features like graph-based discovery and GraphQL support are planned or discussed as future work

## Troubleshooting remote mode

Helpful checks:

```bash
ssh ubuntu@prod-bastion "echo hello"
volt --remote-list
volt --version
```

If a profile is missing, check `.volt.toml` for the correct `[remote.*]` section.

## Combined feature summary

Volt currently combines:
- Automatic route discovery
- Request editing
- Response rendering
- Theme customization
- Config persistence
- SSH remote execution support
- Executor-based architecture for future transport backends

## Notes on documentation cleanup

This file is intended to be the single combined usage document. The other markdown files in the repo can be removed or kept only if you want them as historical notes, but they are no longer needed for user-facing usage documentation.
