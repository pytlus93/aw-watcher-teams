//! The bucket is custom: no official AW call-evidence data properties exist.
//! Strict therefore emits only the workspace-required title.
use crate::{Activity, SessionEvent};
use serde::{ser::SerializeMap, Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    #[default]
    Strict,
    Extended,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Fields {
    pub schema_version: bool,
    pub app: bool,
    pub meeting_title: bool,
    pub organization: bool,
    pub account: bool,
    pub call_kind: bool,
    pub state: bool,
    pub assessment: bool,
    pub evidence: bool,
    pub title_source: bool,
    pub session_id: bool,
    pub identity_source: bool,
}
impl Default for Fields {
    fn default() -> Self {
        Self {
            schema_version: true,
            app: true,
            meeting_title: true,
            organization: true,
            account: false,
            call_kind: true,
            state: true,
            assessment: true,
            evidence: true,
            title_source: true,
            session_id: true,
            identity_source: true,
        }
    }
}
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PayloadConfig {
    pub mode: Mode,
    pub fields: Fields,
}
impl PayloadConfig {
    pub fn extended() -> Self {
        Self {
            mode: Mode::Extended,
            ..Default::default()
        }
    }
    pub fn apply_privacy(&self, activity: &mut Activity) {
        if !self.fields.meeting_title {
            crate::reformat_activity(activity, false, &Default::default());
        } else {
            if !self.fields.organization {
                activity.organization = None;
            }
            if !self.fields.account {
                activity.account = None;
            }
        }
        if !self.fields.call_kind && activity.meeting_title.is_none() {
            activity.title = "Call (title unavailable)".into();
        }
    }
    pub fn title_format(
        &self,
        configured: &crate::title::TitleFormat,
    ) -> crate::title::TitleFormat {
        let mut format = configured.clone();
        format.include_organization &= self.fields.organization;
        format.include_account &= self.fields.account;
        format
    }
    pub fn collect_title(&self, configured: bool) -> bool {
        configured && self.fields.meeting_title
    }
    pub fn wire<'a>(&'a self, event: &'a SessionEvent, id: Option<u64>) -> WireEvent<'a> {
        WireEvent {
            event,
            id,
            config: self,
        }
    }
}
pub struct WireEvent<'a> {
    event: &'a SessionEvent,
    id: Option<u64>,
    config: &'a PayloadConfig,
}
impl Serialize for WireEvent<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        if let Some(id) = self.id {
            map.serialize_entry("id", &id)?;
        }
        map.serialize_entry("timestamp", &self.event.timestamp)?;
        map.serialize_entry("duration", &self.event.duration)?;
        map.serialize_entry(
            "data",
            &Data {
                event: self.event,
                config: self.config,
            },
        )?;
        map.end()
    }
}
struct Data<'a> {
    event: &'a SessionEvent,
    config: &'a PayloadConfig,
}
impl Serialize for Data<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        // No official properties for this custom bucket; required title is first.
        let a = &self.event.data.activity;
        map.serialize_entry("title", &a.title)?;
        if self.config.mode == Mode::Extended {
            let f = &self.config.fields;
            macro_rules! field {
                ($name:ident) => {
                    if f.$name {
                        map.serialize_entry(stringify!($name), &a.$name)?;
                    }
                };
            }
            macro_rules! optional {
                ($name:ident) => {
                    if f.$name {
                        if let Some(value) = &a.$name {
                            map.serialize_entry(stringify!($name), value)?;
                        }
                    }
                };
            }
            field!(schema_version);
            field!(app);
            optional!(meeting_title);
            optional!(organization);
            optional!(account);
            field!(call_kind);
            field!(state);
            field!(assessment);
            field!(evidence);
            optional!(title_source);
            if f.session_id {
                map.serialize_entry("session_id", &self.event.data.session_id)?;
            }
            if f.identity_source {
                map.serialize_entry("identity_source", &self.event.data.identity_source)?;
            }
        }
        map.end()
    }
}
