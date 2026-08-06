## Summary

<!-- What does this change do, and why? -->

## Related issue(s)

<!-- Closes #... , or link to related discussion -->

## Checklist

- [ ] Tests added/updated first (TDD is mandatory - see `CLAUDE.md`)
- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --features test` passes
- [ ] If `client.rs`/`transport.rs`/`error.rs`/a per-version `error.rs`/`actions.rs` changed:
      `cargo build --lib --no-default-features --features ocpp_1_6` (no_std+alloc proof) still
      builds
- [ ] Docs/comments updated where behavior changed
