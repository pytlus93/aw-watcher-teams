use aw_watcher_teams::{aw::AwWriter, classify, SessionEvent, Snapshot, Window};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};
#[derive(Default)]
struct State {
    bucket: bool,
    incompatible: bool,
    events: BTreeMap<u64, Value>,
    next: u64,
    creation_error: Option<u16>,
    event_error: Option<u16>,
    lost_reply: bool,
    creates: Vec<u16>,
    writes: usize,
}
struct Server {
    addr: SocketAddr,
    state: Arc<Mutex<State>>,
    worker: Option<(Arc<AtomicBool>, JoinHandle<()>)>,
}
impl Server {
    fn offline() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        drop(listener);
        Self {
            addr,
            state: Arc::new(Mutex::new(State::default())),
            worker: None,
        }
    }
    fn start(&mut self) {
        let listener = TcpListener::bind(self.addr).unwrap();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let state = self.state.clone();
        let worker = thread::spawn(move || {
            while !flag.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => handle(stream, &state),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
        });
        self.worker = Some((stop, worker));
    }
    fn stop(&mut self) {
        if let Some((flag, worker)) = self.worker.take() {
            flag.store(true, Ordering::SeqCst);
            worker.join().unwrap();
        }
    }
    fn url(&self) -> String {
        format!("http://{}/api/0/buckets/teams-test", self.addr)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}
fn handle(mut stream: TcpStream, state: &Arc<Mutex<State>>) {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request = String::new();
    if reader.read_line(&mut request).is_err() {
        return;
    }
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return;
        }
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap();
        }
    }
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let data: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let words: Vec<_> = request.split_whitespace().collect();
    if words.len() < 2 {
        return;
    }
    let method = words[0];
    let path = words[1].split('?').next().unwrap();
    let mut s = state.lock().unwrap();
    let (status, response) = if path.ends_with("/events") && method == "POST" {
        if !s.bucket {
            (404, json!({}))
        } else if let Some(status) = s.event_error {
            (status, json!({"id":1}))
        } else if !data.is_array() || data.as_array().unwrap().len() != 1 {
            (422, json!({}))
        } else {
            let mut event = data[0].clone();
            let id = if let Some(id) = event["id"].as_u64() {
                id
            } else {
                s.next += 1;
                s.next
            };
            event["id"] = id.into();
            s.events.insert(id, event.clone());
            s.writes += 1;
            if s.lost_reply {
                s.lost_reply = false;
                return;
            }
            (200, json!([event]))
        }
    } else if path.ends_with("/events") && method == "GET" {
        if s.bucket {
            (200, json!(s.events.values().cloned().collect::<Vec<_>>()))
        } else {
            (404, json!({}))
        }
    } else if method == "POST" {
        let status = if let Some(status) = s.creation_error {
            status
        } else if s.bucket {
            304
        } else {
            s.bucket = true;
            200
        };
        s.creates.push(status);
        (status, json!({}))
    } else if method == "GET" {
        if s.bucket {
            (
                200,
                json!({"type":if s.incompatible{"currentwindow"}else{"call-evidence"}}),
            )
        } else {
            (404, json!({}))
        }
    } else {
        (404, json!({}))
    };
    let bytes = if status == 304 {
        Vec::new()
    } else {
        serde_json::to_vec(&response).unwrap()
    };
    drop(s);
    let header=format!("HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len());
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(&bytes);
}
fn event(session: &str, duration: f64) -> SessionEvent {
    SessionEvent {
        timestamp: DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
        duration,
        data: aw_watcher_teams::aw::event_data(
            classify(
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
            ),
            session.into(),
        ),
    }
}
fn checkpoint(name: &str) -> PathBuf {
    let dir = PathBuf::from("local").join(format!(
        "transport-{name}-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir.join("checkpoint.json")
}
#[test]
fn existing_304_and_producer_restart_resume_updates() {
    let mut server = Server::offline();
    server.start();
    let path = checkpoint("restart");
    let mut writer = AwWriter::new(server.url(), path.clone()).unwrap();
    writer.queue(event("one", 5.0)).unwrap();
    writer.flush().unwrap();
    drop(writer);
    let mut writer = AwWriter::new(server.url(), path).unwrap();
    writer.queue(event("one", 15.0)).unwrap();
    writer.flush().unwrap();
    let state = server.state.lock().unwrap();
    assert_eq!(state.creates, vec![200, 304]);
    assert_eq!(state.events.len(), 1);
    assert_eq!(state.events[&1]["duration"], 15.0);
    assert_eq!(state.writes, 2);
}
#[test]
fn aw_starts_late_and_original_observation_is_delivered() {
    let mut server = Server::offline();
    let mut writer = AwWriter::new(server.url(), checkpoint("late")).unwrap();
    let e = event("one", 5.0);
    writer.queue(e.clone()).unwrap();
    assert!(writer.flush().is_err());
    server.start();
    writer.flush().unwrap();
    let state = server.state.lock().unwrap();
    assert_eq!(state.events.len(), 1);
    assert_eq!(state.events[&1]["timestamp"], json!(e.timestamp));
    assert_eq!(state.events[&1]["duration"], 5.0);
}
#[test]
fn socket_outage_and_server_restart_resume_actual_updates() {
    let mut server = Server::offline();
    server.start();
    let mut writer = AwWriter::new(server.url(), checkpoint("outage")).unwrap();
    writer.queue(event("one", 5.0)).unwrap();
    writer.flush().unwrap();
    server.stop();
    writer.queue(event("one", 10.0)).unwrap();
    assert!(writer.flush().is_err());
    server.start();
    writer.flush().unwrap();
    writer.queue(event("one", 20.0)).unwrap();
    writer.flush().unwrap();
    let state = server.state.lock().unwrap();
    assert_eq!(state.events.len(), 1);
    assert_eq!(state.events[&1]["duration"], 20.0);
    assert_eq!(state.writes, 3);
    assert!(state.creates.contains(&304));
}
#[test]
fn recreated_bucket_clears_stale_ids_and_preserves_timestamps() {
    let mut server = Server::offline();
    server.start();
    let mut writer = AwWriter::new(server.url(), checkpoint("recreate")).unwrap();
    writer.queue(event("one", 5.0)).unwrap();
    writer.flush().unwrap();
    {
        let mut s = server.state.lock().unwrap();
        s.bucket = false;
        s.events.clear();
        s.next = 90;
    }
    writer.queue(event("one", 10.0)).unwrap();
    assert!(writer.flush().is_err());
    writer.flush().unwrap();
    let state = server.state.lock().unwrap();
    assert_eq!(state.creates, vec![200, 200]);
    assert_eq!(state.events.len(), 1);
    assert_eq!(state.events[&91]["duration"], 10.0);
    assert_eq!(
        state.events[&91]["timestamp"],
        json!(event("one", 10.0).timestamp)
    );
}
#[test]
fn genuine_creation_failure_and_event_304_are_not_success() {
    let mut server = Server::offline();
    server.start();
    server.state.lock().unwrap().creation_error = Some(500);
    let mut writer = AwWriter::new(server.url(), checkpoint("failures")).unwrap();
    writer.queue(event("one", 5.0)).unwrap();
    assert!(writer.flush().is_err());
    assert_eq!(server.state.lock().unwrap().writes, 0);
    server.state.lock().unwrap().creation_error = None;
    writer.flush().unwrap();
    server.state.lock().unwrap().event_error = Some(304);
    writer.queue(event("one", 10.0)).unwrap();
    assert!(writer.flush().is_err());
    server.state.lock().unwrap().event_error = None;
    writer.flush().unwrap();
    assert_eq!(server.state.lock().unwrap().events[&1]["duration"], 10.0);
}
#[test]
fn incompatible_existing_bucket_receives_no_events() {
    let mut server = Server::offline();
    server.start();
    {
        let mut s = server.state.lock().unwrap();
        s.bucket = true;
        s.incompatible = true;
    }
    let mut writer = AwWriter::new(server.url(), checkpoint("type")).unwrap();
    writer.queue(event("one", 5.0)).unwrap();
    assert!(writer.flush().is_err());
    assert_eq!(server.state.lock().unwrap().writes, 0);
}
#[test]
fn lost_reply_does_not_duplicate_session() {
    let mut server = Server::offline();
    server.start();
    server.state.lock().unwrap().lost_reply = true;
    let mut writer = AwWriter::new(server.url(), checkpoint("lost-reply")).unwrap();
    writer.queue(event("one", 5.0)).unwrap();
    assert!(writer.flush().is_err());
    writer.flush().unwrap();
    let s = server.state.lock().unwrap();
    assert_eq!(s.events.len(), 1);
    assert_eq!(s.events[&1]["duration"], 5.0);
}
#[test]
fn reused_id_after_database_restore_does_not_overwrite_other_session() {
    let mut server = Server::offline();
    server.start();
    let path = checkpoint("id-ownership");
    let mut writer = AwWriter::new(server.url(), path.clone()).unwrap();
    writer.queue(event("one", 5.0)).unwrap();
    writer.flush().unwrap();
    drop(writer);
    {
        let mut s = server.state.lock().unwrap();
        s.events.clear();
        let mut other = serde_json::to_value(event("unrelated", 12.0)).unwrap();
        other["id"] = 1.into();
        s.events.insert(1, other);
    }
    let mut writer = AwWriter::new(server.url(), path).unwrap();
    writer.queue(event("one", 10.0)).unwrap();
    writer.flush().unwrap();
    let s = server.state.lock().unwrap();
    assert_eq!(s.events.len(), 2);
    assert_eq!(s.events[&1]["data"]["session_id"], "unrelated");
    assert_eq!(s.events[&2]["data"]["session_id"], "one");
}

#[test]
fn corrupt_checkpoint_is_preserved_and_startup_recovers() {
    let dir = std::env::temp_dir().join(format!(
        "teams-corruption-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap()
    ));
    fs::create_dir_all(&dir).unwrap();
    let checkpoint = dir.join("session-delivery.json");
    let damaged = vec![0u8; 524];
    fs::write(&checkpoint, &damaged).unwrap();
    let mut writer = AwWriter::new(
        "http://127.0.0.1:5600/api/0/buckets/synthetic".into(),
        checkpoint.clone(),
    )
    .unwrap();
    let files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|f| f.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(fs::read(&files[0]).unwrap(), damaged);
    writer
        .reformat_pending_titles(false, &Default::default())
        .unwrap();
    assert_eq!(fs::read(&checkpoint).unwrap(), b"[]");
    drop(writer);
    assert!(AwWriter::new(
        "http://127.0.0.1:5600/api/0/buckets/synthetic".into(),
        checkpoint
    )
    .is_ok());
    fs::remove_dir_all(dir).unwrap();
}
