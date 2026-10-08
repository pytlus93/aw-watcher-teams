use serde::{Deserialize, Serialize};
#[derive(Debug, Default, Deserialize)]
pub struct Snapshot {
    pub collector_ok: bool,
    pub windows: Vec<Window>,
}
#[derive(Debug, Default, Deserialize)]
pub struct Window {
    pub leave: bool,
    pub microphone: bool,
    pub camera: bool,
    pub join: bool,
    pub sharing: bool,
}
#[derive(Debug, Serialize, PartialEq)]
pub struct Activity {
    pub schema_version: u32,
    pub app: String,
    pub title: String,
    pub state: String,
    pub assessment: String,
    pub evidence: String,
}
pub fn classify(s: &Snapshot) -> Activity {
    let calls: Vec<_> = s
        .windows
        .iter()
        .filter(|w| w.leave && w.microphone && w.camera && !w.join)
        .collect();
    let (state, title, evidence) = if !s.collector_ok {
        ("unknown", "Call state unknown", "collector_failure")
    } else if calls.len() > 1 {
        (
            "ambiguous",
            "Multiple call windows",
            "multiple_control_sets",
        )
    } else if let Some(w) = calls.first() {
        (
            "joined_candidate",
            if w.sharing { "Presenting" } else { "Call" },
            "uia_control_set",
        )
    } else if s.windows.iter().any(|w| w.join) {
        ("prejoin", "Prejoin", "uia_join_control")
    } else {
        ("unknown", "Call state unknown", "no_joined_control_set")
    };
    Activity {
        schema_version: 1,
        app: "MS Teams".into(),
        title: title.into(),
        state: state.into(),
        assessment: "ui_inferred_unvalidated".into(),
        evidence: evidence.into(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn call() -> Window {
        Window {
            leave: true,
            microphone: true,
            camera: true,
            ..Default::default()
        }
    }
    fn snapshot(w: Vec<Window>) -> Snapshot {
        Snapshot {
            collector_ok: true,
            windows: w,
        }
    }
    #[test]
    fn muted_camera_off_controls_still_count() {
        assert_eq!(classify(&snapshot(vec![call()])).state, "joined_candidate");
    }
    #[test]
    fn title_has_no_app_prefix() {
        assert_eq!(classify(&snapshot(vec![call()])).title, "Call");
    }
    #[test]
    fn leave_alone_insufficient() {
        assert_eq!(
            classify(&snapshot(vec![Window {
                leave: true,
                ..Default::default()
            }]))
            .state,
            "unknown"
        );
    }
    #[test]
    fn prejoin_not_joined() {
        let mut w = call();
        w.join = true;
        assert_eq!(classify(&snapshot(vec![w])).state, "prejoin");
    }
    #[test]
    fn multiple_calls_ambiguous() {
        assert_eq!(classify(&snapshot(vec![call(), call()])).state, "ambiguous");
    }
    #[test]
    fn missing_controls_not_end() {
        assert_eq!(classify(&snapshot(vec![])).state, "unknown");
    }
    #[test]
    fn failure_not_end() {
        assert_eq!(
            classify(&Snapshot {
                collector_ok: false,
                windows: vec![call()]
            })
            .evidence,
            "collector_failure"
        );
    }
    #[test]
    fn sharing_title() {
        let mut w = call();
        w.sharing = true;
        assert_eq!(classify(&snapshot(vec![w])).title, "Presenting");
    }
}
pub fn validate_server(value: &str) -> Result<url::Url, String> {
    let u = url::Url::parse(value).map_err(|e| e.to_string())?;
    if u.scheme() != "http"
        || !matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
        || !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
    {
        return Err(
            "AW server must be plain HTTP loopback without credentials/query/fragment".into(),
        );
    }
    Ok(u)
}

pub fn bounded_duration(
    previous: Option<&Activity>,
    current: &Activity,
    elapsed: f64,
    max_gap: f64,
) -> f64 {
    if previous == Some(current)
        && current.state == "joined_candidate"
        && elapsed >= 0.0
        && elapsed <= max_gap
    {
        elapsed
    } else {
        0.0
    }
}
#[cfg(test)]
mod interval_tests {
    use super::*;
    fn active() -> Activity {
        classify(&Snapshot {
            collector_ok: true,
            windows: vec![Window {
                leave: true,
                microphone: true,
                camera: true,
                ..Default::default()
            }],
        })
    }
    #[test]
    fn continuous_evidence_counts() {
        let a = active();
        assert_eq!(bounded_duration(Some(&a), &a, 5.0, 10.0), 5.0);
    }
    #[test]
    fn sleep_gap_not_bridged() {
        let a = active();
        assert_eq!(bounded_duration(Some(&a), &a, 600.0, 10.0), 0.0);
    }
    #[test]
    fn changed_state_not_bridged() {
        let a = active();
        let u = classify(&Snapshot::default());
        assert_eq!(bounded_duration(Some(&a), &u, 5.0, 10.0), 0.0);
    }
    #[test]
    fn first_observation_has_no_duration() {
        assert_eq!(bounded_duration(None, &active(), 5.0, 10.0), 0.0);
    }
    #[test]
    fn external_urls_and_credentials_rejected() {
        for url in [
            "https://example.com",
            "http://example.com",
            "http://user:secret@localhost",
            "http://localhost?secret=x",
        ] {
            assert!(validate_server(url).is_err());
        }
        assert!(validate_server("http://127.0.0.1:5666/").is_ok());
    }
}
