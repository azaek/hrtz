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
fn another_makers_mouse_is_left_alone() {
    // VGN, Lamzu and others answer on Zaopin's vendor ID and interface, with their own CID.
    let r = Rig::connected(SimMouse { cid: 2, mid: 1, ..Default::default() });
    let s = r.state();
    assert!(!s.connected && s.key.is_none() && s.model.is_none() && s.settings.is_none());
    assert!(s.foreign, "kept apart from a mouse that hasn't answered yet");
    assert_eq!(r.mouse.lock().commands_since(0), [cmd::HANDSHAKE], "nothing asked after the handshake");
    assert!(r.events_since(0).is_empty());
}

#[test]
fn a_receiver_with_no_mouse_paired_is_still_taken() {
    // It answers CID 0 and MID 0; the vendor app lists it too, so a mouse can be paired to it.
    let r = Rig::connected(SimMouse { cid: 0, mid: 0, online: false, ..Default::default() });
    let s = r.state();
    assert!(s.connected && !s.foreign);
    assert_eq!(s.key.as_deref(), Some("compx/0-0"));
}

#[test]
fn a_receiver_that_answered_for_no_mouse_asks_again_once_its_mouse_wakes() {
    // A Z11's receiver answered CID 0 and MID 0 with the Z11 paired (Mayank_j, 2026-11-12).
    let mut r = Rig::connected(SimMouse { cid: 0, mid: 0, online: false, device_type: 0, ..SimMouse::z11() });
    assert_eq!(r.state().key.as_deref(), Some("compx/0-0"));
    {
        let mut m = r.mouse.lock();
        (m.cid, m.mid, m.online) = (7, 9, true);
    }
    // Awake: asked again, and let go, to be met afresh.
    r.w.housekeeping();
    r.w.housekeeping();
    let s = r.state();
    assert_eq!((s.key.as_deref(), s.model.as_deref()), (Some("compx/7-9"), Some("Z11")));
    assert_eq!(r.settings().stages.unwrap()[0].dpi, 1350);
}

#[test]
fn a_receiver_with_no_mouse_isnt_asked_again_while_none_wakes() {
    let mut r = Rig::connected(SimMouse { cid: 0, mid: 0, online: false, ..Default::default() });
    let (req, _) = r.mark();
    r.w.housekeeping();
    assert!(!r.mouse.lock().commands_since(req).contains(&cmd::HANDSHAKE));
    assert!(r.state().connected);
}

#[test]
fn a_z11_reads_and_writes_its_dpi_by_its_sensors_table() {
    let mut r = Rig::connected(SimMouse::z11());
    let s = r.state();
    assert_eq!((s.model.as_deref(), s.key.as_deref()), (Some("Z11"), Some("compx/7-9")));
    let caps = s.capabilities.unwrap();
    assert_eq!(caps.dpi.map(|d| d.max), Some(12000));
    assert!(!caps.motion_sync && caps.angle_snapping && caps.lod.is_none());
    let st = r.settings();
    let dpis: Vec<u32> = st.stages.as_ref().unwrap().iter().take(6).map(|x| x.dpi).collect();
    assert_eq!(dpis, [1350, 800, 400, 2400, 3200, 6400], "not 1,600, 950, 500, 2,850, 3,800, 7,550");
    assert!(st.motion_sync.is_none() && st.lod.is_none(), "a PAW3311 has neither");
    let mut next = st.clone();
    next.stages.as_mut().unwrap()[0].dpi = 1600;
    let (req, _) = r.mark();
    r.w.apply_settings(&next).unwrap();
    let m = r.mouse.lock();
    assert_eq!(m.writes_since(req), vec![0x0C, 0x0F], "X and Y, then the check byte");
    assert_eq!(m.image()[0x0C..0x10], [37, 37, 0, checksum(&[37, 37, 0])]);
    drop(m);
    assert_eq!(r.settings().stages.unwrap()[0].dpi, 1600);
    // Motion sync stays as the mouse holds it.
    let held = r.mouse.lock().image()[0xAB] != 0;
    let mut sync = r.settings();
    sync.motion_sync = Some(!held);
    assert!(r.w.apply_settings(&sync).is_err());
}

#[test]
fn a_zaopin_hrtz_doesnt_know_shows_no_dpi_and_tells_none() {
    let mut r = Rig::connected(SimMouse { mid: 10, ..Default::default() });
    let s = r.state();
    assert!(s.connected && s.capabilities.as_ref().unwrap().dpi.is_none());
    let st = s.settings.unwrap();
    assert!(st.stages.is_none() && st.dpi_stage.is_none() && st.dpi_light.is_none());
    assert!(st.motion_sync.is_none() && st.angle_snapping.is_none() && st.sensor_mode.is_none());
    assert!(st.polling_hz.is_some() && st.debounce_ms.is_some() && st.buttons.is_some());
    // Its DPI button isn't told as a DPI.
    let (_, seen) = r.mark();
    r.mouse.lock().status(status::DPI);
    r.w.drain_unsolicited();
    assert!(!r.events_since(seen).iter().any(|e| matches!(e, DeviceEvent::Dpi { .. })));
    // What it offers is written.
    let mut next = st.clone();
    next.debounce_ms = Some(2);
    let (req, _) = r.mark();
    r.w.apply_settings(&next).unwrap();
    assert_eq!(r.mouse.lock().writes_since(req), vec![0xA9]);
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
fn the_first_zaopin_gets_a_copy_of_the_battery_files_from_before_several_mice() {
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
    assert!(estimate.exists() && own_log.exists(), "copied under the mouse's key");
    assert!(dir.join("battery.json").exists(), "and left in place for an older hrtz");
    assert!(!log.lock().entries().is_empty(), "and this mouse's readings go to its own log");
    std::fs::remove_dir_all(&dir).unwrap();
}

// ---- ATK ------------------------------------------------------------------------------------

mod atk_mice {
    use super::*;
    use crate::mice::compx::atk::{self, Method};
    use crate::mice::compx::driver::Maker;
    use crate::mice::compx::sim::atk_sample_image;
    use crate::mice::compx::{atk_catalog, atk_dpi};
    use crate::mice::model::{DpiLight, LightEffect, Lod};

    #[test]
    fn a_u2_plus_is_met_by_asking_who_it_is_and_read_as_atks_app_reads_it() {
        let r = Rig::connected(SimMouse::u2_plus());
        let s = r.state();
        assert!(s.connected && s.online && !s.foreign);
        assert_eq!(s.key.as_deref(), Some("compx/2-78"));
        assert_eq!(s.model.as_deref(), Some("U2 Plus NK (PAW3395)"));
        assert_eq!(s.brand.as_deref(), Some("ATK"));
        assert_eq!(s.firmware.as_deref(), Some("1.23"));
        assert_eq!((s.profile, s.long_range), (Some(0), None));
        let caps = s.capabilities.unwrap();
        assert!(!caps.wired);
        assert_eq!(caps.dpi.unwrap().max_stages, atk::MAX_STAGES);
        assert_eq!(caps.profiles, Some(atk::PROFILES));

        // What ATK's app offers is shown; sleep, the sensor mode and the buttons aren't.
        let st = s.settings.unwrap();
        assert_eq!(st.polling_hz, Some(1000));
        assert_eq!((st.dpi_stage_count, st.dpi_stage, st.dpi()), (Some(4), Some(1), Some(800)));
        assert_eq!(st.stages.as_ref().unwrap()[0].color, "#FF4040");
        assert_eq!(st.dpi_light, Some(DpiLight { effect: LightEffect::Steady, brightness: 5, speed: 3 }));
        assert_eq!((st.lod, st.debounce_ms), (Some(Lod::Mid), Some(2)));
        assert_eq!((st.motion_sync, st.angle_snapping, st.ripple_control), (Some(false), Some(false), Some(false)));
        assert!(
            st.sleep_s.is_none() && st.sensor_mode.is_none() && st.peak_timeout_s.is_none() && st.buttons.is_none()
        );

        // Its own battery reading, with no millivolts: its length says two bytes.
        let b = s.battery.unwrap();
        assert_eq!((b.raw_level, b.millivolts), (100, 0));

        // Linked?, how it's connected, who it is, its firmware, its profile, the settings region,
        // its battery, as ATK's app asks: no "an app is listening", no long range.
        let m = r.mouse.lock();
        let commands = m.commands_since(0);
        assert_eq!(commands[..4], [cmd::ONLINE, cmd::HANDSHAKE, atk::CID_MID, cmd::ONLINE]);
        let asked =
            [cmd::ONLINE, cmd::HANDSHAKE, atk::CID_MID, cmd::VERSION, cmd::GET_PROFILE, cmd::READ_FLASH, cmd::BATTERY];
        assert!(commands.iter().all(|c| asked.contains(c)), "{commands:02X?}");
        assert_eq!(m.reads_since(0).iter().map(|x| x.1).sum::<usize>(), p::CONFIG_REGION);
    }

    #[test]
    fn on_its_cable_it_isnt_asked_whether_its_linked() {
        let r = Rig::connected(SimMouse {
            maker: Maker::Atk { receiver: false },
            usb: (atk::VENDOR_ID, 0x110D),
            device_type: atk::link::WIRED_1K,
            ..SimMouse::u2_plus()
        });
        let s = r.state();
        let caps = s.capabilities.unwrap();
        assert!(s.connected && s.online && caps.wired);
        assert_eq!(caps.polling.unwrap().rates.last(), Some(&1000), "a 1K cable");
        assert!(!r.mouse.lock().commands_since(0).contains(&cmd::ONLINE));
    }

    #[test]
    fn asleep_on_its_receiver_its_waited_for_quietly() {
        let mut r = Rig::connected(SimMouse { online: false, ..SimMouse::u2_plus() });
        let s = r.state();
        assert!(!s.connected && !s.foreign && s.key.is_none());
        assert_eq!(r.mouse.lock().commands_since(0), [cmd::ONLINE], "who it is isn't asked while it sleeps");
        r.mouse.lock().online = true;
        r.w.housekeeping();
        assert_eq!(r.state().key.as_deref(), Some("compx/2-78"));
    }

    #[test]
    fn another_atk_is_left_alone() {
        // One of ATK's unreleased "Mouse" entries, on the same receiver: not one hrtz runs.
        let r = Rig::connected(SimMouse { cid: 1, mid: 86, ..SimMouse::u2_plus() });
        let s = r.state();
        assert!(s.foreign && !s.connected && s.key.is_none());
        assert_eq!(r.mouse.lock().commands_since(0), [cmd::ONLINE, cmd::HANDSHAKE, atk::CID_MID]);
    }

    #[test]
    fn on_nearlink_it_polls_up_to_8k() {
        let r = Rig::connected(SimMouse::u2_plus());
        let s = r.state();
        assert_eq!(s.device_type, atk::link::NEARLINK);
        assert_eq!(s.capabilities.unwrap().polling.unwrap().rates, vec![125, 250, 500, 1000, 2000, 4000, 8000]);
    }

    #[test]
    fn a_vxe_r1_pro_under_compx_vid_is_met_as_an_atk() {
        let r = Rig::connected(SimMouse::r1_pro());
        let s = r.state();
        assert!(s.connected && !s.foreign);
        assert_eq!((s.key.as_deref(), s.brand.as_deref()), (Some("compx/2-7"), Some("VXE")));
        assert_eq!(s.model.as_deref(), Some("R1 PRO (PAW3395)"));
        // Its 8K receiver, from the handshake hrtz asked as it would a Zaopin.
        let caps = s.capabilities.unwrap();
        assert!(!caps.wired && caps.polling.unwrap().rates.last() == Some(&8000));
        // The handshake, then as an ATK: linked?, who it is. No "an app is listening".
        let commands = r.mouse.lock().commands_since(0);
        assert_eq!(commands[..3], [cmd::HANDSHAKE, cmd::ONLINE, atk::CID_MID]);
        assert!(!commands.contains(&cmd::PC_DRIVER_STATUS), "{commands:02X?}");
    }

    #[test]
    fn under_compx_vid_on_a_product_id_atk_doesnt_list_its_left_alone() {
        let r = Rig::connected(SimMouse { usb: (p::VENDOR_ID, 0xF5D5), ..SimMouse::r1_pro() });
        let s = r.state();
        assert!(s.foreign && !s.connected && s.key.is_none());
        assert_eq!(r.mouse.lock().commands_since(0), [cmd::HANDSHAKE]);
    }

    #[test]
    fn a_model_without_a_dpi_light_keeps_its_colours_and_light() {
        // The A9 Plus NK: no DPI light in ATK's app.
        let mut r = Rig::connected(SimMouse { mid: 83, ..SimMouse::u2_plus() });
        assert_eq!(r.state().model.as_deref(), Some("A9 Plus NK"));
        let mut next = r.settings();
        assert!(next.dpi_light.is_none());
        let stage = &mut next.stages.as_mut().unwrap()[0];
        stage.color = "#123456".into();
        stage.dpi = 450;
        let (req, _) = r.mark();
        r.w.apply_settings(&next).unwrap();
        assert_eq!(r.mouse.lock().writes_since(req), vec![0x0C], "the DPI only");
    }

    #[test]
    fn a_dpi_change_is_written_unechoed_then_read_back() {
        let mut r = Rig::connected(SimMouse::u2_plus());
        let mut next = r.settings();
        next.stages.as_mut().unwrap()[1].dpi = 850;
        let (req, _) = r.mark();
        let applied = r.w.apply_settings(&next).unwrap();
        assert_eq!(applied.dpi(), Some(850));
        let m = r.mouse.lock();
        // Stages 1 and 2 together, as ATK's app writes them; not their colours.
        assert_eq!(m.writes_since(req), vec![0x0C], "the stage's DPI only, not its colour");
        assert_eq!(m.reads_since(req), vec![(0x0C, 8)], "read back");
        let (x, ex) = image::dpi_encode(850);
        assert_eq!(m.image()[0x10..0x14], [x, x, ex, checksum(&[x, x, ex])]);
    }

    #[test]
    fn an_echoed_write_is_taken_out_of_the_way() {
        let mut r = Rig::connected(SimMouse { echo_writes: true, ..SimMouse::u2_plus() });
        let mut next = r.settings();
        next.polling_hz = Some(500);
        assert_eq!(r.w.apply_settings(&next).unwrap().polling_hz, Some(500));
        assert!(r.mouse.lock().out.is_empty(), "the echo was read");
    }

    #[test]
    fn a_write_the_mouse_didnt_keep_says_so() {
        let mut r = Rig::connected(SimMouse { forget_writes: true, ..SimMouse::u2_plus() });
        let mut next = r.settings();
        next.dpi_stage = Some(2);
        let e = r.w.apply_settings(&next).unwrap_err().to_string();
        assert!(e.contains("didn't keep"), "{e}");
    }

    #[test]
    fn what_isnt_offered_is_left_as_it_is() {
        let mut r = Rig::connected(SimMouse::u2_plus());
        let before = r.mouse.lock().image().to_vec();
        let mut next = image::decode(&before);
        // What a Zaopin has and an ATK isn't offered, changed: sleep, the sensor mode, peak
        // performance, a button. Only the polling rate, offered, is written.
        next.polling_hz = Some(250);
        next.sleep_s = Some(60);
        next.sensor_mode = Some(crate::mice::model::SensorMode::Hp);
        next.peak_timeout_enabled = Some(true);
        next.buttons.as_mut().unwrap()[0] = crate::mice::model::ButtonAction::Disabled;
        let (req, _) = r.mark();
        r.w.apply_settings(&next).unwrap();
        assert_eq!(r.mouse.lock().writes_since(req), vec![0x00]);
    }

    #[test]
    fn the_light_takes_atks_steps_written_whole() {
        let mut r = Rig::connected(SimMouse::u2_plus());
        let mut next = r.settings();
        next.dpi_light = Some(DpiLight { effect: LightEffect::Steady, brightness: 10, speed: 3 });
        let (req, _) = r.mark();
        r.w.apply_settings(&next).unwrap();
        let m = r.mouse.lock();
        assert_eq!(m.writes_since(req), vec![0x4C], "the light's four pairs in one packet");
        assert_eq!(m.reads_since(req), vec![(0x4C, 8)]);
        assert_eq!(m.image()[0x4E..0x50], [255, checksum(&[255])]);
        drop(m);
        // A step ATK's app doesn't offer is refused, and nothing is written.
        let (req, _) = r.mark();
        let mut next = r.settings();
        next.dpi_light = Some(DpiLight { effect: LightEffect::Steady, brightness: 3, speed: 3 });
        assert!(r.w.apply_settings(&next).is_err());
        next.dpi_light = Some(DpiLight { effect: LightEffect::Breathing, brightness: 10, speed: 2 });
        assert!(r.w.apply_settings(&next).is_err());
        assert!(r.mouse.lock().writes_since(req).is_empty());
    }

    #[test]
    fn debounce_writes_its_row_and_keeps_the_sleep_time() {
        let mut r = Rig::connected(SimMouse::u2_plus());
        let sleep = r.mouse.lock().image()[0xAD..0xAF].to_vec();
        let mut next = r.settings();
        next.debounce_ms = Some(4);
        next.motion_sync = Some(true);
        let (req, _) = r.mark();
        r.w.apply_settings(&next).unwrap();
        let m = r.mouse.lock();
        assert_eq!(m.writes_since(req), vec![0xA9], "the row from debounce to ripple, once");
        assert_eq!(m.image()[0xA9..0xAD], [4, checksum(&[4]), 1, checksum(&[1])]);
        assert_eq!(m.image()[0xAD..0xAF], sleep[..], "ATK's sleep time, as it was");
        drop(m);
        let mut next = r.settings();
        next.debounce_ms = Some(3);
        assert!(r.w.apply_settings(&next).unwrap_err().to_string().contains("3 ms"));
    }

    #[test]
    fn lift_off_and_a_stage_colour_each_write_their_own_field() {
        let mut r = Rig::connected(SimMouse::u2_plus());
        let mut next = r.settings();
        next.lod = Some(Lod::High);
        next.stages.as_mut().unwrap()[1].color = "#123456".into();
        let (req, _) = r.mark();
        let applied = r.w.apply_settings(&next).unwrap();
        assert_eq!(applied.lod, Some(Lod::High));
        assert_eq!(applied.stages.unwrap()[1].color, "#123456");
        // The colours two stages at a time, as ATK's app writes them.
        assert_eq!(r.mouse.lock().writes_since(req), vec![0x0A, 0x2C]);
        // 0.7 mm is a PAW3950's.
        let mut next = r.settings();
        next.lod = Some(Lod::Low);
        let (req, _) = r.mark();
        assert!(r.w.apply_settings(&next).unwrap_err().to_string().contains("lift-off"));
        assert!(r.mouse.lock().writes_since(req).is_empty());
    }

    #[test]
    fn six_stages_at_most_and_dpi_from_100() {
        let mut r = Rig::connected(SimMouse::u2_plus());
        let mut next = r.settings();
        next.dpi_stage_count = Some(7);
        assert!(r.w.apply_settings(&next).unwrap_err().to_string().contains("out of range"));
        next.dpi_stage_count = Some(6);
        r.w.apply_settings(&next).unwrap();
        let mut next = r.settings();
        next.stages.as_mut().unwrap()[0].dpi = 50;
        let e = r.w.apply_settings(&next).unwrap_err().to_string();
        assert!(e.contains("between 100 and 30000"), "{e}");
    }

    #[test]
    fn the_dpi_button_shows_within_a_second() {
        let mut r = Rig::connected(SimMouse::u2_plus());
        let (req, seen) = r.mark();
        r.mouse.lock().put_pair(0x04, 2);
        // Not asked again before a second has gone by.
        r.w.drain_unsolicited();
        assert!(r.mouse.lock().reads_since(req).is_empty());
        thread::sleep(Duration::from_millis(1050));
        r.w.drain_unsolicited();
        assert_eq!(r.mouse.lock().reads_since(req), vec![(0x00, 6)]);
        assert_eq!(r.settings().dpi_stage, Some(2));
        let events = r.events_since(seen);
        assert!(matches!(events[..], [DeviceEvent::Dpi { stage: 2, dpi: 1600 }]), "{events:?}");
        // Nothing new: nothing told.
        thread::sleep(Duration::from_millis(1050));
        r.w.drain_unsolicited();
        assert_eq!(r.events_since(seen).len(), 1);
    }

    #[test]
    fn what_a_zaopin_only_does_is_refused() {
        let mut r = Rig::connected(SimMouse::u2_plus());
        let (req, _) = r.mark();
        assert!(r.w.set_long_range(true).is_err());
        assert!(r.w.pair().is_err());
        assert!(r.w.factory_reset().is_err());
        assert!(r.w.restore_image(&atk_sample_image()).is_err());
        assert!(r.mouse.lock().writes_since(req).is_empty());
    }

    /// Four profiles, the second at 500 Hz.
    fn four_profiles() -> SimMouse {
        let mut m = SimMouse::u2_plus();
        let f = &mut m.profiles[1];
        f[0x00] = 0x02;
        f[0x01] = checksum(&[0x02]);
        m
    }

    #[test]
    fn a_profile_is_switched_then_asked_and_its_settings_read() {
        let mut r = Rig::connected(four_profiles());
        let (req, seen) = r.mark();
        r.w.set_profile(1).unwrap();
        assert_eq!(r.state().profile, Some(1));
        assert_eq!(r.settings().polling_hz, Some(500));
        let m = r.mouse.lock();
        assert_eq!(m.active, 1);
        // Unanswered, then asked (after the engine's "is it awake?"); the new profile's settings,
        // not its slots.
        let c = m.commands_since(req);
        let at = c.iter().position(|x| *x == cmd::SET_PROFILE).expect("switched");
        assert_eq!(c[at..at + 2], [cmd::SET_PROFILE, cmd::GET_PROFILE], "{c:02X?}");
        assert_eq!(m.reads_since(req).iter().map(|x| x.1).sum::<usize>(), p::CONFIG_REGION);
        drop(m);
        assert!(matches!(r.events_since(seen)[..], [DeviceEvent::Profile { index: 1 }]));
        assert!(r.w.set_profile(4).is_err(), "four at most");
    }

    #[test]
    fn an_echoed_switch_is_taken_out_of_the_way() {
        let mut r = Rig::connected(SimMouse { echo_writes: true, ..four_profiles() });
        r.w.set_profile(2).unwrap();
        assert_eq!(r.state().profile, Some(2));
        assert!(r.mouse.lock().out.is_empty(), "the echo was read");
    }

    #[test]
    fn a_switch_the_mouse_didnt_make_says_so() {
        let mut r = Rig::connected(SimMouse { forget_writes: true, ..four_profiles() });
        let e = r.w.set_profile(1).unwrap_err().to_string();
        assert!(e.contains("didn't switch to profile 2"), "{e}");
        assert_eq!(r.state().profile, Some(0));
    }

    #[test]
    fn a_switch_on_the_mouse_shows_within_a_second() {
        let mut r = Rig::connected(four_profiles());
        let (_, seen) = r.mark();
        r.mouse.lock().active = 1;
        r.w.drain_unsolicited();
        assert_eq!(r.state().profile, Some(0), "not asked before a second has gone by");
        thread::sleep(Duration::from_millis(1050));
        r.w.drain_unsolicited();
        assert_eq!(r.state().profile, Some(1));
        assert_eq!(r.settings().polling_hz, Some(500));
        let events = r.events_since(seen);
        assert!(events.iter().any(|e| matches!(e, DeviceEvent::Profile { index: 1 })), "{events:?}");
    }

    /// `m` with its stages (400, 800, 1,600, 3,200, 6,400, 9,600) stored as `method` stores them,
    /// in every profile.
    fn stored_as(mut m: SimMouse, method: Method) -> SimMouse {
        for f in m.profiles.iter_mut() {
            f.resize(0x1B30, 0xFF);
            for (i, dpi) in [400u32, 800, 1600, 3200, 6400, 9600].into_iter().enumerate() {
                let a = atk_dpi::stage_addr(method, i);
                let s = atk_dpi::encode_stage(method, dpi).unwrap();
                f[a..a + s.len()].copy_from_slice(&s);
            }
        }
        m
    }

    #[test]
    fn a_paw3950_ultra_reads_and_writes_its_stages_its_way() {
        // The U2 Plus NK with a PAW3950 Ultra.
        let mut r = Rig::connected(stored_as(SimMouse { mid: 124, ..SimMouse::u2_plus() }, Method::Ultra));
        let s = r.settings();
        let dpis: Vec<u32> = s.stages.as_ref().unwrap().iter().take(4).map(|x| x.dpi).collect();
        assert_eq!((dpis, s.dpi()), (vec![400, 800, 1600, 3200], Some(800)));
        let mut next = s.clone();
        next.stages.as_mut().unwrap()[1].dpi = 26000;
        let (req, _) = r.mark();
        r.w.apply_settings(&next).unwrap();
        let m = r.mouse.lock();
        assert_eq!(m.writes_since(req), vec![0x0C], "stages 1 and 2 together");
        assert_eq!(m.image()[0x10..0x14], atk_dpi::encode_stage(Method::Ultra, 26000).unwrap()[..]);
        assert_eq!(m.image()[0x0C..0x10], atk_dpi::encode_stage(Method::Ultra, 400).unwrap()[..], "as it was");
        drop(m);
        assert_eq!(r.settings().dpi(), Some(26000));
    }

    #[test]
    fn a_paw3955_keeps_its_stages_from_0x1b00() {
        let p3955 = atk_catalog::MODELS.iter().find(|m| m.sensor == atk::Sensor::Paw3955).unwrap();
        let mouse = SimMouse { cid: p3955.cid, mid: p3955.mid, ..SimMouse::u2_plus() };
        let mut r = Rig::connected(stored_as(mouse, Method::Paw3955));
        assert_eq!(r.settings().dpi(), Some(800));
        let mut next = r.settings();
        next.stages.as_mut().unwrap()[2].dpi = 35000;
        let (req, _) = r.mark();
        r.w.apply_settings(&next).unwrap();
        let m = r.mouse.lock();
        assert_eq!(m.writes_since(req), vec![0x1B0C], "its third stage's row, alone");
        assert_eq!(m.image()[0x1B0C..0x1B12], atk_dpi::encode_stage(Method::Paw3955, 35000).unwrap()[..]);
    }

    #[test]
    fn where_its_dpi_isnt_settled_only_the_rest_is_shown_and_written() {
        // The VXE R1S+, with a PAW3311.
        let mut r = Rig::connected(SimMouse { mid: 63, ..SimMouse::u2_plus() });
        let s = r.state();
        assert!(s.connected && s.capabilities.as_ref().unwrap().dpi.is_none());
        let st = s.settings.unwrap();
        assert!(st.stages.is_none() && st.dpi_stage.is_none() && st.lod.is_none() && st.dpi_light.is_none());
        let mut next = st.clone();
        next.polling_hz = Some(500);
        next.debounce_ms = Some(4);
        let (req, seen) = r.mark();
        r.w.apply_settings(&next).unwrap();
        assert_eq!(r.mouse.lock().writes_since(req), vec![0x00, 0xA9]);
        // Its DPI button isn't told as a DPI.
        r.mouse.lock().status(p::status::DPI);
        r.w.drain_unsolicited();
        assert!(!r.events_since(seen).iter().any(|e| matches!(e, DeviceEvent::Dpi { .. })));
    }

    #[test]
    fn on_the_r1_ses_receiver_the_polling_rate_isnt_offered() {
        // A VXE R1SE+ (PAW3395 SE) on its 1K receiver, which keeps the rate elsewhere.
        let se = atk_catalog::MODELS.iter().find(|m| m.sensor == atk::Sensor::Paw3395Se).unwrap();
        let mouse = SimMouse {
            cid: se.cid,
            mid: se.mid,
            usb: (atk::VENDOR_ID, atk::R1_SE_RECEIVER),
            device_type: atk::link::DONGLE_1K,
            ..SimMouse::u2_plus()
        };
        let mut r = Rig::connected(stored_as(mouse, Method::Table));
        assert!(r.state().capabilities.unwrap().polling.is_none());
        let mut next = r.settings();
        next.polling_hz = Some(500);
        assert!(r.w.apply_settings(&next).is_err(), "not written");
        // Its DPI is offered.
        assert_eq!(r.settings().dpi(), Some(800));
    }

    #[test]
    fn a_dpi_its_method_cant_hold_is_refused() {
        let mut r = Rig::connected(stored_as(SimMouse { mid: 124, ..SimMouse::u2_plus() }, Method::Ultra));
        let mut next = r.settings();
        next.stages.as_mut().unwrap()[0].dpi = 30050;
        let e = r.w.apply_settings(&next).unwrap_err().to_string();
        assert!(e.contains("between 50 and 30000"), "{e}");
    }

    #[test]
    fn a_test_report_reads_its_memory_and_says_whose_it_is() {
        let mut r = Rig::connected(SimMouse::u2_plus());
        let report = r.w.report().unwrap();
        assert_eq!(report["memorySize"], p::FLASH_SIZE);
        assert_eq!(report["family"], serde_json::json!({ "maker": "ATK", "cid": 2, "mid": 78, "receiver": true }));
    }
}
