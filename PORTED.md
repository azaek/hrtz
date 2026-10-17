# Ported code

hrtz supports one mouse family at a time. Support for more mice is to be ported from OpenMouse's
[`mouse-protocol`](https://github.com/OpenMouse-Project/mouse-protocol) (AGPL-3.0-or-later): the
driver logic translated to Rust, never their package bundled.

Each ported file opens with a notice giving the upstream file and commit, "translated to Rust and
modified", the date, the copyright of the OpenMouse contributors, and AGPL-3.0-or-later. This list
keeps every port with its upstream commit, so later fixes there can be compared and brought over.

| hrtz file | Upstream file | Upstream commit | Ported |
|---|---|---|---|

Nothing is ported yet: Zaopin's protocol was worked out from the mouse itself and its own app.
