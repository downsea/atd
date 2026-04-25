# atd-tools-shell

Built-in shell tools (`ref:shell.exec`, `ref:shell.pwsh`) for the ATD reference
runtime.

Both tools enforce a timeout, a hard byte cap on stdout/stderr, and the runtime's
capability gate. PowerShell is invoked via `pwsh -NoProfile -Command` when
available; `exec` uses `/bin/sh -c` (Unix) or `cmd /C` (Windows).

Pair with [`atd-runtime`](https://crates.io/crates/atd-runtime), or get them
preregistered via [`atd-ref-server`](https://crates.io/crates/atd-ref-server).

## License

Apache-2.0.
