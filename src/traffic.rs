//! Interface-scoped traffic deltas. No network I/O or wall-clock dependency.
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub at: Instant,
    pub received: u64,
    pub sent: u64,
}
#[derive(Default)]
pub struct Tracker {
    interface: String,
    first: Option<Sample>,
    previous: Option<Sample>,
}
impl Tracker {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn update(&mut self, interface: &str, sample: Sample) -> serde_json::Value {
        if self.interface != interface
            || self.previous.is_some_and(|p| {
                sample.received < p.received || sample.sent < p.sent || sample.at <= p.at
            })
        {
            self.reset();
        }
        self.interface = interface.into();
        let first = *self.first.get_or_insert(sample);
        let mut receiving = "—".to_owned();
        let mut sending = "—".to_owned();
        if let Some(previous) = self.previous {
            let elapsed = sample.at.duration_since(previous.at);
            // A closed popup's slow sampling must not masquerade as an instantaneous rate.
            if elapsed > Duration::ZERO && elapsed <= Duration::from_secs(3) {
                receiving = format!(
                    "{}/s",
                    bytes((sample.received - previous.received) as f64 / elapsed.as_secs_f64())
                );
                sending = format!(
                    "{}/s",
                    bytes((sample.sent - previous.sent) as f64 / elapsed.as_secs_f64())
                );
            }
        }
        self.previous = Some(sample);
        serde_json::json!({
            "receiving": receiving, "sending": sending,
            "downloaded": bytes((sample.received - first.received) as f64),
            "uploaded": bytes((sample.sent - first.sent) as f64),
            "traffic_period": format!("Suivi de cette interface · {} min", sample.at.duration_since(first.at).as_secs() / 60),
        })
    }
}
pub fn unavailable() -> serde_json::Value {
    serde_json::json!({"receiving":"—", "sending":"—", "downloaded":"—", "uploaded":"—", "traffic_period":"Trafic indisponible"})
}
fn bytes(mut value: f64) -> String {
    let units = ["o", "Ko", "Mo", "Go", "To"];
    let mut unit = 0;
    while value >= 1000.0 && unit < units.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value:.0} {}", units[unit])
    } else {
        format!("{value:.1} {}", units[unit])
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rates_use_actual_time_and_totals_use_baseline() {
        let mut tracker = Tracker::default();
        let at = Instant::now();
        let first = Sample {
            at,
            received: 5000,
            sent: 1000,
        };
        assert_eq!(tracker.update("wifi", first)["receiving"], "—");
        let result = tracker.update(
            "wifi",
            Sample {
                at: at + Duration::from_secs(2),
                received: 9000,
                sent: 1500,
            },
        );
        assert_eq!(result["receiving"], "2.0 Ko/s");
        assert_eq!(result["sending"], "250 o/s");
        assert_eq!(result["downloaded"], "4.0 Ko");
    }
    #[test]
    fn interface_changes_and_counter_resets_start_a_new_baseline() {
        let at = Instant::now();
        let mut tracker = Tracker::default();
        for (i, interface, received) in [(0, "wifi1", 5000), (1, "wifi2", 9000), (2, "wifi2", 100)]
        {
            let result = tracker.update(
                interface,
                Sample {
                    at: at + Duration::from_secs(i),
                    received,
                    sent: received,
                },
            );
            assert_eq!(result["downloaded"], "0 o");
            assert_eq!(result["receiving"], "—");
        }
    }
    #[test]
    fn slow_samples_keep_totals_but_not_live_rates() {
        let at = Instant::now();
        let mut tracker = Tracker::default();
        tracker.update(
            "wifi",
            Sample {
                at,
                received: 0,
                sent: 0,
            },
        );
        let result = tracker.update(
            "wifi",
            Sample {
                at: at + Duration::from_secs(30),
                received: 5000,
                sent: 0,
            },
        );
        assert_eq!(result["receiving"], "—");
        assert_eq!(result["downloaded"], "5.0 Ko");
        tracker.reset();
        assert_eq!(
            tracker.update(
                "wifi",
                Sample {
                    at: at + Duration::from_secs(31),
                    received: 6000,
                    sent: 0
                }
            )["downloaded"],
            "0 o"
        );
    }
    #[test]
    fn equal_timestamps_do_not_divide_by_zero() {
        let mut tracker = Tracker::default();
        let sample = Sample {
            at: Instant::now(),
            received: 0,
            sent: 0,
        };
        tracker.update("wifi", sample);
        assert_eq!(tracker.update("wifi", sample)["receiving"], "—");
    }
}
