// The protocol and profile tests are ported from OpenMouse's mouse-protocol
// (https://github.com/OpenMouse-Project/mouse-protocol), commit
// c3b1bc20866fbef3a27b37ad020887607d6dbbf2: src/drivers/logitech/hidpp.test.ts and
// onboard-profiles.test.ts. Translated to Rust and modified for hrtz, 2026-10-18. Copyright (C) the
// OpenMouse contributors. AGPL-3.0-or-later. The driver tests below them are hrtz's own.

//! HID++ and the onboard profiles against bytes read from real mice, then the Logitech driver and
//! the engine against a simulated Superlight (`sim.rs`).

use std::sync::Arc;

use parking_lot::{Mutex, RwLock};

use super::fixtures::{self as fx, hex};
use super::onboard::{self as ob, Plan, Stage};
use super::protocol::{self as p, Charge, Refusal, feature};
use super::sim::{Battery, SimFinder, SimMouse};
use crate::battery_log::BatteryLog;
use crate::mice::model::{DpiStage, Settings};
use crate::mice::{DeviceEvent, DeviceState, Notify, Worker};

// ---- the wire ---------------------------------------------------------------------------------

#[test]
fn requests_carry_a_nonzero_software_id_in_the_low_nibble() {
    assert_eq!(p::with_software_id(0x00), 0x05);
    assert_eq!(p::with_software_id(0x10), 0x15);
    assert_eq!(p::with_software_id(0x1E), 0x15);
    assert_eq!(p::request(0x01, 0x07, 0x50, &[0, 1, 0, 0x10], false), {
        let mut r = vec![0x11, 0x01, 0x07, 0x55, 0, 1, 0, 0x10];
        r.resize(20, 0);
        r
    });
    assert_eq!(p::request(0xFF, 0x00, 0x00, &[0x00, 0x03], false), vec![0x10, 0xFF, 0x00, 0x05, 0x00, 0x03, 0x00]);
    assert_eq!(p::request(0xFF, 0x00, 0x00, &[], true).len(), p::LONG_LEN);
}

#[test]
fn receivers_try_every_slot_before_the_mouse_itself() {
    assert_eq!(p::index_candidates(true), &[1, 2, 3, 4, 5, 6, 0xFF]);
    assert_eq!(p::index_candidates(false), &[0xFF, 1]);
}

#[test]
fn report_rate_lists_decode() {
    // The G402 generation's 0x8060 list: 1, 2, 4 and 8 ms.
    assert_eq!(p::rate_bitmap(0x8B), vec![125, 250, 500, 1000]);
    // Bit i is (i + 1) ms, not 1 << i.
    assert_eq!(p::rate_bitmap(0x01), vec![1000]);
    assert_eq!(p::rate_bitmap(0x02), vec![500]);
    assert_eq!(p::rate_mask(0x007F), vec![125, 250, 500, 1000, 2000, 4000, 8000]);
    assert_eq!(p::rate_mask(0x000F), vec![125, 250, 500, 1000]);
}

#[test]
fn refusals_are_read_by_protocol_and_matched_to_their_request() {
    let v20 = [0x11, 0x01, 0xFF, 0x07, 0x55, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    assert_eq!(p::refusal_for(&v20, 0x01, 0x07, 0x50), Some(Refusal::V20(0x02)));
    assert_eq!(Refusal::V20(0x02).to_string(), "The mouse rejected that (invalid argument).");
    // Another request's refusal, or another slot's, isn't ours.
    assert_eq!(p::refusal_for(&v20, 0x01, 0x07, 0x60), None);
    assert_eq!(p::refusal_for(&v20, 0x02, 0x07, 0x50), None);
    let v10 = [0x10, 0x02, 0x8F, 0x00, 0x05, 0x08, 0x00];
    assert_eq!(p::refusal_for(&v10, 0x02, 0x00, 0x00), Some(Refusal::V10(0x08)));
    assert_eq!(Refusal::V10(0x0A).to_string(), "The mouse rejected that (request unavailable).");
    assert_eq!(Refusal::V20(0x42).to_string(), "The mouse rejected that (HID++ 2.0 error 0x42).");
    assert!(Refusal::V10(0x09).unreachable() && Refusal::V10(0x04).unreachable());
    assert!(!Refusal::V10(0x08).unreachable() && !Refusal::V20(0x09).unreachable());
}

#[test]
fn someone_elses_answers_are_not_ours() {
    let ours = [0x11, 0x01, 0x07, 0x55, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let ghub = [0x11, 0x01, 0x07, 0x5A, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    let event = [0x11, 0x01, 0x07, 0x00, 1, 2, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    assert!(p::answers(&ours, 0x01, 0x07, 0x50));
    assert!(!p::answers(&ghub, 0x01, 0x07, 0x50) && !p::is_event(&ghub, 0x01));
    assert!(p::is_event(&event, 0x01) && !p::is_event(&event, 0x02));
    // The receiver's own notice of a link change isn't a feature's event.
    assert!(!p::is_event(&[0x10, 0x01, 0x41, 0x04, 0x61, 0x93, 0x40], 0x01));
    // Nor is a cut-off report.
    assert!(!p::answers(&ours[..10], 0x01, 0x07, 0x50));
}

#[test]
fn the_two_battery_features_number_their_states_differently() {
    assert_eq!(p::unified_battery_state(2), Charge::ChargingSlowly);
    assert_eq!(p::battery_level_state(2), Charge::AlmostFull);
    assert_eq!(p::unified_battery_state(4), Charge::Unknown, "0x1004's 4 is a charging fault");
    assert_eq!(p::battery_level_state(4), Charge::ChargingSlowly);
    assert!(p::unified_battery_state(3).charging() && !p::unified_battery_state(0).charging());
}

#[test]
fn dpi_lists_expand_their_ranges() {
    assert_eq!(p::dpi_list(&hex("00 64 e0 32 01 f4 00 00")), Some(vec![100, 150, 200, 250, 300, 350, 400, 450, 500]));
    assert_eq!(p::dpi_list(&hex("00 64 00 c8 01 90 00 00")), Some(vec![100, 200, 400]));
    assert_eq!(p::dpi_list(&hex("e0 32 01 f4")), None, "a range needs a value before it");
    assert_eq!(p::dpi_list(&[]), Some(vec![]));
}

#[test]
fn a_voltage_reads_as_a_percentage_on_the_curve() {
    assert_eq!(p::voltage_percent(4200), 100);
    assert_eq!(p::voltage_percent(3989), 80);
    assert_eq!(p::voltage_percent(3950), 74);
    assert_eq!(p::voltage_percent(3500), 0);
}

// ---- onboard profiles -------------------------------------------------------------------------

#[test]
fn reads_get_info() {
    assert_eq!(
        ob::Info::parse(&fx::superlight2_info()),
        ob::Info {
            memory_model: 1,
            format: 7,
            macro_format: 1,
            profile_count: 5,
            button_count: 5,
            sector_count: 16,
            sector_size: 255
        }
    );
    let sl = ob::Info::parse(&fx::superlight_info());
    assert_eq!((sl.format, sl.profile_count, sl.sector_count, sl.sector_len()), (4, 5, 16, 255));
    let g102 = ob::Info::parse(&fx::g102_info());
    assert_eq!((g102.format, g102.profile_count, g102.button_count), (4, 1, 6));
    assert_eq!(ob::Info::parse(&[]).sector_len(), 255, "a nonsense size falls back to 255");
}

#[test]
fn reads_the_profile_directory() {
    let entries = ob::directory(&fx::superlight2_directory());
    assert_eq!(
        entries.iter().map(|e| (e.sector, e.enabled)).collect::<Vec<_>>(),
        [(1, false), (2, false), (3, true), (4, false), (5, false)]
    );
    assert_eq!(ob::directory(&fx::g102_directory()).len(), 1);
    assert!(ob::directory(&[0xFF; 64]).is_empty());
}

#[test]
fn the_crc_matches_every_sector_read_from_a_mouse() {
    for (name, sector, crc) in [
        ("Superlight 2, set up", fx::superlight2_sector_3(), 0x20C7),
        ("Superlight 2, factory", fx::superlight2_factory(), 0x84DB),
        ("Superlight", fx::superlight_sector_1(), 0xDD75),
        ("G102", fx::g102_profile(), 0x6827),
        ("G102 directory", fx::g102_directory(), 0x587C),
        ("Superstrike", fx::superstrike_sector_1(), 0x2A38),
    ] {
        assert_eq!(sector.len(), 255, "{name}");
        assert_eq!(ob::stored_crc(&sector), crc, "{name}");
        assert_eq!(ob::crc(&sector), crc, "{name}");
    }
    let mut broken = fx::superlight2_sector_3();
    broken[0x10] ^= 1;
    assert!(!ob::crc_ok(&broken));
}

/// G HUB changed sector 3 one setting at a time; only the first state was read in full, so each
/// matching checksum confirms the CRC on its own.
#[test]
fn the_crc_reproduces_what_g_hub_wrote() {
    for (rate, bunny_hop, crc) in
        [(0x03, 0x00, 0x20C7), (0x06, 0x00, 0x6E7C), (0x06, 0x0A, 0xD92C), (0x06, 0x14, 0x10FD)]
    {
        let mut s = fx::superlight2_sector_3();
        s[0x00] = rate;
        s[0x25] = bunny_hop;
        ob::apply_crc(&mut s);
        assert_eq!(ob::stored_crc(&s), crc, "rate {rate:#x}, bunny hop {bunny_hop:#x}");
    }
}

fn stages(list: &[(u32, u8)]) -> Vec<Stage> {
    list.iter().map(|(dpi, lod)| Stage { x: *dpi, y: *dpi, lod: *lod }).collect()
}

#[test]
fn decodes_profiles_of_both_layouts() {
    let set_up = ob::decode(&fx::superlight2_sector_3(), 7);
    assert_eq!(set_up.stages, stages(&[(1600, 2)]));
    assert_eq!((set_up.rate_wireless, set_up.rate_wired), (Some(1000), Some(1000)));

    // Little-endian DPI: 20 03 is 800, not 8,195.
    let factory = ob::decode(&fx::superlight2_factory(), 7);
    assert_eq!(factory.stages, stages(&[(800, 2), (1200, 2), (1600, 2), (2400, 2), (3200, 2)]));
    assert_eq!(factory.default_index, Some(0));

    // The first Superlight reports format 4: one DPI per stage, no lift-off, one interval.
    let sl = ob::decode(&fx::superlight_sector_1(), 4);
    assert_eq!(sl.stages, stages(&[(800, 0)]));
    assert_eq!((sl.default_index, sl.rate_wireless, sl.rate_wired), (Some(0), Some(1000), Some(1000)));

    let g102 = ob::decode(&fx::g102_profile(), 4);
    assert_eq!(g102.stages, stages(&[(50, 0), (2000, 0), (1600, 0)]));
    assert_eq!(g102.default_index, Some(1));

    let superstrike = ob::decode(&fx::superstrike_sector_1(), 8);
    assert_eq!(superstrike.stages, stages(&[(800, 2), (1200, 2), (1600, 2), (2400, 2), (3200, 2)]));
}

const FIVE: [(u32, u8); 5] = [(800, 1), (1200, 2), (1600, 2), (2400, 2), (3200, 3)];

fn encode(sector: &[u8], format: u8, list: &[(u32, u8)], default_index: u8) -> Vec<u8> {
    let plan = Plan { stages: stages(list), default_index };
    ob::encode_stages(sector, format, &plan, ob::dpi_limits(format), true).unwrap()
}

#[test]
fn writing_the_same_stages_back_changes_nothing() {
    let sector = fx::superlight2_sector_3();
    let d = ob::decode(&sector, 7);
    let plan = Plan { stages: d.stages, default_index: d.default_index.unwrap() };
    assert_eq!(ob::encode_stages(&sector, 7, &plan, ob::dpi_limits(7), true).unwrap(), sector);
}

#[test]
fn every_stage_round_trips_and_dropped_ones_are_zeroed() {
    let five = encode(&fx::superlight2_sector_3(), 7, &FIVE, 2);
    let d = ob::decode(&five, 7);
    assert_eq!((d.stages, d.default_index), (stages(&FIVE), Some(2)));
    assert!(ob::crc_ok(&five));

    let two = encode(&five, 7, &FIVE[..2], 0);
    for stage in 2..5 {
        let base = 0x02 + 2 + stage * 5;
        assert_eq!(two[base..base + 5], [0; 5], "stage {}", stage + 1);
    }
    assert_eq!(ob::decode(&two, 7).stages, stages(&FIVE[..2]));
    assert!(ob::crc_ok(&two));
}

#[test]
fn fewer_stages_pull_the_g_shift_stage_back_into_range() {
    let mut shifted = encode(&fx::superlight2_sector_3(), 7, &FIVE, 0);
    shifted[0x03] = 4;
    assert_eq!(encode(&shifted, 7, &FIVE[..1], 0)[0x03], 0);
    assert_eq!(encode(&shifted, 7, &FIVE, 0)[0x03], 4, "still in range, left alone");
}

#[test]
fn format_4_stages_are_one_dpi_each() {
    let profile = fx::g102_profile();
    let mut list: Vec<(u32, u8)> = ob::decode(&profile, 4).stages.iter().map(|s| (s.x, 0)).collect();
    list[1].0 = 1000;
    let out = encode(&profile, 4, &list, 1);
    assert_eq!(ob::decode(&out, 4).stages[1].x, 1000);
    assert!(ob::crc_ok(&out));
    // A separate Y or a lift-off level has nowhere to go.
    let plan = Plan { stages: vec![Stage { x: 800, y: 1600, lod: 0 }], default_index: 0 };
    assert!(ob::encode_stages(&profile, 4, &plan, ob::dpi_limits(4), true).is_err());
    let plan = Plan { stages: vec![Stage { x: 800, y: 800, lod: 2 }], default_index: 0 };
    assert!(ob::encode_stages(&profile, 4, &plan, ob::dpi_limits(4), true).is_err());
}

#[test]
fn a_stage_keeps_its_lift_off_byte_when_that_byte_is_not_written() {
    let mut sector = vec![0xFF; 255];
    sector[0x02 + 2 + 4] = 0x02;
    let plan = Plan { stages: stages(&[(800, 1)]), default_index: 0 };
    let out = ob::encode_stages(&sector, 7, &plan, ob::dpi_limits(7), false).unwrap();
    assert_eq!((out[0x04], out[0x08]), (0x20, 0x02));
}

#[test]
fn stage_plans_are_held_to_the_format() {
    let limits = ob::dpi_limits(7);
    let check = |list: &[(u32, u8)], default_index: u8| {
        ob::check_plan(&Plan { stages: stages(list), default_index }, limits, 7)
    };
    assert!(check(&[(800, 1)], 0).is_ok());
    assert_eq!(check(&[], 0).unwrap_err(), "This mouse holds 1 to 5 DPI stages.");
    assert_eq!(check(&[(800, 1); 6], 0).unwrap_err(), "This mouse holds 1 to 5 DPI stages.");
    assert_eq!(check(&[(800, 1)], 1).unwrap_err(), "The stage in use must be one of the stages.");
    assert_eq!(check(&[(60, 1)], 0).unwrap_err(), "Stage 1 must be 100 to 32000 DPI.");
    assert_eq!(check(&[(825, 1)], 0).unwrap_err(), "Stage 1 must be a multiple of 50 DPI.");
    // 0 marks a stage unused, so a stage in use can't have it.
    assert_eq!(check(&[(800, 0)], 0).unwrap_err(), "Stage 1 has no lift-off level.");
    assert!(check(&[(800, 4)], 0).is_err());
    assert!(ob::check_plan(&Plan { stages: stages(&[(800, 1)]), default_index: 0 }, ob::dpi_limits(6), 6).is_err());
}

#[test]
fn limits_are_the_formats_and_the_sensor_narrows_them() {
    assert_eq!(ob::dpi_limits(7), Some(ob::DpiLimits { max_stages: 5, min: 100, max: 32000, step: 50 }));
    assert_eq!(ob::dpi_limits(4), Some(ob::DpiLimits { max_stages: 5, min: 50, max: 8000, step: 50 }));
    assert_eq!(ob::dpi_limits(6), None, "never established");
    let sensor: Vec<u32> = (100..=25600).step_by(50).collect();
    assert_eq!(
        ob::dpi_limits_for_sensor(ob::dpi_limits(4), &sensor),
        Some(ob::DpiLimits { max_stages: 5, min: 100, max: 25600, step: 50 })
    );
    assert_eq!(ob::dpi_limits_for_sensor(ob::dpi_limits(4), &[]), ob::dpi_limits(4));
    assert_eq!(ob::dpi_limits_for_sensor(None, &sensor), None);
}

#[test]
fn report_rates_are_held_per_link_and_per_format() {
    let sl2 = ob::rate_limits(7, Some("C09B"));
    assert_eq!(ob::rates_for(sl2, false), [125, 250, 500, 1000, 2000, 4000, 8000]);
    assert_eq!(ob::rates_for(sl2, true), [125, 250, 500, 1000]);
    assert_eq!(ob::rates_for(ob::rate_limits(4, None), false), [125, 250, 500, 1000]);
    assert_eq!(ob::rate_limits(6, None), None);
    // The PRO X 3 Superstrike's cable carries its full range; the PRO X 2's doesn't.
    assert_eq!(ob::rate_limits(8, Some("c0a9")).unwrap().wired_max, 8000);
    assert_eq!(ob::rate_limits(8, Some("C0A8")).unwrap().wired_max, 1000);
    // What the mouse says the link carries narrows the list.
    assert_eq!(ob::rates_offered(sl2, false, &[125, 500, 1000, 2000]), [125, 500, 1000, 2000]);
}

/// From G HUB on a Superlight 2, sector 1: wireless to 8,000 Hz (0x00: 03 -> 06), wired to 250 Hz
/// (0x01: 03 -> 01).
#[test]
fn report_rate_bytes_match_what_g_hub_writes() {
    let before = fx::superlight2_sector_3();
    let limits = ob::rate_limits(7, None);
    let wireless = ob::encode_rate(&before, 7, false, 8000, limits).unwrap();
    let after = ob::encode_rate(&wireless, 7, true, 250, limits).unwrap();
    assert_eq!((after[0x00], after[0x01]), (0x06, 0x01));
    assert_eq!(after[2..253], before[2..253], "nothing else moves");
    assert!(ob::crc_ok(&after));
    assert_eq!(
        ob::encode_rate(&before, 7, true, 8000, limits).unwrap_err(),
        "The cable carries 125, 250, 500 or 1,000 Hz."
    );
}

#[test]
fn format_4_keeps_one_interval_in_milliseconds() {
    // An index (3 for 1,000 Hz) would read back as 333 Hz.
    let out = ob::encode_rate(&fx::g102_profile(), 4, true, 500, ob::rate_limits(4, None)).unwrap();
    assert_eq!(out[0x00], 2);
    let d = ob::decode(&out, 4);
    assert_eq!((d.rate_wired, d.rate_wireless), (Some(500), Some(500)));
    let back = ob::encode_rate(&out, 4, false, 1000, ob::rate_limits(4, None)).unwrap();
    assert_eq!(back[0x00], 1);
    assert_eq!(back[1..253], out[1..253]);
}

// ---- the driver, against a simulated mouse ----------------------------------------------------

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
        let w = Worker::new(Box::new(SimFinder::new(mouse.clone())), state, notify, None, log);
        Self { w, mouse, seen }
    }

    fn connected(m: SimMouse) -> Self {
        let mut r = Self::new(m);
        r.w.housekeeping();
        r
    }

    fn state(&self) -> DeviceState {
        self.w.snapshot()
    }

    fn settings(&self) -> Settings {
        self.state().settings.expect("settings read")
    }

    fn mark(&self) -> (usize, usize) {
        (self.mouse.lock().requests.len(), self.seen.lock().len())
    }

    fn events_since(&self, from: usize) -> Vec<DeviceEvent> {
        self.seen.lock()[from..].iter().filter_map(|x| x.1.clone()).collect()
    }

    fn profile_calls_since(&self, from: usize) -> Vec<u8> {
        self.mouse.lock().calls_since(from, feature::ONBOARD_PROFILES)
    }

    fn sector(&self, s: u16) -> Vec<u8> {
        self.mouse.lock().sectors[&s].clone()
    }
}

fn dpis(s: &Settings) -> Vec<u32> {
    s.stages.as_ref().unwrap().iter().map(|x| x.dpi).collect()
}

fn with_stages(s: &Settings, list: &[u32], in_use: u8) -> Settings {
    let mut slots: Vec<DpiStage> = list.iter().map(|d| DpiStage { dpi: *d, color: "#FFFFFF".into() }).collect();
    slots.extend(s.stages.clone().unwrap().into_iter().skip(list.len()));
    Settings { stages: Some(slots), dpi_stage_count: Some(list.len() as u8), dpi_stage: Some(in_use), ..s.clone() }
}

/// What hrtz's driver read from the user's Superlight in the probe (2026-10-18), but the unit ID:
/// the simulated one answers as that mouse did.
#[test]
fn a_superlight_on_its_receiver_is_met_and_read() {
    let r = Rig::connected(SimMouse::superlight());
    let s = r.state();
    assert!(s.connected && s.online);
    assert_eq!(s.key.as_deref(), Some("hidpp/c094/1a2b3c4d"));
    assert_eq!(s.model.as_deref(), Some("PRO X SUPERLIGHT"));
    assert_eq!(s.firmware.as_deref(), Some("MPM 25.02"), "the main application, though the bootloader is listed first");
    assert_eq!(s.profile, Some(0));
    let caps = s.capabilities.clone().unwrap();
    assert!(!caps.wired && caps.battery);
    assert_eq!(caps.profiles, Some(5));
    let dpi = caps.dpi.unwrap();
    assert_eq!((dpi.min, dpi.max, dpi.step, dpi.max_stages, dpi.colors), (100, 25600, 50, 5, false));
    assert_eq!(caps.polling.unwrap().rates, [125, 250, 500, 1000]);
    assert!(!caps.backups && !caps.factory_reset && !caps.pairing && caps.lod.is_none() && caps.buttons.is_none());

    let st = r.settings();
    assert_eq!(st.polling_hz, Some(1000));
    assert_eq!((st.dpi_stage_count, st.dpi_stage), (Some(1), Some(0)));
    assert_eq!(dpis(&st), [1600, 800, 1200, 2400, 3200], "the stage in use, then the vendor's others");
    assert_eq!(st.dpi(), Some(1600));
    assert!(st.lod.is_none() && st.buttons.is_none() && st.sensor_mode.is_none());

    // Its own percentage, from 0x1004, with no voltage.
    let b = s.battery.unwrap();
    assert_eq!((b.level, b.millivolts, b.charging), (44, 0, false));
}

#[test]
fn a_battery_by_voltage_is_read_on_openmouses_curve() {
    let r = Rig::connected(SimMouse { battery: Battery::Voltage { mv: 3989, flags: 0 }, ..SimMouse::superlight() });
    let b = r.state().battery.unwrap();
    assert_eq!((b.level, b.millivolts, b.charging), (80, 3989, false), "not Zaopin's curve");
}

#[test]
fn the_users_superlight_profiles_decode() {
    let dir = fx::superlight_c547_directory();
    assert_eq!(ob::stored_crc(&dir), 0x4037);
    assert!(ob::crc_ok(&dir));
    assert_eq!(
        ob::directory(&dir).iter().map(|e| (e.sector, e.enabled)).collect::<Vec<_>>(),
        [(1, true), (2, false), (3, false), (4, false), (5, false)]
    );

    let mine = fx::superlight_c547_profile();
    assert_eq!(ob::stored_crc(&mine), 0x2330);
    assert!(ob::crc_ok(&mine));
    let d = ob::decode(&mine, 4);
    assert_eq!((d.stages, d.default_index, d.rate_wireless), (stages(&[(1600, 0)]), Some(0), Some(1000)));

    let factory = fx::superlight_c547_factory();
    assert_eq!(ob::stored_crc(&factory), 0x60E7);
    assert!(ob::crc_ok(&factory));
    let d = ob::decode(&factory, 4);
    assert_eq!(d.stages, stages(&[(400, 0), (800, 0), (1600, 0), (3200, 0), (6400, 0)]));
    assert_eq!((d.default_index, d.rate_wired), (Some(1), Some(1000)));

    // Writing its stages back as they are changes nothing.
    let plan = Plan { stages: d.stages, default_index: 1 };
    assert_eq!(
        ob::encode_stages(
            &factory,
            4,
            &plan,
            ob::dpi_limits_for_sensor(ob::dpi_limits(4), &(100..=25600).step_by(50).collect::<Vec<_>>()),
            true
        )
        .unwrap(),
        factory
    );
}

#[test]
fn on_its_cable_it_is_found_past_its_empty_0xff_and_keeps_its_key() {
    let wireless = Rig::connected(SimMouse::superlight()).state();
    let r = Rig::connected(SimMouse::superlight_cabled());
    let s = r.state();
    assert!(s.connected && s.online);
    assert!(s.capabilities.unwrap().wired);
    assert_eq!(s.key, wireless.key, "the same mouse, whichever way it's plugged in");
    assert!(r.mouse.lock().requests.iter().any(|q| q[1] == 0xFF), "0xFF was asked first");
    assert!(r.mouse.lock().requests.last().is_some_and(|q| q[1] == 0x01));
}

#[test]
fn a_superlight_2_shows_its_profile_in_use_and_its_links_rates() {
    let r = Rig::connected(SimMouse::superlight2());
    let s = r.state();
    assert_eq!(s.key.as_deref(), Some("hidpp/c09b/5e6f7081"));
    assert_eq!(s.model.as_deref(), Some("PRO X SUPERLIGHT 2"));
    assert_eq!(s.profile, Some(2), "sector 3 is the third profile");
    let caps = s.capabilities.unwrap();
    assert_eq!(caps.polling.unwrap().rates, [125, 250, 500, 1000, 2000, 4000, 8000]);
    assert_eq!((caps.dpi.as_ref().unwrap().min, caps.dpi.as_ref().unwrap().max), (100, 32000));
    let st = r.settings();
    assert_eq!((st.polling_hz, st.dpi_stage_count, st.dpi()), (Some(1000), Some(1), Some(1600)));
    let b = s.battery.unwrap();
    assert_eq!((b.level, b.millivolts), (72, 0), "its own percentage, with no voltage");
}

#[test]
fn the_mouse_is_found_past_a_keyboard_on_a_merged_receiver() {
    let r = Rig::connected(SimMouse { slot: 0x03, keyboard: Some(0x01), ..SimMouse::superlight2() });
    let s = r.state();
    assert!(s.connected && s.online && s.settings.is_some());
    assert!(r.mouse.lock().requests.iter().all(|q| q[1] != 0xFF), "found before the receiver's own index");
}

#[test]
fn a_receiver_with_only_a_keyboard_is_no_mouse() {
    let r = Rig::connected(SimMouse { slot: 0x07, keyboard: Some(0x01), ..SimMouse::superlight() });
    assert!(!r.state().connected);
}

#[test]
fn sleep_and_wake_through_the_receiver() {
    let mut r = Rig::connected(SimMouse::superlight());
    r.mouse.lock().online = false;
    r.w.housekeeping();
    let s = r.state();
    assert!(s.connected && !s.online);
    r.mouse.lock().online = true;
    r.w.housekeeping();
    assert!(r.state().online);
}

#[test]
fn a_mouse_asleep_when_hrtz_starts_is_met_once_it_wakes() {
    let mut r = Rig::new(SimMouse { online: false, ..SimMouse::superlight() });
    r.w.housekeeping();
    assert!(!r.state().connected);
    r.mouse.lock().online = true;
    r.w.housekeeping();
    assert!(r.state().connected && r.state().online);
}

#[test]
fn g_hubs_answers_on_the_same_receiver_are_passed_over() {
    let r = Rig::connected(SimMouse { chatter: true, ..SimMouse::superlight2() });
    assert_eq!(r.settings().dpi(), Some(1600));
    assert_eq!(r.state().firmware.as_deref(), Some("MPM 25.02"));
}

#[test]
fn a_new_stage_is_written_into_the_profile_in_use_and_selected() {
    let mut r = Rig::connected(SimMouse::superlight());
    let (from, _) = r.mark();
    let new = with_stages(&r.settings(), &[800, 1600], 1);
    let after = r.w.apply_settings(&new).unwrap();

    let sector = r.sector(1);
    assert!(ob::crc_ok(&sector));
    let d = ob::decode(&sector, 4);
    assert_eq!(d.stages, stages(&[(800, 0), (1600, 0)]));
    assert_eq!(d.default_index, Some(1));
    assert_eq!(sector[0x00], 0x01, "the rate is left as it was");
    // Read again, written, read back, then selected again so the mouse runs it.
    let calls = r.profile_calls_since(from);
    let write = calls.iter().position(|f| *f == ob::func::ADDR_WRITE).expect("a write");
    assert!(calls[..write].contains(&ob::func::READ));
    assert_eq!(calls.iter().filter(|f| **f == ob::func::WRITE).count(), 16, "255 bytes, 16 at a time");
    assert!(calls[write..].contains(&ob::func::WRITE_END));
    assert_eq!(calls.last(), Some(&ob::func::SET_CURRENT));

    assert_eq!((after.dpi_stage_count, after.dpi_stage, after.dpi()), (Some(2), Some(1), Some(1600)));
}

#[test]
fn settings_that_did_not_change_are_not_written() {
    let mut r = Rig::connected(SimMouse::superlight2());
    let (from, _) = r.mark();
    let same = r.settings();
    r.w.apply_settings(&same).unwrap();
    let calls = r.profile_calls_since(from);
    assert!(!calls.contains(&ob::func::ADDR_WRITE) && !calls.contains(&ob::func::SET_CURRENT));
}

#[test]
fn a_superlight_2s_rate_is_written_to_its_receivers_byte_only() {
    let mut r = Rig::connected(SimMouse::superlight2());
    let new = Settings { polling_hz: Some(8000), ..r.settings() };
    assert_eq!(r.w.apply_settings(&new).unwrap().polling_hz, Some(8000));
    let sector = r.sector(3);
    assert_eq!((sector[0x00], sector[0x01]), (0x06, 0x03), "wireless set, cable left at 1,000 Hz");
    assert_eq!(ob::stored_crc(&sector), 0x6E7C, "the checksum G HUB wrote for the same change");
}

#[test]
fn a_superlight_2s_new_stages_take_its_lift_off_and_keep_their_y() {
    let mut m = SimMouse::superlight2();
    // G HUB split stage 1's X and Y.
    let s3 = m.sectors.get_mut(&3).unwrap();
    s3[0x06..0x08].copy_from_slice(&3200u16.to_le_bytes());
    ob::apply_crc(s3);
    let mut r = Rig::connected(m);
    let new = with_stages(&r.settings(), &[1600, 3200], 0);
    r.w.apply_settings(&new).unwrap();
    let d = ob::decode(&r.sector(3), 7);
    assert_eq!(d.stages, vec![Stage { x: 1600, y: 3200, lod: 2 }, Stage { x: 3200, y: 3200, lod: 2 }]);
}

#[test]
fn what_the_link_or_sensor_cannot_do_is_refused_before_anything_is_written() {
    let mut r = Rig::connected(SimMouse::superlight());
    let (from, _) = r.mark();
    let fast = Settings { polling_hz: Some(2000), ..r.settings() };
    assert_eq!(r.w.apply_settings(&fast).unwrap_err().to_string(), "This connection doesn't carry 2,000 Hz.");
    let odd = with_stages(&r.settings(), &[125], 0);
    assert_eq!(r.w.apply_settings(&odd).unwrap_err().to_string(), "Stage 1: this mouse doesn't offer 125 DPI.");
    assert!(!r.profile_calls_since(from).contains(&ob::func::ADDR_WRITE));
    assert_eq!(r.sector(1), fx::superlight_c547_profile());
}

#[test]
fn a_profile_the_mouse_did_not_keep_is_put_back() {
    let mut r = Rig::connected(SimMouse { forget_writes: true, ..SimMouse::superlight() });
    let (from, _) = r.mark();
    let new = with_stages(&r.settings(), &[800, 1600], 0);
    let e = r.w.apply_settings(&new).unwrap_err();
    assert_eq!(e.to_string(), "The mouse didn't keep the new settings, so its profile was put back.");
    let writes = r.profile_calls_since(from).iter().filter(|f| **f == ob::func::ADDR_WRITE).count();
    assert_eq!(writes, 2, "the new profile, then the old one back");
}

#[test]
fn a_profile_whose_checksum_fails_is_not_written_over() {
    let mut r = Rig::connected(SimMouse::superlight());
    r.mouse.lock().sectors.get_mut(&1).unwrap()[0x30] ^= 0xFF;
    let (from, _) = r.mark();
    let new = with_stages(&r.settings(), &[800, 1600], 0);
    assert!(r.w.apply_settings(&new).unwrap_err().to_string().contains("checksum"));
    assert!(!r.profile_calls_since(from).contains(&ob::func::ADDR_WRITE));
}

#[test]
fn switching_profile_selects_its_sector_and_writes_nothing() {
    let mut r = Rig::connected(SimMouse::superlight2());
    let (from, seen) = r.mark();
    r.w.set_profile(0).unwrap();
    let calls = r.profile_calls_since(from);
    assert!(calls.contains(&ob::func::SET_CURRENT) && !calls.contains(&ob::func::ADDR_WRITE));
    assert_eq!(r.mouse.lock().current, 1);
    let s = r.state();
    assert_eq!(s.profile, Some(0));
    assert_eq!(dpis(&r.settings()), [800, 1200, 1600, 2400, 3200]);
    assert!(matches!(r.events_since(seen)[..], [DeviceEvent::Profile { index: 0 }]));
    assert!(r.w.set_profile(5).is_err());
}

#[test]
fn a_profile_switched_on_the_mouse_is_read_and_announced() {
    let mut r = Rig::connected(SimMouse::superlight2());
    let (_, seen) = r.mark();
    r.mouse.lock().switch_on_mouse(5);
    r.w.drain_unsolicited();
    assert_eq!(r.state().profile, Some(4));
    assert_eq!(r.settings().dpi_stage_count, Some(5));
    assert!(r.events_since(seen).iter().any(|e| matches!(e, DeviceEvent::Profile { index: 4 })));
}

#[test]
fn a_battery_event_reads_the_battery_again() {
    let mut r = Rig::connected(SimMouse::superlight2());
    r.mouse.lock().battery = Battery::Unified { percent: 71, status: 1 };
    r.mouse.lock().battery_event();
    let (from, _) = r.mark();
    r.w.drain_unsolicited();
    assert!(!r.mouse.lock().calls_since(from, feature::UNIFIED_BATTERY).is_empty());
    assert!(r.state().battery.unwrap().charging);
}

#[test]
fn an_unplugged_receiver_lets_the_mouse_go() {
    let mut r = Rig::connected(SimMouse::superlight());
    r.mouse.lock().unplugged = true;
    r.w.housekeeping();
    let s = r.state();
    assert!(!s.connected);
    assert_eq!(s.key.as_deref(), Some("hidpp/c094/1a2b3c4d"), "who it was stays known");
}

#[test]
fn a_receivers_short_and_long_collections_pair_up_by_path() {
    use super::driver::interface_of;
    let short = r"\?\HID#VID_046D&PID_C539&MI_02&Col01#8&2d9b0a4f&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
    let long = r"\?\HID#VID_046D&PID_C539&MI_02&Col02#8&2d9b0a4f&0&0001#{4d1e55b2-f16f-11cf-88cb-001111000030}";
    let other = r"\?\HID#VID_046D&PID_C539&MI_02&Col01#8&11aa22bb&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
    assert_eq!(interface_of(short), interface_of(long));
    assert_ne!(interface_of(short), interface_of(other), "another receiver of the same model");
    assert_eq!(interface_of("/dev/hidraw3"), "/dev/hidraw3");
}

#[test]
fn a_simulated_mouse_in_a_dev_build_is_never_under_a_real_key() {
    let mouse = Arc::new(Mutex::new(SimMouse::superlight()));
    let seen: Seen = Arc::default();
    let sink = seen.clone();
    let notify: Notify = Arc::new(move |s, e| sink.lock().push((s.clone(), e)));
    let state = Arc::new(RwLock::new(DeviceState::default()));
    let log = Arc::new(Mutex::new(BatteryLog::load(None)));
    let mut w = Worker::new(Box::new(SimFinder::dev(mouse)), state, notify, None, log);
    w.housekeeping();
    assert_eq!(w.snapshot().key.as_deref(), Some("sim/c094/1a2b3c4d"));
    assert!(seen.lock().iter().all(|(s, _)| s.key.as_deref().is_none_or(|k| k.starts_with("sim/"))));
}
