//! The hub: finds every mouse connection (a receiver, or a mouse on its cable) and runs each on a
//! thread of its own (an `Engine`). Its own thread lists the HID devices every 1.5 s, starts an
//! engine for each connection a family claims, and lets go of one that's gone.

use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use hidapi::HidApi;
use parking_lot::RwLock;

use super::{DeviceState, Engine, Family, Notify};

/// How often to look for connections plugged in or out.
const LOOK_EVERY: Duration = Duration::from_millis(1500);

struct Conn {
    /// The family's id for the connection, the same while it stays plugged in.
    id: String,
    engine: Engine,
}

pub struct Hub {
    conns: RwLock<Vec<Conn>>,
}

impl Hub {
    /// A hub with nothing found yet; `run` starts looking.
    pub fn new() -> Arc<Self> {
        Arc::new(Self { conns: RwLock::new(Vec::new()) })
    }

    /// Start looking, once whoever `notify` tells is ready to hear. `data_dir` keeps each mouse's
    /// battery files.
    pub fn run(self: &Arc<Self>, families: Vec<Box<dyn Family>>, notify: Notify, data_dir: Option<PathBuf>) {
        let looker = self.clone();
        thread::Builder::new()
            .name("hrtz-hub".into())
            .spawn(move || {
                let mut api = HidApi::new().ok();
                loop {
                    let devices: Vec<_> = match api.as_mut() {
                        Some(api) => match api.refresh_devices() {
                            Ok(()) => api.device_list().cloned().collect(),
                            Err(_) => Vec::new(),
                        },
                        None => {
                            api = HidApi::new().ok();
                            Vec::new()
                        }
                    };
                    let found: Vec<(String, &dyn Family)> = families
                        .iter()
                        .flat_map(|f| f.connections(&devices).into_iter().map(move |id| (id, f.as_ref())))
                        .collect();
                    looker.follow(&found, &notify, &data_dir);
                    thread::sleep(LOOK_EVERY);
                }
            })
            .expect("spawn hub");
    }

    /// Start an engine for each connection found that has none, and let go of the ones gone. A
    /// let-go engine tells the UI its mouse went before its thread ends.
    fn follow(&self, found: &[(String, &dyn Family)], notify: &Notify, data_dir: &Option<PathBuf>) {
        let mut conns = self.conns.write();
        conns.retain(|c| found.iter().any(|(id, _)| *id == c.id));
        for (id, family) in found {
            if !conns.iter().any(|c| c.id == *id) {
                let engine = Engine::start(family.finder(id), notify.clone(), data_dir.clone());
                conns.push(Conn { id: id.clone(), engine });
            }
        }
    }

    /// Every connection's engine, in the order they were found.
    pub fn engines(&self) -> Vec<Engine> {
        self.conns.read().iter().map(|c| c.engine.clone()).collect()
    }

    /// The engine running mouse `key`, if it's connected.
    pub fn engine(&self, key: &str) -> Option<Engine> {
        self.conns.read().iter().find(|c| c.engine.state.read().key.as_deref() == Some(key)).map(|c| c.engine.clone())
    }

    /// Every connection's state.
    pub fn states(&self) -> Vec<DeviceState> {
        self.conns.read().iter().map(|c| c.engine.snapshot()).collect()
    }

    /// The mouse the tray, the HUD and the pop-ups follow: one awake, else one connected, else the
    /// first found. (Which one is being moved comes later: PLAN-mice.md, 3.10.)
    pub fn primary(&self) -> Option<Engine> {
        let conns = self.conns.read();
        let pick = |f: &dyn Fn(&DeviceState) -> bool| {
            conns.iter().find(|c| f(&c.engine.state.read())).map(|c| c.engine.clone())
        };
        pick(&|s| s.connected && s.online && s.key.is_some())
            .or_else(|| pick(&|s| s.connected && s.key.is_some()))
            .or_else(|| conns.first().map(|c| c.engine.clone()))
    }

    /// The primary mouse's state; a default (nothing connected) when there's none.
    pub fn primary_state(&self) -> DeviceState {
        self.primary().map(|e| e.snapshot()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use parking_lot::Mutex;

    use super::*;
    use crate::mice::DeviceEvent;
    use crate::mice::compx::sim::{SimFamily, SimMouse};

    /// Every state the engines told, with what happened.
    type Heard = Arc<Mutex<Vec<(DeviceState, Option<DeviceEvent>)>>>;

    /// Wait up to 5 s for `f`.
    fn until(f: impl Fn() -> bool) {
        let start = Instant::now();
        while !f() {
            assert!(start.elapsed() < Duration::from_secs(5), "timed out");
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn each_mouse_gets_its_own_thread_and_a_lost_one_says_so() {
        let family = SimFamily::of(vec![
            ("sim:a", SimMouse::default()),
            ("sim:b", SimMouse { mid: 3, device_type: 1, ..Default::default() }),
        ]);
        let b = family.mice[1].1.clone();
        let heard: Heard = Arc::default();
        let sink = heard.clone();
        let hub = Hub::new();
        hub.run(vec![Box::new(family)], Arc::new(move |s, e| sink.lock().push((s.clone(), e))), None);

        let connected = |key: &str| hub.states().iter().any(|s| s.key.as_deref() == Some(key) && s.settings.is_some());
        until(|| connected("sim/7-4") && connected("sim/7-3"));
        assert_eq!(hub.states().len(), 2);
        assert_eq!(hub.primary_state().key.as_deref(), Some("sim/7-4"), "the first found, awake");

        // A command reaches the mouse it names.
        hub.engine("sim/7-3").unwrap().call(|w| w.set_long_range(true)).unwrap();
        assert!(b.lock().long_range);
        assert_eq!(hub.engine("sim/7-4").unwrap().snapshot().long_range, Some(false));

        // Unplugged: its thread goes, and the UI hears which mouse went.
        b.lock().unplugged = true;
        until(|| hub.states().len() == 1);
        until(|| heard.lock().iter().any(|(s, _)| s.key.as_deref() == Some("sim/7-3") && !s.connected));
        assert!(hub.engine("sim/7-3").is_none());
    }
}
