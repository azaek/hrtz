//! Inspect a mouse's reading of a mouse on CompX's channel that hrtz doesn't run: another maker's
//! (VGN, Lamzu, Pulsar and others share Zaopin's vendor ID and messages), or an ATK it can't name.
//! Who it is, what it answers, what its memory holds and what hrtz makes of that (`pieces`), so
//! its owner's one report has what adding it takes.
//!
//! Only when its owner starts the check, and it asks what CompX's makers' own apps ask: the
//! handshake (which hrtz asked already, to tell whose mouse it is), whether the receiver's mouse
//! is linked, its firmware, profile and battery, and its memory. An ATK is also asked who it is
//! (0x10), as ATK's app asks; a mouse on Zaopin's vendor ID for its long-range mode, as Zaopin's
//! does. Every question and its answer is kept as sent and received (`asked`).
//!
//! Nothing is written to its memory. A mouse that's there but gives none of it is told an app is
//! listening (0x02), as its maker's app tells it before reading, asked again, and told the app
//! has gone once it's read.
//!
//! One change can be tried, when its owner asks for it (`Trials`): the polling rate, to another
//! rate every mouse on this channel has, and back.

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use base64::Engine as _;
use parking_lot::Mutex;
use serde_json::{Value, json};

use super::atk;
use super::driver::{Connected, Maker, Opened, rand_bytes};
use super::image;
use super::pieces;
use super::protocol::{self as p, Packet, cmd};
use crate::mice::Driver;

/// How long an answer is waited for, twice: a mouse that doesn't know a question says nothing.
const QUICK: Duration = Duration::from_millis(300);
/// How much memory is read between two words on how far it is.
const TOLD_EVERY: usize = 200;
/// What's read past a Zaopin's memory: where a PAW3955 keeps its stages (`atk::memory_len`). A
/// mouse that keeps nothing there says nothing, and its memory is whole without it.
const FURTHER: usize = 6 * 8;
/// The row a trial change is written in, whole, as ATK's app writes it (`atk::fields`): the
/// polling rate first.
const ROW: usize = 10;
/// A change tried and not put back by its window (closed, or hung) is put back after this long.
const PUT_BACK_AFTER: Duration = Duration::from_secs(45);
/// How long between two tries at putting one back, for a mouse that's asleep or doesn't take it.
const TRY_AGAIN_AFTER: Duration = Duration::from_secs(5);
/// How many times a put-back the mouse doesn't keep is written before hrtz lets it be.
const TRIES: u8 = 3;

fn hex(r: &[u8]) -> String {
    r.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// Every question put to the mouse, in order, with what came back.
#[derive(Default)]
struct Asked(Vec<Value>);

impl Asked {
    fn ask(&mut self, m: &mut Connected, what: &'static str, pkt: Packet) -> Option<Packet> {
        let at = Instant::now();
        let got = m.ask(&pkt, QUICK, 2).ok();
        self.0.push(json!({
            "what": what,
            "sent": hex(&pkt),
            "got": got.as_ref().map(|r| hex(r)),
            "ms": at.elapsed().as_millis() as u64,
        }));
        got
    }
}

fn simple(command: u8) -> Packet {
    p::packet(command, 0, &[], None)
}

/// Whether a receiver's answer says its mouse is linked: any non-zero on Zaopin's vendor ID, 1 on
/// ATK's, as hrtz reads each when it runs one.
fn linked(maker: Maker, answer: &Packet) -> bool {
    match maker {
        Maker::Zaopin => answer[6] != 0,
        Maker::Atk { .. } => answer[6] == 1,
    }
}

/// Read the mouse on `opened`. A receiver whose mouse isn't linked gives `asleep`, with who the
/// receiver says it is; otherwise who it is, what it answered, and its memory as far as it gave it,
/// read a piece at a time.
pub(crate) fn read(opened: Opened, progress: &mut dyn FnMut(usize, usize)) -> Result<Value> {
    let maker = opened.maker;
    let mut m = Connected::bare(opened.link, opened.gap);
    let mut asked = Asked::default();

    let handshake = asked.ask(&mut m, "handshake", p::packet(cmd::HANDSHAKE, 0, &rand_bytes(), Some(8)));
    let ids = handshake.map(|r| {
        let d = p::payload(&r);
        (d[4], d[5], d[6])
    });
    // An ATK's connection is told by its USB IDs, as when hrtz meets one; another's by the
    // handshake's connection type.
    let wired = match (maker, ids) {
        (Maker::Atk { receiver }, _) => !receiver,
        (Maker::Zaopin, Some((_, _, kind))) => atk::link::wired(kind),
        (Maker::Zaopin, None) if m.lost() => bail!("It was unplugged."),
        (Maker::Zaopin, None) => bail!("It didn't answer. Unplug it, plug it back in, and check again."),
    };
    let answered_as = ids.map(|(cid, mid, kind)| json!({ "cid": cid, "mid": mid, "deviceType": kind }));

    // On its cable it's always there. A receiver that doesn't say is taken by whether its mouse
    // answers for its firmware.
    let mut version = None;
    let awake = wired
        || match asked.ask(&mut m, "online", simple(cmd::ONLINE)) {
            Some(r) => linked(maker, &r),
            None => {
                version = asked.ask(&mut m, "version", simple(cmd::VERSION));
                version.is_some()
            }
        };
    if m.lost() {
        bail!("It was unplugged.");
    }
    if !awake {
        return Ok(json!({ "asleep": true, "wired": false, "answeredAs": answered_as, "asked": asked.0 }));
    }

    let who =
        if matches!(maker, Maker::Atk { .. }) { asked.ask(&mut m, "whoItIs", simple(atk::CID_MID)) } else { None };
    let version = version.or_else(|| asked.ask(&mut m, "version", simple(cmd::VERSION)));
    let profile = asked.ask(&mut m, "profile", simple(cmd::GET_PROFILE));
    let long_range =
        if maker == Maker::Zaopin { asked.ask(&mut m, "longRange", simple(cmd::GET_LONG_RANGE)) } else { None };
    let battery = asked.ask(&mut m, "battery", simple(cmd::BATTERY));

    let total = p::FLASH_SIZE;
    let mut memory = Vec::with_capacity(total + FURTHER);
    let mut stopped = None;
    // Whether it had to be told an app is listening, and whether it took that.
    let mut listening = None;
    progress(0, total);
    while memory.len() < total {
        let n = p::MAX_CHUNK.min(total - memory.len());
        match m.read_flash(memory.len(), n) {
            Ok(row) => memory.extend(row),
            // There, and none of its memory given: its maker's app says it's listening before it
            // reads. Said once, and the memory asked for again.
            Err(_) if memory.is_empty() && listening.is_none() && maker == Maker::Zaopin && !m.lost() => {
                let told = asked.ask(&mut m, "listening", p::packet(cmd::PC_DRIVER_STATUS, 0, &[1], None));
                listening = Some(told.is_some());
            }
            Err(e) => {
                stopped = Some(format!("{e:#}"));
                break;
            }
        }
        if memory.len().is_multiple_of(TOLD_EVERY) || memory.len() == total {
            progress(memory.len(), total);
        }
    }
    // Past a Zaopin's memory, where a PAW3955's stages are: a mouse with nothing there says
    // nothing, and that's not a memory read short.
    let mut further = 0;
    while stopped.is_none() && further < FURTHER {
        let n = p::MAX_CHUNK.min(FURTHER - further);
        let pkt = p::packet(cmd::READ_FLASH, (total + further) as u16, &[], Some(n as u8));
        let Ok(r) = m.ask(&pkt, QUICK, 2) else { break };
        memory.extend_from_slice(&r[6..6 + n]);
        further += n;
    }
    // Told whether or not its answer came: it may have heard.
    if listening.is_some() {
        asked.ask(&mut m, "notListening", p::packet(cmd::PC_DRIVER_STATUS, 0, &[0], None));
    }
    if m.lost() {
        bail!("It was unplugged.");
    }
    // A read that stopped short: whether its mouse went to sleep meanwhile.
    let fell_asleep = stopped.is_some()
        && !wired
        && asked.ask(&mut m, "online", simple(cmd::ONLINE)).is_some_and(|r| !linked(maker, &r));

    // An ATK says who it is by 0x10; another by the handshake.
    let named = who.map(|r| (r[6], r[7])).or(ids.map(|(cid, mid, _)| (cid, mid)));
    // A model ATK's own list has, met where hrtz doesn't run it: its name, and how its sensor
    // keeps its DPI.
    let listed = named.and_then(|(cid, mid)| atk::model(cid, mid));
    let picture = pieces::read(&memory, maker, listed.and_then(|x| x.sensor.dpi_method()));
    Ok(json!({
        "asleep": false,
        "wired": wired,
        "answeredAs": answered_as,
        "model": who.map(|r| json!({ "cid": r[6], "mid": r[7] })),
        "knownAs": listed.map(|x| format!("{} {}", x.brand, x.name)),
        // As a Zaopin's and an ATK's: hex major.minor.
        "firmware": version.map(|r| format!("{:x}.{:02x}", r[6], r[7])),
        "profile": profile.filter(|r| r[5] > 0).map(|r| r[6]),
        "longRange": long_range.map(|r| r[6] != 0),
        "battery": battery.map(|r| json!({ "level": r[6], "charging": r[7] != 0 })),
        "asked": asked.0,
        "listening": listening,
        "memoryWanted": total,
        "memorySize": memory.len(),
        "memory": base64::engine::general_purpose::STANDARD.encode(&memory),
        "stopped": stopped,
        "fellAsleep": fell_asleep,
        "picture": picture,
    }))
}

/// What a change being tried put aside.
#[derive(Clone, Copy)]
struct Held {
    /// When the change was made, and when putting it back was last tried.
    at: Instant,
    tried: Instant,
    /// The two bytes of its polling rate as they were, and as hrtz set them.
    original: [u8; 2],
    wrote: [u8; 2],
    /// How many put-backs the mouse didn't keep.
    tries: u8,
}

/// What's held, by connection, for as long as hrtz runs. It outlives the connection's own thread:
/// a mouse unplugged while a change is tried has it put back once it's plugged into the same port
/// again.
static HELD: LazyLock<Mutex<HashMap<String, Held>>> = LazyLock::new(Mutex::default);

/// A change tried on a mouse hrtz leaves alone, when its owner asks: its polling rate set to
/// another rate, for Inspect a mouse to measure, then put back. Proof in one inspection that a
/// change holds, which otherwise takes a second report once the mouse is added.
///
/// Only the polling rate: it's the one setting hrtz can check by itself, by counting the
/// mouse's reports. And only where its two bytes read as a rate this channel has, so nothing is
/// written where hrtz doesn't know what's kept. What's held is those two bytes as they were, and
/// as hrtz set them: they go back as read, whatever they mean, and only while the mouse still
/// holds what hrtz set (a rate its owner or its own app has set since is theirs).
pub(crate) struct Trials {
    /// The connection: what's held for it is found by this.
    id: String,
}

impl Trials {
    pub fn of(id: &str) -> Self {
        Self { id: id.to_string() }
    }

    fn held(&self) -> Option<Held> {
        HELD.lock().get(&self.id).copied()
    }

    /// A change is tried and not put back yet.
    pub fn pending(&self) -> bool {
        self.held().is_some()
    }

    /// One tried too long ago: its window went without putting it back. Not asked for more often
    /// than every few seconds, for a mouse that's asleep.
    pub fn overdue(&self) -> bool {
        self.held().is_some_and(|h| h.at.elapsed() >= PUT_BACK_AFTER && h.tried.elapsed() >= TRY_AGAIN_AFTER)
    }

    /// As if the change was made `by` ago, and its put-back last tried then.
    #[cfg(test)]
    pub fn age(&self, by: Duration) {
        if let Some(h) = HELD.lock().get_mut(&self.id) {
            h.at = h.at.checked_sub(by).expect("a time before now");
            h.tried = h.tried.checked_sub(by).expect("a time before now");
        }
    }

    /// The connection with its mouse there (on its cable, or linked) and the two bytes of its
    /// polling rate as they are. A mouse that gives no memory until told an app is listening is
    /// told, as `read` tells it: `true` then, for `done` to tell it the app has gone.
    fn there(opened: Opened) -> Result<(Connected, bool, [u8; 2])> {
        let maker = opened.maker;
        let mut m = Connected::bare(opened.link, opened.gap);
        let wired = match maker {
            Maker::Atk { receiver } => !receiver,
            Maker::Zaopin => m
                .ask(&p::packet(cmd::HANDSHAKE, 0, &rand_bytes(), Some(8)), QUICK, 2)
                .map(|r| atk::link::wired(atk::kind_of(&r)))
                .map_err(|_| anyhow!("It didn't answer. Unplug it, plug it back in, and try again."))?,
        };
        if !wired && !m.ask(&simple(cmd::ONLINE), QUICK, 2).is_ok_and(|r| linked(maker, &r)) {
            bail!("The mouse is asleep. Move it, then try again.");
        }
        let (told, now) = match m.read_flash(0, 2) {
            Ok(now) => (false, now),
            Err(e) if maker != Maker::Zaopin || m.lost() => return Err(e),
            Err(_) => {
                let _ = m.ask(&p::packet(cmd::PC_DRIVER_STATUS, 0, &[1], None), QUICK, 2);
                match m.read_flash(0, 2) {
                    Ok(now) => (true, now),
                    Err(e) => {
                        Self::done(&mut m, true);
                        return Err(e);
                    }
                }
            }
        };
        Ok((m, told, [now[0], now[1]]))
    }

    /// Tell a mouse that was told an app is listening that it has gone.
    fn done(m: &mut Connected, told: bool) {
        if told {
            let _ = m.ask(&p::packet(cmd::PC_DRIVER_STATUS, 0, &[0], None), QUICK, 2);
        }
    }

    /// The row with the polling rate, written whole with `rate` in it, then read back.
    fn write(m: &mut Connected, rate: [u8; 2]) -> Result<bool> {
        // As it is now: the stage in use is in this row, and its DPI button may have moved it.
        let mut row = m.read_flash(0, ROW)?;
        row[..2].copy_from_slice(&rate);
        m.send_unanswered(&p::packet(cmd::WRITE_FLASH, 0, &row, None))?;
        Ok(m.read_flash(0, 2)? == rate)
    }

    /// Set the polling rate to another one: 500 Hz, or 1,000 from 500. Both are on every mouse
    /// and every connection of this channel.
    pub fn change(&mut self, opened: Opened) -> Result<Value> {
        if self.pending() {
            bail!("A change is being tried already.");
        }
        let (mut m, told, was) = Self::there(opened)?;
        let made = (|| {
            let from = image::pair(&was, 0).and_then(image::polling_hz).ok_or_else(|| {
                anyhow!("hrtz doesn't know how this mouse keeps its polling rate, so nothing was changed.")
            })?;
            let to = if from == 500 { 1000 } else { 500 };
            let code = image::polling_code(to).expect("a rate of this channel");
            let wrote = [code, p::checksum(&[code])];
            // Held before it's written: a write that fails halfway is put back too.
            let now = Instant::now();
            HELD.lock().insert(self.id.clone(), Held { at: now, tried: now, original: was, wrote, tries: 0 });
            let kept = Self::write(&mut m, wrote)?;
            Ok(json!({ "what": "Polling rate", "from": from, "to": to, "kept": kept }))
        })();
        Self::done(&mut m, told);
        made
    }

    /// Put back what `change` found, where the mouse still holds what `change` set: a rate set
    /// since by its owner or its own app is left as it is. A mouse that's asleep keeps the change
    /// until it's asked again; one that doesn't keep the put-back is let be after a few tries.
    pub fn restore(&mut self, opened: Opened) -> Result<Value> {
        let Some(h) = self.held() else { bail!("No change is being tried.") };
        if let Some(h) = HELD.lock().get_mut(&self.id) {
            h.tried = Instant::now();
        }
        let (mut m, told, now) = Self::there(opened)?;
        let put_back = if now == h.original {
            // It never kept the change, or has it back already.
            Ok(Some(true))
        } else if now != h.wrote {
            Ok(None)
        } else {
            Self::write(&mut m, h.original).map(Some)
        };
        Self::done(&mut m, told);
        let restored = match put_back {
            Ok(Some(true)) => true,
            // Not what hrtz set: nothing of hrtz's is left to put back.
            Ok(None) => {
                HELD.lock().remove(&self.id);
                return Ok(json!({ "restored": false, "setSince": true }));
            }
            // Written, and not kept, or not written at all: tried again a few times at most.
            not => {
                let tries = h.tries + 1;
                if tries >= TRIES {
                    eprintln!("a tried polling rate wasn't put back after {tries} tries; left as the mouse holds it");
                    HELD.lock().remove(&self.id);
                } else if let Some(h) = HELD.lock().get_mut(&self.id) {
                    h.tries = tries;
                }
                not?;
                false
            }
        };
        if restored {
            HELD.lock().remove(&self.id);
        }
        Ok(json!({ "restored": restored }))
    }
}
