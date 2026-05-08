After installation, the remote execution README section should focus only on:

* configuring SSH
* adding remote profiles
* using `volt --remote`

The important UX idea is:

```txt id="k1j3m9"
Volt already exists on both machines.
Remote mode only changes WHERE requests execute.
```

So your README section should look more like this:

---

# Remote Execution

Volt can execute requests remotely over SSH.

This allows APIs to be tested from:

* VPC machines
* staging servers
* Kubernetes-accessible hosts
* bastions
* internal infrastructure

Instead of:

```txt id="r2w1h6"
your laptop → API
```

requests become:

```txt id="u8x4s2"
your laptop
    ↓ SSH
remote machine
    ↓ HTTP
target API
```

---

# Basic Usage

After installing Volt normally:

```bash id="d7m4f1"
cd your-project
volt
```

configure a remote profile inside:

```txt id="v5n8q0"
.volt.toml
```

Example:

```toml id="z0t3p4"
base_url = "http://internal-api:8080"

[remote.staging]
host = "staging.example.com"
user = "ubuntu"
identity = "~/.ssh/staging_key"
```

Run Volt remotely:

```bash id="q4y9m1"
volt --remote staging
```

All requests now execute from the remote machine.

---

# SSH Setup

Generate a key:

```bash id="a7c2d8"
ssh-keygen -t ed25519 -f ~/.ssh/volt_remote -N ""
```

Copy key to remote host:

```bash id="x3m7b0"
ssh-copy-id -i ~/.ssh/volt_remote.pub ubuntu@staging.example.com
```

Verify SSH access:

```bash id="c6v1p5"
ssh -i ~/.ssh/volt_remote ubuntu@staging.example.com
```

---

# Verify Remote Agent

Test the Volt remote agent manually:

```bash id="w2k8n9"
ssh -i ~/.ssh/volt_remote ubuntu@staging.example.com volt --agent
```

Send:

```json id="m9p3x7"
{"Health":null}
```

Expected response:

```json id="f4r8t1"
"HealthOk"
```

---

# Example Use Cases

## Internal APIs

```toml id="j8u5l2"
base_url = "http://payments.internal:8080"
```

Only reachable from inside company infrastructure.

---

## Kubernetes Services

```toml id="s7e4d6"
base_url = "http://auth-service.default.svc.cluster.local"
```

Execute requests from a cluster-accessible machine.

---

## Staging Environments

```bash id="h2w9v4"
volt --remote staging
```

Run integration tests without exposing services publicly.

---

# Example Remote Profiles

```toml id="b6m1n8"
[remote.prod]
host = "prod.example.com"
user = "ubuntu"
identity = "~/.ssh/prod_key"

[remote.staging]
host = "staging.example.com"
user = "ubuntu"
identity = "~/.ssh/staging_key"

[remote.local]
host = "localhost"
user = "testuser"
port = 2222
identity = "/tmp/volt_test_key"
```

---

# Troubleshooting

## Verify SSH

```bash id="g1x8c3"
ssh -i ~/.ssh/volt_remote user@host
```

---

## Verify Volt Exists Remotely

```bash id="t9r2m7"
ssh user@host which volt
```

---

## Verify Agent

```bash id="n4k7q1"
ssh user@host volt --agent
```

Send:

```json id="e5v3z8"
{"Health":null}
```

Expected:

```json id="u1m6w2"
"HealthOk"
```

---

# Notes

Current implementation:

* SSH-based execution
* remote request execution
* JSON agent protocol

Planned:

* persistent SSH sessions
* streaming responses
* Kubernetes executor
* distributed workflows
* DAG execution
* parallel execution graphs
