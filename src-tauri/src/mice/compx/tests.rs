//! The engine and the Zaopin driver against a simulated mouse (`sim.rs`): what's sent, what's
//! read, and what the UI is told. Written before the device code moved here, to pin its behaviour.

use std::collections::VecDeque;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use parking_lot::{Mutex, RwLock};

use super::driver::LOADED_REGION;
use super::image::{self, sample_image};
use super::protocol::{self as p, checksum, cmd, status};
use super::sim::{SimFinder, SimMouse};
use crate::battery_log::BatteryLog;
use crate::mice::model::Settings;
use crate::mice::{DeviceEvent, DeviceState, Engine, Notify, Worker};

type Seen = Arc<Mutex<Vec<(DeviceState, Option<DeviceEvent>)>>>;

struct Rig {
    w: Worker,
    mouse: Arc<Mutex<SimMouse>>,
    seen: Seen,
}

impl Rig {
    fn new(m: SimMouse) -> Self {
        let mouse = Arc::new(Mutex::new(m));
        let seen: Seen = Arc::default();
        let sink = seen.clone();
        let notify: Notify = Arc::new(move |s, e| sink.lock().push((s.clone(), e)));
        let state = Arc::new(RwLock::new(DeviceState::default()));
        let log = Arc::new(Mutex::new(BatteryLog::load(None)));
        let w = Worker::new(Box::new(SimFinder(mouse.clone())), state, notify, None, log);
        Self { w, mouse, seen }
    }

    /// Found, and read if awake: the first round of housekeeping.
    fn connected(m: SimMouse) -> Self {
        let mut r = Self::new(m);
        r.w.housekeeping();
        r
    }

    fn state(&self) -> DeviceState {
        self.w.snapshot()
    }

    /// How many requests and publishes so far, to look at what comes after.
    fn mark(&self) -> (usize, usize) {
        (self.mouse.lock().requests.len(), self.seen.lock().len())
    }

    fn events_since(&self, from: usize) -> Vec<DeviceEvent> {
        self.seen.lock()[from..].iter().filter_map(|x| x.1.clone()).collect()
    }

    fn settings(&self) -> Settings {
        self.state().settings.expect("settings read")
    }
}

/// The test image at another polling rate: 125 Hz.
fn image_at_125() -> Vec<u8> {
    let mut f = sample_image();
    f[0x00] = 0x08;
    f[0x01] = checksum(&[0x08]);
    f
}

#[test]
fn connects_and_reads_everything_the_ui_shows() {
    let r = Rig::connected(SimMouse::default());
    let s = r.state();
    assert!(s.connected && s.online);
    assert_eq!(s.model.as_deref(), Some("Z1 PRO MAX"));
    assert_eq!((s.cid, s.mid, s.device_type), (7, 4, 5));
    assert_eq!(s.firmware.as_deref(), Some("2.17"));
    assert_eq!(s.profile, Some(0));
    assert_eq!(s.long_range, Some(false));
    assert_eq!(s.settings, Some(image::decode(&sample_image())));
    assert!(s.capabilities.is_some());
    let b = s.battery.expect("battery read");
    assert_eq!((b.raw_level, b.charging, b.millivolts), (80, false, 3950));

    let m = r.mouse.lock();
    // The handshake, "an app is listening", then what's shown: firmware, profile, long range,
    // the first 0x300 bytes of memory, the battery.
    let commands = m.commands_since(0);
    assert_eq!(
        commands[..6],
        [cmd::HANDSHAKE, cmd::PC_DRIVER_STATUS, cmd::ONLINE, cmd::VERSION, cmd::GET_PROFILE, cmd::GET_LONG_RANGE]
    );
    assert_eq!(m.requests[1][6], 1, "status notifications on");
    assert_eq!(commands.last(), Some(&cmd::BATTERY));
    let reads = m.reads_since(0);
    assert_eq!(reads.first(), Some(&(0, p::MAX_CHUNK)));
    assert_eq!(reads.iter().map(|x| x.1).sum::<usize>(), LOADED_REGION);
}

#[test]
fn an_asleep_mouse_shows_connected_and_isnt_read() {
    let r = Rig::connected(SimMouse { online: false, ..Default::default() });
    let s = r.state();
    assert!(s.connected && !s.online);
    assert!(s.settings.is_none() && s.firmware.is_none() && s.battery.is_none());
    assert!(r.mouse.lock().reads_since(0).is_empty());
}

#[test]
fn waking_shows_awake_at_once_then_the_settings() {
    let mut r = Rig::connected(SimMouse { online: false, ..Default::default() });
    let (_, seen) = r.mark();
    r.mouse.lock().online = true;
    r.w.housekeeping();
    let published: Vec<DeviceState> = r.seen.lock()[seen..].iter().map(|x| x.0.clone()).collect();
    assert!(published[0].online && published[0].settings.is_none(), "awake shows before the settings are read");
    assert!(published.last().is_some_and(|s| s.settings.is_some()));
}

#[test]
fn falling_asleep_is_noticed() {
    let mut r = Rig::connected(SimMouse::default());
    r.mouse.lock().online = false;
    r.w.housekeeping();
    let s = r.state();
    assert!(s.connected && !s.online);
    assert!(s.settings.is_some(), "the last settings stay on show");
}

#[test]
fn the_dpi_button_reads_four_bytes_and_says_the_new_dpi() {
    let mut r = Rig::connected(SimMouse::default());
    assert_eq!(r.settings().dpi_stage, Some(1));
    let (req, seen) = r.mark();
    {
        let mut m = r.mouse.lock();
        m.put_pair(0x04, 2);
        m.status(status::DPI);
    }
    r.w.drain_unsolicited();
    let s = r.settings();
    assert_eq!(s.dpi_stage, Some(2));
    assert_eq!(r.mouse.lock().reads_since(req), vec![(0x02, 4)]);
    let events = r.events_since(seen);
    assert!(matches!(events[..], [DeviceEvent::Dpi { stage: 2, dpi: 1600 }]), "{events:?}");
}

#[test]
fn a_polling_change_on_the_mouse_reads_two_bytes() {
    let mut r = Rig::connected(SimMouse::default());
    let (req, seen) = r.mark();
    {
        let mut m = r.mouse.lock();
        m.put_pair(0x00, 0x08);
        m.status(status::POLLING);
    }
    r.w.drain_unsolicited();
    assert_eq!(r.settings().polling_hz, Some(125));
    assert_eq!(r.mouse.lock().reads_since(req), vec![(0x00, 2)]);
    assert!(matches!(r.events_since(seen)[..], [DeviceEvent::Polling { hz: 125 }]));
}

#[test]
fn a_battery_change_on_the_mouse_is_read() {
    let mut r = Rig::connected(SimMouse::default());
    let (req, _) = r.mark();
    {
        let mut m = r.mouse.lock();
        m.battery = (79, true, 4010);
        m.status(status::BATTERY);
    }
    r.w.drain_unsolicited();
    assert_eq!(r.mouse.lock().commands_since(req), vec![cmd::BATTERY]);
    let b = r.state().battery.unwrap();
    assert_eq!((b.raw_level, b.charging, b.millivolts), (79, true, 4010));
}

#[test]
fn a_profile_switch_on_the_mouse_loads_that_profile() {
    let mut m = SimMouse::default();
    m.profiles[2] = image_at_125();
    let mut r = Rig::connected(m);
    let (req, seen) = r.mark();
    {
        let mut m = r.mouse.lock();
        m.active = 2;
        m.status(status::PROFILE);
    }
    r.w.drain_unsolicited();
    assert_eq!(r.state().profile, Some(2));
    assert_eq!(r.settings().polling_hz, Some(125));
    assert!(matches!(r.events_since(seen)[..], [DeviceEvent::Profile { index: 2 }]));
    assert_eq!(r.mouse.lock().reads_since(req).iter().map(|x| x.1).sum::<usize>(), LOADED_REGION);
}

#[test]
fn switching_profile_pops_up_once_with_its_settings_and_skips_the_echo() {
    let mut m = SimMouse::default();
    m.profiles[1] = image_at_125();
    let mut r = Rig::connected(m);
    let (req, seen) = r.mark();
    r.w.set_profile(1).unwrap();
    // Awake first, then the switch.
    assert_eq!(r.mouse.lock().commands_since(req)[..2], [cmd::ONLINE, cmd::SET_PROFILE]);
    assert_eq!(r.mouse.lock().requests[req + 1][1..7], [cmd::SET_PROFILE, 0, 0, 0, 1, 1]);
    // The profile's settings first, then its key-combination slots.
    let reads = r.mouse.lock().reads_since(req);
    let split = reads.iter().position(|x| x.0 >= p::CONFIG_REGION).unwrap();
    assert_eq!(reads[..split].iter().map(|x| x.1).sum::<usize>(), p::CONFIG_REGION);
    assert_eq!(reads[split..].iter().map(|x| x.1).sum::<usize>(), LOADED_REGION - p::CONFIG_REGION);
    // The pop-up already has the new profile's settings.
    let popped = r.seen.lock()[seen..].iter().find(|x| matches!(x.1, Some(DeviceEvent::Profile { index: 1 }))).cloned();
    assert_eq!(popped.and_then(|x| x.0.settings).and_then(|s| s.polling_hz), Some(125));
    // The mouse's echo of the switch is checked, not loaded again, and doesn't pop up again.
    let (req, _) = r.mark();
    r.w.drain_unsolicited();
    assert_eq!(r.mouse.lock().commands_since(req), vec![cmd::GET_PROFILE]);
    assert_eq!(r.events_since(seen).iter().filter(|e| matches!(e, DeviceEvent::Profile { .. })).count(), 1);
    assert_eq!(r.state().profile, Some(1));
}

#[test]
fn applying_writes_only_what_changed() {
    let mut r = Rig::connected(SimMouse::default());
    let mut next = r.settings();
    next.polling_hz = Some(125);
    next.stages.as_mut().unwrap()[0].dpi = 3200;
    let (req, _) = r.mark();
    let applied = r.w.apply_settings(&next).unwrap();
    assert_eq!(applied, next);
    assert_eq!(r.state().settings.as_ref(), Some(&next));
    assert_eq!(image::decode(r.mouse.lock().image()), next);
    // The rate's two bytes and the first stage's DPI, and nothing else.
    let writes = r.mouse.lock().writes_since(req);
    assert!(!writes.is_empty());
    assert!(writes.iter().all(|a| (0x00..0x02).contains(a) || (0x0C..0x10).contains(a)), "{writes:02X?}");
}

#[test]
fn applying_refuses_what_the_mouse_cant_do_and_writes_nothing() {
    let mut r = Rig::connected(SimMouse::default());
    let mut next = r.settings();
    next.stages.as_mut().unwrap()[0].dpi = 30_050;
    let (req, _) = r.mark();
    let e = r.w.apply_settings(&next).unwrap_err().to_string();
    assert!(e.contains("multiple of 50"), "{e}");
    assert!(r.mouse.lock().writes_since(req).is_empty());
}

#[test]
fn a_write_the_mouse_refuses_is_an_error() {
    let mut r = Rig::connected(SimMouse::default());
    r.mouse.lock().reject_write_at = Some(0x00);
    let mut next = r.settings();
    next.polling_hz = Some(125);
    let e = r.w.apply_settings(&next).unwrap_err().to_string();
    assert!(e.contains("rejected a write at 0x0000"), "{e}");
}

#[test]
fn changes_wait_for_the_mouse_to_wake() {
    let mut r = Rig::connected(SimMouse { online: false, ..Default::default() });
    let e = r.w.set_profile(1).unwrap_err().to_string();
    assert!(e.contains("asleep"), "{e}");
}

#[test]
fn a_backup_reads_all_of_memory_and_shows_progress() {
    let mut r = Rig::connected(SimMouse::default());
    let (_, seen) = r.mark();
    let image = r.w.read_full_image().unwrap();
    assert_eq!(image, r.mouse.lock().image());
    let progress: Vec<(usize, usize)> = r
        .events_since(seen)
        .iter()
        .filter_map(|e| match e {
            DeviceEvent::Progress { done, total } => Some((*done, *total)),
            _ => None,
        })
        .collect();
    assert_eq!(progress.len(), p::FLASH_SIZE.div_ceil(200));
    assert_eq!(progress.last(), Some(&(p::FLASH_SIZE, p::FLASH_SIZE)));
}

#[test]
fn a_quiet_backup_shows_no_progress() {
    let mut r = Rig::connected(SimMouse::default());
    let (_, seen) = r.mark();
    let image = r.w.read_full_image_quietly().unwrap();
    assert_eq!(image.len(), p::FLASH_SIZE);
    assert!(r.events_since(seen).is_empty());
}

#[test]
fn restoring_writes_the_differences_and_checks_each() {
    let mut r = Rig::connected(SimMouse::default());
    let current = r.mouse.lock().image().to_vec();
    let mut target = image_at_125();
    target[0x500] ^= 0x5A; // a byte of a macro
    let written = r.w.write_image(&current, &target).unwrap();
    assert_eq!(written, 2);
    assert_eq!(r.mouse.lock().image(), &target[..]);
    assert_eq!(r.settings().polling_hz, Some(125));
}

#[test]
fn a_restore_the_mouse_doesnt_keep_fails_its_check() {
    let mut r = Rig::connected(SimMouse::default());
    r.mouse.lock().forget_writes = true;
    let current = r.mouse.lock().image().to_vec();
    let e = r.w.write_image(&current, &image_at_125()).unwrap_err().to_string();
    assert!(e.contains("Verification failed at 0x0000"), "{e}");
}

#[test]
fn a_lost_receiver_clears_everything_and_is_found_again() {
    let mut r = Rig::connected(SimMouse::default());
    r.mouse.lock().unplugged = true;
    r.w.housekeeping();
    let s = r.state();
    assert!(!s.connected && s.settings.is_none());
    assert!(r.seen.lock().last().is_some_and(|x| !x.0.connected), "the UI is told");
    r.mouse.lock().unplugged = false;
    r.w.housekeeping();
    assert!(r.state().connected);
}

#[test]
fn pairing_asks_the_receiver_until_a_mouse_pairs() {
    let pairing = VecDeque::from([p::pair_state::PAIRING, p::pair_state::PAIRED]);
    let mut r = Rig::connected(SimMouse { pair_states: pairing, ..Default::default() });
    let (req, seen) = r.mark();
    assert!(r.w.pair().unwrap());
    assert_eq!(r.mouse.lock().requests[req], p::pair_packet(7));
    assert!(r.events_since(seen).iter().any(|e| matches!(e, DeviceEvent::Pairing { .. })));
    assert!(!r.state().connected, "met again after pairing, in case it's another model");
}

#[test]
fn pairing_needs_one_receiver_and_no_cable() {
    let mut r = Rig::connected(SimMouse { receivers: 2, ..Default::default() });
    assert!(r.w.pair().unwrap_err().to_string().contains("More than one receiver"));
    let mut r = Rig::connected(SimMouse { device_type: 3, ..Default::default() });
    assert!(r.w.pair().unwrap_err().to_string().contains("cable"));
}

#[test]
fn a_factory_reset_reads_the_mouse_again() {
    let mut m = SimMouse::default();
    m.profiles[0] = image_at_125();
    let mut r = Rig::connected(m);
    assert_eq!(r.settings().polling_hz, Some(125));
    let (req, _) = r.mark();
    r.w.factory_reset().unwrap();
    assert!(r.mouse.lock().commands_since(req).contains(&cmd::FACTORY_RESET));
    assert_eq!(r.state().settings, Some(image::decode(&sample_image())));
}

#[test]
fn long_range_is_set_then_read_back() {
    let mut r = Rig::connected(SimMouse::default());
    r.w.set_long_range(true).unwrap();
    assert_eq!(r.state().long_range, Some(true));
    assert!(r.mouse.lock().long_range);
}

#[test]
fn goodbye_says_no_app_is_listening() {
    let mut r = Rig::connected(SimMouse::default());
    let (req, _) = r.mark();
    r.w.goodbye();
    let m = r.mouse.lock();
    assert_eq!(m.commands_since(req), vec![cmd::PC_DRIVER_STATUS]);
    assert_eq!(m.requests[req][6], 0);
}

#[test]
fn the_engine_finds_the_mouse_and_runs_calls_on_its_thread() {
    let mouse = Arc::new(Mutex::new(SimMouse::default()));
    let notify: Notify = Arc::new(|_, _| {});
    let engine = Engine::start(Box::new(SimFinder(mouse.clone())), notify, None);
    let start = Instant::now();
    while engine.snapshot().settings.is_none() {
        assert!(start.elapsed() < Duration::from_secs(5), "never connected");
        thread::sleep(Duration::from_millis(10));
    }
    engine.call(|w| w.set_long_range(true)).unwrap();
    assert!(mouse.lock().long_range);
    assert_eq!(engine.snapshot().long_range, Some(true));
}

#[test]
fn applying_only_some_settings_leaves_the_rest_as_they_are() {
    // The battery saver sends only the rate.
    let mut r = Rig::connected(SimMouse::default());
    let before = r.settings();
    let (req, _) = r.mark();
    let applied = r.w.apply_settings(&Settings { polling_hz: Some(125), ..Default::default() }).unwrap();
    assert_eq!(applied, Settings { polling_hz: Some(125), ..before });
    assert!(r.mouse.lock().writes_since(req).iter().all(|a| (0x00..0x02).contains(a)));
}

#[test]
fn a_setting_the_mouse_lacks_is_left_out_of_what_the_ui_gets() {
    let s = Settings { polling_hz: Some(1000), ..Default::default() };
    assert_eq!(serde_json::to_value(&s).unwrap(), serde_json::json!({ "pollingHz": 1000 }));
    let back: Settings = serde_json::from_value(serde_json::json!({ "pollingHz": 1000 })).unwrap();
    assert_eq!(back, s);
}

#[test]
fn the_first_zaopin_takes_over_the_battery_files_from_before_several_mice() {
    let dir = std::env::temp_dir().join(format!("hrtz-battery-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("battery.json"), r#"{"level":64,"at":1700000000}"#).unwrap();
    std::fs::write(dir.join("battery_log.jsonl"), "").unwrap();
    let mouse = Arc::new(Mutex::new(SimMouse::default()));
    let notify: Notify = Arc::new(|_, _| {});
    let state = Arc::new(RwLock::new(DeviceState::default()));
    let log = Arc::new(Mutex::new(BatteryLog::load(None)));
    let mut w = Worker::new(Box::new(SimFinder(mouse)), state, notify, Some(dir.clone()), log.clone());
    w.housekeeping();
    let (estimate, own_log) = crate::mice::engine::battery_files(&dir, "compx/7-4");
    assert!(estimate.exists() && own_log.exists(), "moved under the mouse's key");
    assert!(!dir.join("battery.json").exists() && !dir.join("battery_log.jsonl").exists());
    assert!(!log.lock().entries().is_empty(), "and this mouse's readings go to its own log");
    std::fs::remove_dir_all(&dir).unwrap();
}
