<!-- Thanks for helping with hrtz! Fill in what applies, and delete what doesn't. -->

## What this changes

<!-- What it does for someone using hrtz, in a few lines. Link the issue it closes: "Closes #12". -->

## The mouse

<!-- Only for a change to how hrtz talks to a mouse: a new model, a setting, or a fix. -->

- Mouse and connection: <!-- G304 on its LIGHTSPEED receiver (046D:C53F) -->
- Firmware: <!-- Settings, under Mouse -->
- What it writes to the mouse, and what happens if a write fails:
- Tried on the real mouse:
  - [ ] Read only: nothing was written
  - [ ] Each setting it offers was changed, kept, and put back
  - [ ] Its check passed: **Inspect a mouse** in the title bar. Link the report's issue, or attach the report file.

## Checks

- [ ] `cargo test --lib`, `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` pass in `src-tauri`, and `npx tsc --noEmit -p .` in the root. The PR runs them too.
- [ ] A change people will notice has a line under `## Unreleased` in `CHANGELOG.md`.
- [ ] A change to the app's look has screenshots, light and dark.
- [ ] A new mouse picture says where it's from in `src/assets/models/NOTICE.md`.
- [ ] Code taken from another project keeps its notice and has a row in `PORTED.md`.
- [ ] Nothing from a vendor's app (its code, decompiled or not), no firmware or memory dumps, and no notes about your own machine or build setup. What you learned from them, in your own words, is welcome.
