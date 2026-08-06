# Security Policy

## Supported Versions

`ocpp-client` is pre-1.0 (`1.0.0-alpha.1`-track versioning under `0.x`). Security fixes are made
against the latest published release on [crates.io](https://crates.io/crates/ocpp-client); older
`0.x` versions are not backported.

## Reporting a Vulnerability

Please do not report security vulnerabilities through public GitHub issues.

Instead, report them via [GitHub's private vulnerability reporting](https://github.com/flowionab/ocpp-client/security/advisories/new),
or by emailing joatin@granlund.io. Include:

* A description of the vulnerability and its potential impact
* Steps to reproduce, or a minimal proof of concept
* The affected version(s)

You should expect an initial response within a few business days. We'll work with you to
understand and confirm the issue, develop and test a fix, and coordinate disclosure timing before
any public release.

## Scope

This crate implements the OCPP-J charge point (client) protocol layer over WebSocket. Security
issues of particular interest include:

* Memory safety or panics reachable from untrusted server input (malformed CALL/RESULT/ERROR
  frames, oversized payloads, malformed JSON)
* TLS/mutual TLS handshake or certificate validation issues in the WebSocket transport
  (`src/transport/websocket.rs`)
* Denial-of-service vectors reachable from a malicious or compromised CSMS (unbounded allocation,
  request/timeout bookkeeping exhaustion)
* Protocol state confusion (message ID reuse, mismatched CALLRESULT/CALLERROR routing)

Issues in the charging logic or hardware control layers are out of scope, since this crate is the
network/protocol layer only.
