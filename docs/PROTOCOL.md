# Zaopin / JinLeMa wireless mouse protocol

How hrtz talks to Zaopin's mice and receivers, worked out for interoperability. Verified against a Z1 PRO MAX (CID 7, MID 4) on the 8K receiver; the other models are recognised by the same handshake but not yet tested.

Commands marked **do not use** can change the device's identity or put it into firmware update or test modes. hrtz never sends them.

## Devices

| | VID | PID | Notes |
|---|---|---|---|
| Mouse (wired) | 0x3554 | 0xF526, 0xF50F | |
| Dongle | 0x3554 | 0xF524, 0xF50D, 0xF527 | F527 = 8K dongle (CH32V305) |

Models are identified by `CID`/`MID` from the handshake, not by PID. CID 7 = Zaopin. MID: 1 ZI PRO, 2 Z2, 3 Z2 mini, 4 Z1 PRO MAX, 5 Z2 MAX, 6 Z2 mini MAX. Device type 5 = 8K dongle.

## Transport

Interface `MI_01`, HID collection 5: usage page `0xFF02`, usage `0x02`, 17-byte input and output reports. Plain `WriteFile`/`ReadFile`; each open handle receives its own copy of input reports, so several programs can talk to the mouse at once.

```
offset  0     1    2       3       4       5     6..15    16
        0x08  cmd  status  addrHi  addrLo  len   data[10] checksum
```

- `checksum = (0x55 - sum(bytes[0..15])) & 0xFF`
- `len` is the number of meaningful data bytes, at most 10.
- Requests are sent one at a time. The reply carries the same `cmd`. The vendor's software waits 1 s for a reply (5 s for `0x09`) and tries up to 5 times, with at least 8 ms between writes: the receiver drops packets sent closer together.
- For `0x07` (write flash) the device echoes the full 17-byte packet. Any difference means the write failed.
- `0x0A` arrives unsolicited when something changes on the mouse (see below).

## Commands

| cmd | name | request data | reply data |
|---|---|---|---|
| 0x01 | handshake ("encryption") | len 8, 4 random bytes | `[4]=CID [5]=MID [6]=device type` |
| 0x02 | PC driver status | len 1, `[0]=1 active / 0 inactive` | echo |
| 0x03 | mouse online? | | `[0]!=0` if the mouse is connected to the dongle |
| 0x04 | battery | | `[0]=percent [1]=charging [2..3]=mV (big-endian)` |
| 0x05 | receiver: enter pairing | len 2. The vendor app sends `[0]=0 [1]=0 [2]=CID`: any mouse of that brand. A form with `[0]=CID [1]=MID` also exists (0,0 = any) | |
| 0x06 | pairing status | | `[0]`: 1 pairing, 2 failed, 3 paired. Ask once a second, with other polling paused, and give up after 30 s or if the receiver disappears |
| 0x07 | write flash | addr, len ≤10, bytes | echo |
| 0x08 | read flash | addr, len ≤10 | bytes at addr |
| 0x09 | factory reset | | (slow, 5 s timeout) |
| 0x0A | status changed (device → host) | | bitmask: 0x01 DPI, 0x02 poll rate, 0x04 config, 0x08 DPI LED, 0x10 logo LED, 0x20 LED bar, 0x40 battery |
| 0x0B | set VID/PID | **do not use** | |
| 0x0C | set USB descriptor string | **do not use** | |
| 0x0D | enter USB firmware update | **do not use** | |
| 0x0E | get current profile | | `len 0` = profiles unsupported, else `[0]=index` |
| 0x0F | set current profile | len 1, `[0]=index 0-3` | |
| 0x10 | read CID/MID | | |
| 0x11 | enter MTK (test) mode | **do not use** | |
| 0x12 | firmware version | | `[0..1]` big-endian, e.g. 0x0217 |
| 0x14 | set 4K/8K dongle RGB | len 10, DongleRGB | |
| 0x15 | get dongle RGB | | DongleRGB |
| 0x16 | set long-range mode | len 10, `[0]=0/1` | |
| 0x17 | get long-range mode | | `[0]=0/1` |
| 0xB3 | receiver firmware version | | |

## Flash map (6912 bytes)

Every value is stored with its own check byte so that the group sums to 0x55. A 1-byte value is stored as `v, 0x55-v`; a 3-byte value as `a, b, c, 0x55-a-b-c`. Configuration is changed by writing these encoded bytes back with `0x07`, field by field. The vendor app only writes fields that changed.

| addr | field | encoding / values |
|---|---|---|
| 0x00 | polling rate | pair. `1`=1000, `2`=500, `4`=250, `8`=125, `0x10`=2000, `0x20`=4000, `0x40`=8000 Hz |
| 0x02 | DPI stage count | pair (default 6) |
| 0x04 | current DPI stage | pair, 0-based, must be < count |
| 0x06 | X spindown | pair |
| 0x08 | Y spindown | pair |
| 0x0A | LOD | pair. `3`=0.7 mm, `1`=1 mm, `2`=2 mm |
| 0x0C | DPI stages ×8 | 4 bytes each: `x, y, ex, ck` |
| 0x2C | DPI stage colours ×8 | 4 bytes each: `r, g, b, ck` |
| 0x4C | DPI LED | pairs: mode (0 off, 1 fixed, 2 flicker), brightness, speed, enable |
| 0x60 | buttons ×16 | 4 bytes each: `type, p1, p2, ck` |
| 0xA0 | LED bar | `mode, r, g, b, speed, brightness, ck` + pair at 0xA7 enable |
| 0xA9 | debounce | pair, ms (0-20) |
| 0xAB | motion sync | pair, 0/1 |
| 0xAD | LED-off delay | pair, units of 10 s |
| 0xAF | angle snapping | pair, 0/1 |
| 0xB1 | ripple control | pair, 0/1 |
| 0xB3 | LEDs off while moving | pair, 0/1 |
| 0xB5 | peak performance timeout enable | pair, 0/1 |
| 0xB7 | peak performance timeout | pair, units of 10 s (3=30 s … 90=15 min) |
| 0xB9 | sensor mode | pair. `0`=LP, `1`=HP, `10`=Corded |
| 0xBB | sleep timer | pair, seconds without movement before the mouse sleeps; `0A 4B` = 10 as a Z1 PRO MAX comes. The receiver reports the mouse gone about 6 s later: from the last movement, 10 = 15.9 s, 20 = 25.9 s, 40 = 46.5 s, 80 = 86.2 s, 120 = 126.5 s (5 = 7.6 s). Takes effect at once. The vendor app doesn't offer it |
| 0x100 | shortcuts ×16 | 32 bytes each, see below |
| 0x300 | macros ×16 | 384 bytes each, see below |

### DPI (PAW3395 / PAW3950)

```
raw = dpi / 50 - 1                 # 50-DPI steps
x = y = raw & 0xFF
ex = ((raw >> 8) << 2) | ((raw >> 8) << 6)
decode: dpi = ((x | ((ex >> 6) << 8)) + 1) * 50
```

`ex & 0x22` switches to 100-DPI steps and `ex & 0x11` to doubling; neither is used on the 3395/3950.

### Buttons

`type`: 0 disabled, 1 mouse button, 2 DPI, 3 horizontal scroll, 4 fire key, 5 shortcut/media, 6 macro, 7 polling-rate switch, 8 lighting, 9 profile switch, 10 DPI lock, 11 vertical scroll.

- type 1: `p1` = 0x01 left, 0x02 right, 0x04 middle, 0x08 back, 0x10 forward
- type 2: `p1` = 1 loop, 2 up, 3 down
- type 3 / 11: `p1` = 1 left/up, 2 right/down
- type 4: `p1` = interval (10-255 ms), `p2` = repeat count (0 = while held)
- type 6: `p1` = macro slot, `p2` = repeat count / cycle mode
- type 10: `p1` = (DPI / 50) - 1
- type 5: the action lives in the shortcut slot for this button

Default Z1 PRO MAX layout: 0 left, 1 right, 2 middle, 3 back, 4 forward, 5 DPI loop.

### Shortcut slot (0x100 + i×0x20)

`count` (2-6), then `count` × `[state<<6 | hidType, v0, v1]`, then a check byte over bytes `0..count*3`.
`state`: 0x80 = key down, 0x40 = key up. `hidType`: 0 modifier, 1 key, 2 consumer/media, 3 power, 4 mouse, 5 move XY.

### Macro slot (0x300 + i×0x180)

`nameLen, name[30]`, then at offset 0x1F `count` (2-70), then `count` × `[state<<6 | hidType, v0, v1, delayHi, delayLo]`, then a check byte over `0x1F .. 0x1F + count*5`.
