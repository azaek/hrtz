# Ported code

hrtz supports one mouse family at a time. Support for more mice is to be ported from OpenMouse's
[`mouse-protocol`](https://github.com/OpenMouse-Project/mouse-protocol) (AGPL-3.0-or-later): the
driver logic translated to Rust, never their package bundled.

Each ported file opens with a notice giving the upstream file and commit, "translated to Rust and
modified", the date, the copyright of the OpenMouse contributors, and AGPL-3.0-or-later. This list
keeps every port with its upstream commit, so later fixes there can be compared and brought over.

| hrtz file | Upstream file | Upstream commit | Ported |
|---|---|---|---|
| `src-tauri/src/mice/hidpp/protocol.rs` | `src/logitech/index.ts`, with constants from `src/drivers/logitech/hidpp.ts` and `src/drivers/vendors.ts` | `c3b1bc20866fbef3a27b37ad020887607d6dbbf2` | 2026-10-18 |
| `src-tauri/src/mice/hidpp/onboard.rs` | `src/drivers/logitech/onboard-profiles.ts` | `c3b1bc20866fbef3a27b37ad020887607d6dbbf2` | 2026-10-18 |
| `src-tauri/src/mice/hidpp/driver.rs` | `src/drivers/logitech/hidpp.ts` | `c3b1bc20866fbef3a27b37ad020887607d6dbbf2` | 2026-10-18 |
| `src-tauri/src/mice/hidpp/buttons.rs` | `src/drivers/logitech/onboard-profiles.ts` (the button records) | `c3b1bc20866fbef3a27b37ad020887607d6dbbf2` | 2026-10-18 |
| `src-tauri/src/mice/hidpp/models.rs` | `src/logitech/index.ts`, `src/drivers/vendors.ts` (product IDs) | `c3b1bc20866fbef3a27b37ad020887607d6dbbf2` | 2026-10-18 |
| `src-tauri/src/mice/hidpp/fixtures.rs` | `src/drivers/logitech/onboard-profiles.test.ts` (the captures) | `c3b1bc20866fbef3a27b37ad020887607d6dbbf2` | 2026-10-18 |
| `src-tauri/src/mice/hidpp/tests.rs` (the protocol and profile tests) | `src/drivers/logitech/hidpp.test.ts`, `onboard-profiles.test.ts` | `c3b1bc20866fbef3a27b37ad020887607d6dbbf2` | 2026-10-18 |

Zaopin's protocol (`mice/compx/`) wasn't ported: it was worked out from the mouse itself and its own
app. The Logitech driver keeps OpenMouse's rules where hrtz has the same need, and differs where
hrtz does: it never switches the mouse to host mode, so every setting goes into the onboard profile,
and it opens the short and long HID++ collections apart, as Windows lists them.
