use crate::{SessionData, SessionEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, fs, path::PathBuf, time::Duration};

#[derive(Clone, Serialize, Deserialize)]
struct Pending {
    event: SessionEvent,
    id: Option<u64>,
    dirty: bool,
    may_exist: bool,
}
/// Persist before delivery. Reuse AW's event ID for every update of the same session.
/// If an insertion reply is lost, locate the local session ID before inserting again.
pub struct AwWriter {
    agent: ureq::Agent,
    bucket_url: String,
    checkpoint: PathBuf,
    pending: Vec<Pending>,
    initialized: bool,
    verified: HashSet<String>,
    payload: crate::payload::PayloadConfig,
    collect_title: bool,
    title_format: crate::title::TitleFormat,
}
impl AwWriter {
    pub fn new(
        bucket_url: String,
        checkpoint: PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        crate::validate_server(&bucket_url)?;
        let pending = if checkpoint.exists() {
            match serde_json::from_slice(&fs::read(&checkpoint)?) {
                Ok(pending) => pending,
                Err(_) => {
                    let backup = checkpoint.with_extension(format!(
                        "corrupt-{}-{}",
                        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
                        std::process::id()
                    ));
                    fs::rename(&checkpoint, &backup)?;
                    eprintln!(
                        "Damaged delivery checkpoint preserved at {}; unsent data may be lost",
                        backup.display()
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        Ok(Self {
            agent: ureq::AgentBuilder::new()
                .timeout(Duration::from_secs(3))
                .redirects(0)
                .build(),
            bucket_url,
            checkpoint,
            pending,
            initialized: false,
            verified: HashSet::new(),
            payload: crate::payload::PayloadConfig::default(),
            collect_title: true,
            title_format: crate::title::TitleFormat::default(),
        })
    }
    pub fn configure_payload(
        &mut self,
        payload: crate::payload::PayloadConfig,
        collect_title: bool,
        format: crate::title::TitleFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.title_format = payload.title_format(&format);
        self.collect_title = payload.collect_title(collect_title);
        self.payload = payload;
        self.reformat_pending_titles(self.collect_title, &self.title_format.clone())?;
        // Payload changes must replace the latest retained event, including extension removal.
        for item in &mut self.pending {
            item.dirty = true;
            self.payload.apply_privacy(&mut item.event.data.activity);
        }
        self.save()
    }
    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let temp = self.checkpoint.with_extension("tmp");
        use std::io::Write;
        let mut file = fs::File::create(&temp)?;
        file.write_all(&serde_json::to_vec(&self.pending)?)?;
        file.sync_all()?;
        drop(file);
        fs::rename(temp, &self.checkpoint)?;
        Ok(())
    }
    pub fn queue(&mut self, mut event: SessionEvent) -> Result<(), Box<dyn std::error::Error>> {
        if !event.duration.is_finite() || event.duration <= 0.0 {
            return Err("Only positive-duration session events may be stored".into());
        }
        crate::reformat_activity(
            &mut event.data.activity,
            self.collect_title,
            &self.title_format,
        );
        self.payload.apply_privacy(&mut event.data.activity);
        if let Some(existing) = self
            .pending
            .iter_mut()
            .find(|p| p.event.data.session_id == event.data.session_id)
        {
            existing.event = event;
            existing.dirty = true;
        } else {
            if self.pending.len() >= 1000 {
                return Err("Session delivery backlog full".into());
            }
            self.pending.push(Pending {
                event,
                id: None,
                dirty: true,
                may_exist: false,
            });
        }
        self.save()
    }
    pub fn reformat_pending_titles(
        &mut self,
        collect_title: bool,
        format: &crate::title::TitleFormat,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.collect_title = self.payload.collect_title(collect_title);
        self.title_format = self.payload.title_format(format);
        for item in &mut self.pending {
            let before = item.event.data.activity.clone();
            crate::reformat_activity(
                &mut item.event.data.activity,
                self.collect_title,
                &self.title_format,
            );
            self.payload.apply_privacy(&mut item.event.data.activity);
            if item.event.data.activity != before {
                item.dirty = true;
            }
        }
        self.save()
    }
    pub fn redact_pending_titles(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.reformat_pending_titles(false, &crate::title::TitleFormat::default())
    }
    fn recover_id(&self, item: &Pending) -> Result<Option<u64>, Box<dyn std::error::Error>> {
        let mut url = url::Url::parse(&format!("{}/events", self.bucket_url))?;
        url.query_pairs_mut()
            .append_pair("start", &item.event.timestamp.to_rfc3339())
            .append_pair("limit", "-1");
        let events: Vec<Value> = self.agent.get(url.as_str()).call()?.into_json()?;
        let matches: Vec<_> = events
            .iter()
            .filter(|e| match e["data"]["session_id"].as_str() {
                Some(id) => id == item.event.data.session_id,
                None => e["timestamp"]
                    .as_str()
                    .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
                    .is_some_and(|stamp| {
                        stamp.timestamp_millis() == item.event.timestamp.timestamp_millis()
                    }),
            })
            .collect();
        if matches.len() > 1 {
            return Err("Ambiguous session recovery; refusing to overwrite events".into());
        }
        Ok(matches.first().and_then(|e| e["id"].as_u64()))
    }

    fn initialize_bucket(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "local".into());
        let response = self
            .agent
            .post(&self.bucket_url)
            .send_json(serde_json::json!({
                "client":"aw-watcher-teams", "type":"call-evidence", "hostname":hostname
            }));
        // 304 is success ONLY for bucket creation, not arbitrary transport requests.
        let created = match response {
            Ok(r) if r.status() == 304 => false,
            Ok(r) if r.status() == 200 || r.status() == 201 => true,
            Err(ureq::Error::Status(304, _)) => false,
            _ => return Err("Bucket initialization unavailable".into()),
        };
        let metadata: Value = self.agent.get(&self.bucket_url).call()?.into_json()?;
        if metadata["type"].as_str() != Some("call-evidence") {
            return Err("Existing bucket has an incompatible event type".into());
        }
        self.verified.clear();
        if created {
            // A lost/recreated database cannot reuse old event IDs.
            for item in &mut self.pending {
                item.id = None;
                item.may_exist = false;
                item.dirty = true;
            }
            self.save()?;
        }
        self.initialized = true;
        Ok(())
    }
    pub fn flush(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let result = self.flush_inner();
        if result.is_err() {
            self.initialized = false;
            self.verified.clear();
        }
        result
    }
    fn flush_inner(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if !self.initialized {
            self.initialize_bucket()?;
        }
        for index in 0..self.pending.len() {
            if !self.pending[index].dirty {
                continue;
            }
            let session_id = self.pending[index].event.data.session_id.clone();
            if !self.verified.contains(&session_id)
                && (self.pending[index].id.is_some() || self.pending[index].may_exist)
            {
                // Never overwrite another session if a restored database reused an ID.
                self.pending[index].id = self.recover_id(&self.pending[index])?;
                self.verified.insert(session_id.clone());
            }
            self.pending[index].may_exist = true;
            self.save()?;
            let body = serde_json::to_string(&[self
                .payload
                .wire(&self.pending[index].event, self.pending[index].id)])?;
            let reply = self
                .agent
                .post(&format!("{}/events", self.bucket_url))
                .set("Content-Type", "application/json")
                .send_string(&body)?;
            if reply.status() != 200 {
                return Err("AW event write was not acknowledged".into());
            }
            let response: Value = reply.into_json()?;
            let id = response
                .as_array()
                .and_then(|a| a.first())
                .and_then(|e| e["id"].as_u64())
                .or_else(|| response["id"].as_u64())
                .or(self.pending[index].id);
            self.pending[index].id = match id {
                Some(id) => Some(id),
                None => self.recover_id(&self.pending[index])?,
            };
            if self.pending[index].id.is_none() {
                return Err("AW event acknowledgement unavailable".into());
            }
            self.verified.insert(session_id);
            self.pending[index].dirty = false;
            self.save()?;
        }
        // Preserve the latest acknowledged session for future updates; retain failed older sessions.
        let latest = self.pending.last().map(|p| p.event.data.session_id.clone());
        self.pending
            .retain(|p| p.dirty || Some(&p.event.data.session_id) == latest.as_ref());
        self.save()
    }
}
pub fn event_data(activity: crate::Activity, session_id: String) -> SessionData {
    SessionData {
        activity,
        session_id,
        identity_source: "local_observed_session".into(),
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;
    use crate::{classify, Snapshot, Window};
    use chrono::{DateTime, Utc};
    struct TestBucket {
        agent: ureq::Agent,
        url: String,
    }
    impl Drop for TestBucket {
        fn drop(&mut self) {
            // This guard owns only the uniquely generated synthetic test bucket.
            let _ = self.agent.delete(&format!("{}?force=1", self.url)).call();
        }
    }
    #[test]
    #[ignore = "Requires explicit AW_PROTOCOL_SERVER and writes only a temporary synthetic bucket"]
    fn installed_aw_updates_one_session_and_recovers_lost_ack() {
        let server = crate::validate_server(
            &std::env::var("AW_PROTOCOL_SERVER").expect("Set local test server"),
        )
        .unwrap();
        let bucket = format!(
            "aw-watcher-teams-protocol-test-{}-{}",
            std::process::id(),
            Utc::now().timestamp_millis()
        );
        let url = server
            .join(&format!("api/0/buckets/{bucket}"))
            .unwrap()
            .to_string();
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(3))
            .redirects(0)
            .build();
        agent.post(&url).send_json(serde_json::json!({
            "client":"aw-watcher-teams-protocol-test","type":"call-evidence","hostname":"synthetic"
        })).unwrap();
        let guard = TestBucket {
            agent,
            url: url.clone(),
        };
        let dir = PathBuf::from("local").join(&bucket);
        fs::create_dir_all(&dir).unwrap();
        let checkpoint = dir.join("checkpoint.json");
        let activity = classify(
            &Snapshot {
                collector_ok: true,
                windows: vec![Window {
                    key: "synthetic".into(),
                    leave: true,
                    microphone: true,
                    title: Some("Synthetic planning".into()),
                    ..Default::default()
                }],
            },
            true,
        );
        let mut event = SessionEvent {
            timestamp: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            duration: 5.0,
            data: event_data(activity, "synthetic-session-1".into()),
        };
        let mut writer = AwWriter::new(url.clone(), checkpoint.clone()).unwrap();
        writer
            .configure_payload(
                crate::payload::PayloadConfig::extended(),
                true,
                Default::default(),
            )
            .unwrap();
        writer.queue(event.clone()).unwrap();
        writer.flush().unwrap();
        let id = writer.pending[0].id.unwrap();
        for seconds in [10.0, 15.0, 3600.0] {
            event.duration = seconds;
            writer.queue(event.clone()).unwrap();
            writer.flush().unwrap();
            assert_eq!(writer.pending[0].id, Some(id));
        }
        // Simulate successful insertion with its reply/checkpoint acknowledgement lost.
        writer.pending[0].id = None;
        writer.pending[0].dirty = true;
        writer.pending[0].may_exist = true;
        writer.save().unwrap();
        drop(writer);
        let mut writer = AwWriter::new(url.clone(), checkpoint.clone()).unwrap();
        writer
            .configure_payload(
                crate::payload::PayloadConfig::extended(),
                true,
                Default::default(),
            )
            .unwrap();
        writer.flush().unwrap();
        assert_eq!(writer.pending[0].id, Some(id));
        let records: Vec<Value> = guard
            .agent
            .get(&format!("{url}/events"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["duration"].as_f64(), Some(3600.0));
        assert_eq!(records[0]["data"]["title"], "Synthetic planning");
        writer.redact_pending_titles().unwrap();
        writer.flush().unwrap();
        let records: Vec<Value> = guard
            .agent
            .get(&format!("{url}/events"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(records.len(), 1);
        assert!(records[0]["data"]["meeting_title"].is_null());
        assert_eq!(records[0]["data"]["title"], "Call (title unavailable)");
        // Migrate the old caption shape and update only this event's title/metadata.
        writer
            .reformat_pending_titles(true, &Default::default())
            .unwrap();
        event.data.activity.schema_version = 2;
        event.data.activity.meeting_title =
            Some("Echo | Example Company Ltd. | self@example.invalid".into());
        event.data.activity.title = event.data.activity.meeting_title.clone().unwrap();
        writer.queue(event).unwrap();
        writer.flush().unwrap();
        let format = crate::title::TitleFormat {
            organization_map: std::collections::BTreeMap::from([(
                "Example Company Ltd.".into(),
                "Example".into(),
            )]),
            ..Default::default()
        };
        writer.reformat_pending_titles(true, &format).unwrap();
        writer.flush().unwrap();
        let records: Vec<Value> = guard
            .agent
            .get(&format!("{url}/events"))
            .call()
            .unwrap()
            .into_json()
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["id"].as_u64(), Some(id));
        assert_eq!(records[0]["duration"].as_f64(), Some(3600.0));
        assert_eq!(records[0]["data"]["title"], "Echo | Example");
        assert_eq!(records[0]["data"]["meeting_title"], "Echo");
        assert!(records[0]["data"].get("account").is_none());
        drop(guard);
    }
}
