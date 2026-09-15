//! Native Wi-Fi counters, sampled independently of the slower PowerShell scan.
use super::*;
use crate::traffic::{Sample, Tracker};
use windows::{
    Win32::NetworkManagement::{
        IpHelper::{GetIfEntry2, MIB_IF_ROW2},
        Ndis::IfOperStatusUp,
    },
    core::GUID,
};

pub(super) struct Monitor {
    tracker: Tracker,
    interface: String,
    values: serde_json::Value,
    pub due: Instant,
    running: bool,
    failed: bool,
}
impl Default for Monitor {
    fn default() -> Self {
        Self {
            tracker: Tracker::default(),
            interface: String::new(),
            values: crate::traffic::unavailable(),
            due: Instant::now(),
            running: false,
            failed: false,
        }
    }
}
impl Monitor {
    pub fn select(&mut self, interface: &str) {
        if self.interface != interface {
            self.interface = interface.into();
            self.tracker.reset();
            self.failed = false;
            self.values = crate::traffic::unavailable();
            self.due = Instant::now();
        }
    }
    pub fn merge(&self, data: &mut serde_json::Value) {
        if let Some(object) = data.as_object_mut() {
            object.extend(self.values.as_object().expect("traffic object").clone());
        }
    }
    pub fn poll(&mut self, name: &str, generation: u64, open: bool, tx: &EventSender) {
        if self.running || self.interface.is_empty() || self.due > Instant::now() {
            return;
        }
        self.running = true;
        self.due = Instant::now() + Duration::from_secs(if open { 1 } else { 30 });
        let name = name.to_owned();
        let interface = self.interface.clone();
        let tx = tx.clone();
        std::thread::spawn(move || {
            let result = read(&interface);
            let _ = tx.send(Event::AppletTraffic {
                name,
                generation,
                interface,
                result,
            });
        });
    }
    /// Request one metadata refresh on a transition to unavailable counters.
    pub fn finish(&mut self, interface: &str, result: Result<Sample, String>) -> bool {
        self.running = false;
        if self.interface != interface {
            return false;
        }
        let refresh = result.is_err() && !self.failed;
        self.failed = result.is_err();
        self.values = match result {
            Ok(sample) => self.tracker.update(interface, sample),
            Err(error) => {
                tracing::debug!(%error, "Wi-Fi traffic unavailable");
                self.tracker.reset();
                crate::traffic::unavailable()
            }
        };
        refresh
    }
}
pub(super) fn read(interface: &str) -> Result<Sample, String> {
    let guid = GUID::try_from(interface.trim_matches(['{', '}']))
        .map_err(|e| format!("invalid interface GUID: {e}"))?;
    let mut row = MIB_IF_ROW2 {
        InterfaceGuid: guid,
        ..Default::default()
    };
    // SAFETY: row is initialized and exclusively borrowed for this synchronous API call.
    unsafe { GetIfEntry2(&mut row) }
        .ok()
        .map_err(|e| e.to_string())?;
    // Do not accidentally sample an Ethernet, tunnel, VPN or disconnected adapter.
    if row.Type != 71 || row.OperStatus != IfOperStatusUp {
        return Err("Wi-Fi interface is not connected".into());
    }
    Ok(Sample {
        at: Instant::now(),
        received: row.InOctets,
        sent: row.OutOctets,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrong_interface_completion_is_ignored() {
        let mut monitor = Monitor::default();
        monitor.select("new");
        monitor.running = true;
        monitor.finish(
            "old",
            Ok(Sample {
                at: Instant::now(),
                received: 100,
                sent: 100,
            }),
        );
        assert!(!monitor.running);
        assert_eq!(monitor.values, crate::traffic::unavailable());
    }
    #[test]
    fn failures_request_one_refresh_not_a_scan_storm() {
        let mut monitor = Monitor::default();
        monitor.select("wifi");
        assert!(monitor.finish("wifi", Err("disconnected".into())));
        assert!(!monitor.finish("wifi", Err("still disconnected".into())));
        assert!(!monitor.finish(
            "wifi",
            Ok(Sample {
                at: Instant::now(),
                received: 100,
                sent: 100
            })
        ));
        assert!(monitor.finish("wifi", Err("disconnected again".into())));
    }
    #[test]
    fn native_reader_rejects_invalid_guids() {
        assert!(read("not-an-interface").is_err());
    }
}
