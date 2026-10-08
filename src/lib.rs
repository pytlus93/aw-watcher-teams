use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Deserialize)]
pub struct Snapshot {
    pub collector_ok: bool,
    pub windows: Vec<Window>,
}
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Window {
    /// Volatile process-instance/window correlation only; never a provider meeting ID.
    pub key: String,
    pub leave: bool,
    pub microphone: bool,
    pub camera: bool,
    pub join: bool,
    pub sharing: bool,
    pub meeting_info: bool,
    pub title: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Activity {
    pub schema_version: u32,
    pub app: String,
    pub title: String,
    pub meeting_title: Option<String>,
    pub call_kind: String,
    pub state: String,
    pub assessment: String,
    pub evidence: String,
    pub title_source: Option<String>,
}
pub fn readable_title(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let mut title = raw.trim();
    for suffix in [
        " | Microsoft Teams",
        " | MS Teams",
        " - Microsoft Teams",
        " — Microsoft Teams",
    ] {
        if let Some(value) = title.strip_suffix(suffix) {
            title = value.trim();
            break;
        }
    }
    if title.is_empty()
        || [
            "Microsoft Teams",
            "MS Teams",
            "Teams",
            "Meeting",
            "Call",
            "Schůzka",
            "| Microsoft Teams",
        ]
        .iter()
        .any(|generic| title.eq_ignore_ascii_case(generic))
    {
        return None;
    }
    let title: String = title
        .chars()
        .filter(|c| !c.is_control())
        .take(240)
        .collect();
    if title.trim().is_empty() {
        None
    } else {
        Some(title)
    }
}
pub fn candidates(snapshot: &Snapshot) -> Vec<&Window> {
    snapshot
        .windows
        .iter()
        .filter(|w| w.leave && (w.microphone || w.camera || w.sharing) && !w.join)
        .collect()
}
pub fn classify(snapshot: &Snapshot, collect_title: bool) -> Activity {
    let calls = candidates(snapshot);
    let mut result = Activity {
        schema_version: 2,
        app: "MS Teams".into(),
        title: "Call state unknown".into(),
        meeting_title: None,
        call_kind: "unknown".into(),
        state: "unknown".into(),
        assessment: "ui_inferred".into(),
        evidence: "no_joined_control_set".into(),
        title_source: None,
    };
    if !snapshot.collector_ok {
        result.evidence = "collector_failure".into();
    } else if calls.len() > 1 {
        result.state = "ambiguous".into();
        result.evidence = "multiple_control_sets".into();
    } else if let Some(w) = calls.first() {
        result.state = "joined_candidate".into();
        result.evidence = "uia_control_set".into();
        result.call_kind = if w.meeting_info { "meeting" } else { "unknown" }.into();
        result.title = if w.meeting_info {
            "Meeting (title unavailable)"
        } else {
            "Call (title unavailable)"
        }
        .into();
        if collect_title {
            result.meeting_title = readable_title(w.title.as_deref());
            if let Some(title) = &result.meeting_title {
                result.title = title.clone();
                result.title_source = Some("qualified_window_title".into());
            }
        }
    } else if snapshot.windows.iter().any(|w| w.join) {
        result.state = "prejoin".into();
        result.title = "Prejoin".into();
        result.evidence = "uia_join_control".into();
    }
    result
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionData {
    #[serde(flatten)]
    pub activity: Activity,
    pub session_id: String,
    pub identity_source: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEvent {
    pub timestamp: DateTime<Utc>,
    pub duration: f64,
    pub data: SessionData,
}
struct Active {
    key: String,
    last: DateTime<Utc>,
    event: SessionEvent,
}
/// A positive interval is emitted only after two matching observations.
/// Media/title changes update the same event; failures/sleep never extend it.
/// Session IDs are locally generated, not invented provider IDs.
pub struct Tracker {
    active: Option<Active>,
    namespace: String,
    next: u64,
    max_gap_seconds: f64,
    collect_title: bool,
}
impl Tracker {
    pub fn new(namespace: String, max_gap_seconds: f64, collect_title: bool) -> Self {
        Self {
            active: None,
            namespace,
            next: 0,
            max_gap_seconds,
            collect_title,
        }
    }
    pub fn observe(&mut self, snapshot: &Snapshot, now: DateTime<Utc>) -> Option<SessionEvent> {
        let activity = classify(snapshot, self.collect_title);
        let calls = candidates(snapshot);
        if activity.state != "joined_candidate" || calls[0].key.is_empty() {
            self.active = None;
            return None;
        }
        let key = &calls[0].key;
        if let Some(active) = &mut self.active {
            let elapsed = (now - active.last).num_milliseconds() as f64 / 1000.0;
            if &active.key == key && elapsed > 0.0 && elapsed <= self.max_gap_seconds {
                active.last = now;
                active.event.duration =
                    (now - active.event.timestamp).num_milliseconds() as f64 / 1000.0;
                // Missing title on the same window does not discard already observed context.
                let mut current = activity;
                if self.collect_title && current.meeting_title.is_none() {
                    current.meeting_title = active.event.data.activity.meeting_title.clone();
                    current.title_source = active.event.data.activity.title_source.clone();
                    if let Some(title) = &current.meeting_title {
                        current.title = title.clone();
                    }
                }
                if current.call_kind == "unknown"
                    && active.event.data.activity.call_kind == "meeting"
                {
                    current.call_kind = "meeting".into();
                }
                active.event.data.activity = current;
                return Some(active.event.clone());
            }
        }
        self.next += 1;
        self.active = Some(Active {
            key: key.clone(),
            last: now,
            event: SessionEvent {
                timestamp: now,
                duration: 0.0,
                data: SessionData {
                    activity,
                    session_id: format!("{}-{}", self.namespace, self.next),
                    identity_source: "local_observed_session".into(),
                },
            },
        });
        None
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
        return Err("AW server must be HTTP loopback without credentials/query/fragment".into());
    }
    Ok(u)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn call() -> Window {
        Window {
            key: "synthetic-window-1".into(),
            leave: true,
            microphone: true,
            camera: true,
            title: Some("Synthetic planning | Microsoft Teams".into()),
            ..Default::default()
        }
    }
    fn snapshot(windows: Vec<Window>) -> Snapshot {
        Snapshot {
            collector_ok: true,
            windows,
        }
    }
    fn at(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000 + seconds, 0).unwrap()
    }
    fn tracker() -> Tracker {
        Tracker::new("test".into(), 15.0, true)
    }
    #[test]
    fn no_zero_or_unknown_records() {
        let mut t = tracker();
        assert!(t.observe(&Snapshot::default(), at(0)).is_none());
        assert!(t.observe(&snapshot(vec![call()]), at(1)).is_none());
        assert!(t.observe(&snapshot(vec![]), at(2)).is_none());
    }
    #[test]
    fn one_hour_is_one_session_with_full_duration() {
        let mut t = tracker();
        let s = snapshot(vec![call()]);
        assert!(t.observe(&s, at(0)).is_none());
        for second in (5..=3600).step_by(5) {
            let e = t.observe(&s, at(second)).unwrap();
            assert_eq!(e.data.session_id, "test-1");
            assert_eq!(e.timestamp, at(0));
            assert_eq!(e.duration, second as f64);
        }
    }
    #[test]
    fn sharing_does_not_split_call() {
        let mut t = tracker();
        t.observe(&snapshot(vec![call()]), at(0));
        let mut w = call();
        w.sharing = true;
        let e = t.observe(&snapshot(vec![w]), at(5)).unwrap();
        assert_eq!(e.data.session_id, "test-1");
        assert_eq!(e.duration, 5.0);
    }
    #[test]
    fn title_appearing_later_updates_same_session() {
        let mut t = tracker();
        let mut w = call();
        w.title = None;
        t.observe(&snapshot(vec![w]), at(0));
        let e = t.observe(&snapshot(vec![call()]), at(5)).unwrap();
        assert_eq!(e.data.session_id, "test-1");
        assert_eq!(e.data.activity.title, "Synthetic planning");
    }
    #[test]
    fn lost_title_is_retained_only_within_session() {
        let mut t = tracker();
        t.observe(&snapshot(vec![call()]), at(0));
        let mut w = call();
        w.title = None;
        assert_eq!(
            t.observe(&snapshot(vec![w.clone()]), at(5))
                .unwrap()
                .data
                .activity
                .title,
            "Synthetic planning"
        );
        t.observe(&snapshot(vec![]), at(10));
        t.observe(&snapshot(vec![w.clone()]), at(15));
        assert_eq!(
            t.observe(&snapshot(vec![w]), at(20))
                .unwrap()
                .data
                .activity
                .meeting_title,
            None
        );
    }
    #[test]
    fn media_controls_can_be_missing() {
        let mut w = call();
        w.camera = false;
        assert_eq!(classify(&snapshot(vec![w]), true).state, "joined_candidate");
    }
    #[test]
    fn leave_alone_is_not_attendance() {
        let w = Window {
            leave: true,
            ..Default::default()
        };
        assert_eq!(classify(&snapshot(vec![w]), true).state, "unknown");
    }
    #[test]
    fn prejoin_is_not_attendance() {
        let mut w = call();
        w.join = true;
        assert_eq!(classify(&snapshot(vec![w]), true).state, "prejoin");
    }
    #[test]
    fn separate_windows_are_not_merged_by_title() {
        let mut t = tracker();
        t.observe(&snapshot(vec![call()]), at(0));
        let mut w = call();
        w.key = "another-window".into();
        assert!(t.observe(&snapshot(vec![w.clone()]), at(5)).is_none());
        assert_eq!(
            t.observe(&snapshot(vec![w]), at(10))
                .unwrap()
                .data
                .session_id,
            "test-2"
        );
    }
    #[test]
    fn failure_breaks_duration() {
        let mut t = tracker();
        t.observe(&snapshot(vec![call()]), at(0));
        t.observe(&snapshot(vec![call()]), at(5));
        assert!(t.observe(&Snapshot::default(), at(10)).is_none());
        assert!(t.observe(&snapshot(vec![call()]), at(15)).is_none());
        assert_eq!(
            t.observe(&snapshot(vec![call()]), at(20)).unwrap().duration,
            5.0
        );
    }
    #[test]
    fn sleep_or_clock_jump_is_not_counted() {
        let mut t = tracker();
        let s = snapshot(vec![call()]);
        t.observe(&s, at(0));
        assert!(t.observe(&s, at(600)).is_none());
        assert_eq!(t.observe(&s, at(605)).unwrap().duration, 5.0);
    }
    #[test]
    fn clock_rollback_is_not_counted() {
        let mut t = tracker();
        let s = snapshot(vec![call()]);
        t.observe(&s, at(10));
        assert!(t.observe(&s, at(5)).is_none());
        assert_eq!(t.observe(&s, at(10)).unwrap().duration, 5.0);
    }
    #[test]
    fn privacy_removes_context_from_title_and_metadata() {
        let a = classify(&snapshot(vec![call()]), false);
        assert_eq!(a.title, "Call (title unavailable)");
        assert_eq!(a.meeting_title, None);
        assert_eq!(a.title_source, None);
    }
    #[test]
    fn meeting_kind_needs_evidence() {
        let mut w = call();
        w.meeting_info = true;
        assert_eq!(classify(&snapshot(vec![w]), true).call_kind, "meeting");
        assert_eq!(classify(&snapshot(vec![call()]), true).call_kind, "unknown");
    }
    #[test]
    fn generic_title_does_not_replace_fallback() {
        assert_eq!(readable_title(Some("Microsoft Teams")), None);
        assert_eq!(readable_title(Some(" | Microsoft Teams")), None);
        assert_eq!(
            readable_title(Some("Synthetic review | Microsoft Teams")),
            Some("Synthetic review".into())
        );
    }
    #[test]
    fn multiple_windows_preserve_ambiguity() {
        assert_eq!(
            classify(&snapshot(vec![call(), call()]), true).state,
            "ambiguous"
        );
    }
    #[test]
    fn loopback_only() {
        for u in [
            "https://example.com",
            "http://example.com",
            "http://user:secret@localhost",
            "http://localhost?secret=x",
        ] {
            assert!(validate_server(u).is_err());
        }
        assert!(validate_server("http://127.0.0.1:5666/").is_ok());
    }
}

pub mod aw;
