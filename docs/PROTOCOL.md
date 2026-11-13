# Zaopin / JinLeMa wireless mouse protocol

How hrtz talks to Zaopin's mice and receivers, worked out for interoperability. Verified against a Z1 PRO MAX (CID 7, MID 4) on the 8K receiver; the other models are recognised by the same handshake but not yet tested.

Commands marked **do not use** can change the device's identity or put it into firmware update or test modes. hrtz never sends them.

## Devices

| | VID | PID | Notes |
|---|---|---|---|
| Mouse (wired) | 0x3554 | 0xF526, 0xF50F, 0xF511 | F511 = a Z11 ("2.4G Dual Mode Mouse") |
| Dongle | 0x3554 | 0xF524, 0xF50D, 0xF527, 0xF5D5 | F527 = 8K dongle (CH32V305); F5D5 = a Z11's 1K receiver ("2.4G Wireless Receiver") |

Models are identified by `CID`/`MID` from the handshake, not by PID. CID 7 = Zaopin. MID: 1 ZI PRO, 2 Z2, 3 Z2 mini, 4 Z1 PRO MAX, 5 Z2 MAX, 6 Z2 mini MAX (the vendor app's list), 9 Z11 (PAW3311, from an owner's report, 2026-11-12). Device type 5 = 8K dongle.

A Z11's receiver (F5D5) answered the handshake with CID 0 and MID 0 while the Z11 was paired, likely while it slept (it sleeps after 10 s). hrtz takes 0, 0 as a receiver with no mouse paired, and asks again once the receiver says a mouse is there.

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

### DPI (PAW3311, the Z11)

A table, not steps: the vendor driver's `driver_sensor.h` (`SENSOR_3311_DPI_<dpi>`) gives a code for each DPI from 50 to 10,000 in steps of 50, 200 in all, from 1 to 235 with 35 codes skipped (7, 13, 20, … every 6 or 7). `x = y = code`, `ex = 0`. The vendor app doubles a table DPI where `ex & 0x22`, and again where `ex & 0x11`; hrtz offers 50 to 10,000 only, where every source agrees. ATK's table for its PAW3395 SE is the same, code for code (`atk_dpi::Method::Table`).

A Z11 as it came (its owner's report): codes 31, 18, 9, 56, 75 and 150 at 0x0C, which are 1,350, 800, 400, 2,400, 3,200 and 6,400 DPI by the table (read 50 per step they'd be 1,600, 950, 500, 2,850, 3,800 and 7,550).

### What each sensor has

From the vendor driver's `driver_sensor.h`: motion sync on the 3395 and 3950 only; the sensor mode, ripple control and angle snapping ("fixline") on the 3395, 3311, 3370, 3335 and 3950; lift-off calibration on the 3395, 3370, 3399, 3335 and 3950. hrtz offers the Z11 no motion sync and LP and HP only (Corded is the 3395's and 3950's), and a Zaopin it doesn't know none of these, nor its DPI.

### Buttons

`type`: 0 disabled, 1 mouse button, 2 DPI, 3 horizontal scroll, 4 fire key, 5 shortcut/media, 6 macro, 7 polling-rate switch, 8 lighting, 9 profile switch, 10 DPI lock, 11 vertical scroll.

- type 1: `p1` = 0x01 left, 0x02 right, 0x04 middle, 0x08 back, 0x10 forward
- type 2: `p1` = 1 loop, 2 up, 3 down
- type 3 / 11: `p1` = 1 left/up, 2 right/down
- type 4: `p1` = interval (10-255 ms), `p2` = repeat count (0 = while held)
- type 6: `p1` = macro slot, `p2` = repeat count / cycle mode
- type 10: `p1` = (DPI / 50) - 1, or the table's code on a PAW3311
- type 5: the action lives in the shortcut slot for this button

Default Z1 PRO MAX layout: 0 left, 1 right, 2 middle, 3 back, 4 forward, 5 DPI loop.

### Shortcut slot (0x100 + i×0x20)

`count` (2-6), then `count` × `[state<<6 | hidType, v0, v1]`, then a check byte over bytes `0..count*3`.
`state`: 0x80 = key down, 0x40 = key up. `hidType`: 0 modifier, 1 key, 2 consumer/media, 3 power, 4 mouse, 5 move XY.

### Macro slot (0x300 + i×0x180)

`nameLen, name[30]`, then at offset 0x1F `count` (2-70), then `count` × `[state<<6 | hidType, v0, v1, delayHi, delayLo]`, then a check byte over `0x1F .. 0x1F + count*5`.

## ATK

ATK's mice use this channel too: the same packets, checksum, commands and flash addresses. hrtz runs every named ATK, VXE and MAD model in ATK HUB 3.2.32's own list (160), in `src-tauri/src/mice/compx/atk_catalog.rs`, and offers on each what its sensor allows (`atk::Sensor`). What differs, from OpenMouse's `mouse-protocol`, ATK's own app and two Inspect a mouse reports from an owner:

- VID 0x373B. The U2 Plus's NearLink receiver is PID 0x10C9 ("NK mouse NANO dongle"); the mouse on its cable is 0x110D. Both have a second `0xFF02:0x02` collection with 19-byte reports (report 0x13); the settings channel is the one with report 0x08.
- The model comes from command `0x10`, reply `[0]=CID [1]=MID` (the U2 Plus is CID 2, MID 78). ATK's app also sends the handshake (`0x01`, as a Zaopin's) and reads only its connection type, `[6]`: 0 a 1K receiver, 1 4K, 4 2K, 5 8K, 8 NearLink; 2 a cable at 1K, 3 at 8K, 6 at 2K, 7 at 4K (0 to 5 as a Zaopin's). It offers 125 to 1,000 Hz on all, up to 8,000 on 3, 5 and 8, 4,000 on 1 and 2,000 on 4. The polling codes at 0x00 are a Zaopin's.
- Some ATK mice (VXE's R1 line, ATK's Dragonfly F1) are on CompX's own vendor ID, 0x3554, as Zaopin's are, several models to a product ID. hrtz sends them the handshake as it would a Zaopin, and takes one whose CID and MID are a model it runs, on a product ID ATK's app lists there, as an ATK.
- Per model, from ATK's app: whether it has a DPI light (its colours and the light at 0x4C aren't offered or written without one), and whether 0 ms debounce is offered.
- DPI stages by sensor (`atk_dpi.rs`; ATK HUB 3.2.32 and mouse-protocol's reading of 3.2.27 agree on these). A 4-byte stage is X, Y, a byte of two mode nibbles (X low, Y high) and a check byte:
  - PAW3395, PAW3395 Ultra, PAW3950, PAW3950 DM: 50 per step, the count's top bits in nibble bits 2-3 (a Zaopin's); above 30,000 (PAW3950), 100 per step with bit 0.
  - PAW3950 Ultra: 10 per step to 10,000, then `(dpi - 10050) / 50` with bit 1; above 30,000 halved with bit 0.
  - PAW3395 SE: a table of 200 codes for 50 to 10,000 (1 to 235, skipping 35); above 10,000 halved with bit 1.
  - PAW3955: 6-byte stages at 0x1B00 + 6i (8 of them): X and Y as `dpi - 1` in 16 bits, flags (bit 0 X doubled, bit 4 Y), check byte; `dpi / 2 - 1` with the flag from 40,000.
  - Not settled, so not offered: PAW3311 and PAW3315 (ATK HUB 3.2.32 uses lookup tables, the SE's and its own; mouse-protocol counts 100 per step from 200) and CORE26K (ATK reads a step-100 and a doubled flag that mouse-protocol doesn't). A Zaopin PAW3311 uses the SE's table (above), which backs ATK HUB's reading, but ATK's table marks 5,050 to 9,000 as doubled where Zaopin's doesn't, so an ATK PAW3311 stays off until an owner's check shows which.
- Lift-off on a PAW3950 Ultra and a PAW3955 is codes 1 to 11 for 0.7 to 1.7 mm, at the same 0x0A: code 2 is 0.8 mm there but 2 mm on a PAW3395.
- The VXE R1 SE's 1K receiver (0x373B:0x1085) keeps the polling rate in a row of its own (mouse-protocol's "live settings"), not at 0x00: hrtz doesn't offer it there.
- ATK's app writes in groups: the system row 0x00 to 0x09 whole (0x06 and 0x08 hold other settings, written back), the DPI stages and colours two at a time, a PAW3955's stages one at a time. hrtz does the same; mouse-protocol's captures of an R1 Pro Max agree.
- `0x04` battery answers with a length of 2, so only its level and charging are read.
- Writes are echoed, as a Zaopin's are. hrtz also reads each write back, and writes a whole field at a time, as ATK's app does.
- The same as a Zaopin's: the system row (0x00, 0x02, 0x04), lift-off at 0x0A (1 = 1 mm, 2 = 2 mm, 3 = 0.7 mm on a PAW3950), the DPI stages (0x0C, up to six) and colours (0x2C), the DPI light (0x4C; ATK's app offers brightness 16, 128 and 255, speed 1, 3 and 5), debounce (0xA9; 0, 1, 2, 4, 8, 15 or 20 ms), motion sync (0xAB), straight-line correction (0xAF, a Zaopin's angle snapping) and ripple control (0xB1).
- Profiles: ATK's app shows four, read and switched with `0x0E` and `0x0F` as a Zaopin's are. hrtz sends a switch without counting on an answer, asks `0x0E` after, and asks it every second while the mouse is awake, so a switch made on the mouse shows too.
- Different: the sleep time is 0xAD and 0xB7 in 10 s steps (a Zaopin's is 0xBB, in seconds), and Corded sensor mode is 2 at 0xB9 (a Zaopin's is 10). hrtz leaves both, and the buttons, as the mouse holds them for now.
