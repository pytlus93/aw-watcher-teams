# Teams event data contract (0.4)

This is an AW-managed module using its existing custom bucket type **call-evidence**.
It is not currentwindow, afkstatus, or an official Teams event type. AW's documented
generic contract requires an event timestamp, duration and object-valued data; there
are **no official data properties for call-evidence**. Strict mode therefore sends
only the workspace-required data.title. This workspace addition is not an upstream
standard property for this custom bucket.

Event envelope: timestamp is the first matched observation in UTC/RFC3339; duration
is a positive number of seconds through the last matched observation; id is the AW
integer identifier on updates. No zero-duration initial sample is sent.
The metadata required to create the bucket is client=aw-watcher-teams,
type=call-evidence and hostname=<local computer>; these are bucket properties, not
optional event data. No foreground-attention or provider-authenticated attendance
contract is claimed.

## Modes and defaults

Config payload.mode is strict (default/missing) or extended. In extended mode every
property below except title has its own payload.fields Boolean. Unknown names,
invalid modes and non-Boolean choices are rejected. Optional unavailable properties
are absent, never null, on the wire.

The field switches for meeting_title, organization and account are privacy
permissions as well as extension selectors: a false choice prevents that context
from appearing in new titles and queued data even in strict mode. A true choice
permits its use in the required title in strict mode, without adding its separate
property. collect_meeting_title=false disables all caption collection regardless of
those switches. title_format.include_organization and include_account further restrict
both structured context and title context; enabling payload.fields.account alone
does not bypass title_format.include_account=false. Both must be explicitly true
for account context. Organization mappings shorten title labels; the permitted
structured organization remains canonical.

All other optional switches control emitted extensions. The worker's private
delivery checkpoint still needs operational tracking state and local interval
identity for retries even when those extensions are not emitted; it is not an AW
export. Disabled caption values are removed from that checkpoint on configuration
application. No raw whole caption, window key/HWND, control-name list, chat/message,
roster, credentials, microphone/camera recordings, or SQL is emitted.

| data field | Type / synthetic example | Meaning and applicability | Extended default | Privacy / unavailable |
| --- | --- | --- | --- | --- |
| title | string: Review \| Example | Required readable topic/context; every emitted interval | Always | Caption-derived only with collection/field permissions; otherwise Call (title unavailable), or Meeting (title unavailable) when kind is permitted |
| schema_version | integer: 3 | Version of the extended activity semantics | On | No private content; always available |
| app | string: MS Teams | Source application label; not a title prefix | On | No private content; always available |
| meeting_title | string: Review | Topic of a qualifying window; does not prove meeting vs call | On, collection Off | Potentially personal/customer content; absent without permission/context |
| organization | string: Example Ltd. | Permitted canonical organization suffix; mappings affect title only | On, collection Off | Organizational context; absent when hidden, unrecognized or unavailable |
| account | string: self@example.invalid | Recognized current-account suffix; not a participant or meeting ID | Off | Personal email; requires all explicit permissions; absent otherwise |
| call_kind | string: meeting or unknown | meeting only with recognized meeting-info UI control; otherwise unknown | On | Inferred class only, not provider state; unknown is preserved; disabling it makes missing-topic title neutral Call |
| state | string: joined_candidate | Detection state; only joined_candidate intervals are emitted | On | UI inference; prejoin/unknown/ambiguous are not emitted as events |
| assessment | string: ui_inferred | Provenance/strength of detection | On | Not proof of attendance or active work; always available |
| evidence | string: uia_control_set | Narrow category of qualifying controls | On | No actual control texts; only category on emitted events |
| title_source | string: qualified_window_title | Caption/qualified UIA root provenance (not which fallback was used) | On, collection Off | Absent without a permitted usable topic |
| session_id | string: local-test-1 | Local observed interval identity | On | Machine-local correlation; never a Teams/provider/conversation ID; absent in strict |
| identity_source | string: local_observed_session | Explains local session_id provenance | On | Always available when enabled; no provider identity claim |

An email legitimately written inside a topic is topic content, not recognized as the
account suffix, and is not guaranteed to be removed by the account option. Disable
topic capture when all personal caption text must be excluded.

## Settings and migration

Run the installed module with --settings, or use its Settings.lnk after installing
0.4. This launches the module's embedded Windows dialog, not another watcher.
It exposes mode, every extra field, caption permission, and title-context switches.
Existing organization mappings, server and timing are retained. Hosts supporting module actions protocol 1 expose settings via the matching sidecar; older trays retain manual --settings access. Mapping/separator and
server/timing changes still use config.json. Restart **only through AW's Modules
menu** after saving; directly launching a worker makes the tray lose ownership.

--check-config <path> validates a configuration without collecting, opening UI or
writing it. New configs contain payload. Old files without it use strict and
account-off; no new private metadata is silently enabled. Previously explicit
account permission additionally needs the new account-field switch in 0.4.
Removing extensions from the latest retained event is queued on startup, using
the same AW ID/timestamp/duration. Older AW history is not bulk rewritten: old
values in earlier events/exports remain until the user manages that history.
Configuration changes are not a retrospective erasure promise.

Example (other config properties are still required):

~~~json
"payload": {
  "mode": "extended",
  "fields": {
    "schema_version": true,
    "app": true,
    "meeting_title": true,
    "organization": true,
    "account": false,
    "call_kind": true,
    "state": true,
    "assessment": true,
    "evidence": true,
    "title_source": true,
    "session_id": true,
    "identity_source": true
  }
}
~~~

## Delivery identity and order

Extended mode can expose the internal session ID when selected. Strict mode and
ID-disabled extended mode reconcile a lost acknowledgement by the original event
start timestamp at AW's millisecond precision within this module's own bucket.
An existing different exposed session ID is never treated as matching just because
its timestamp agrees. Multiple matches fail closed rather than overwriting one.
There is no fabricated positive duration or inferred interval across a collection
failure, lock, sleep/large clock gap, producer crash or restart.

Request JSON serializes envelope properties first, then data.title, then selected
extensions in the documented table order. JSON order has no semantic meaning.
Actual isolated Python 0.13.2 API retained title before schema_version in the test;
Rust 0.13.2 reordered it. Stored properties and export omissions were checked on
both actual servers; raw database key order and GUI tooltip ordering were not.
A custom bucket title is not evidence of built-in timeline/chart/tooltip support.
Any guaranteed display order belongs to AW's presentation layer.

Sources: [AW data model](https://docs.activitywatch.net/en/latest/buckets-and-events.html),
[REST API](https://docs.activitywatch.net/en/latest/api/rest.html),
[watchers](https://docs.activitywatch.net/en/latest/watchers.html),
[writing watchers](https://docs.activitywatch.net/en/latest/examples/writing-watchers.html).
The installed Python server's /api/swagger.json describes generic Event properties,
not a call-evidence data schema. Documentation and observed implementation/version
behavior are recorded separately in VALIDATION.md.
