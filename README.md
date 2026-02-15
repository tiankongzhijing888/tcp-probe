# tcp-probe

Fast TCP health probe for checking service availability. Written in Rust for minimal latency overhead.

## Features

- **Concurrent probes**: Check multiple hosts in parallel
- **Latency measurement**: Precise TCP handshake timing
- **Retry logic**: Configurable retry count with backoff
- **CI/CD friendly**: Non-zero exit codes on failure
- **JSON output**: Machine-readable results
- **IPv4/IPv6**: Dual-stack support

## Usage

```bash
# Single host
tcp-probe example.com:443

# Multiple hosts
tcp-probe example.com:443 db.internal:5432 redis:6379

# With options
tcp-probe --timeout 3s --retries 2 --json example.com:443

# From file
tcp-probe --file targets.txt
```

## Output

```
$ tcp-probe example.com:443 db.internal:5432
[OK]   example.com:443      12.3ms
[FAIL] db.internal:5432     timeout (5000ms)

Summary: 1/2 healthy
```

JSON output with `--json`:
```json
{
  "results": [
    {"host": "example.com:443", "status": "ok", "latency_ms": 12.3},
    {"host": "db.internal:5432", "status": "fail", "error": "timeout"}
  ],
  "healthy": 1,
  "total": 2
}
```

## Install

```bash
cargo install tcp-probe
```

## Build

```bash
cargo build --release
```

## License

MIT