# Meeting title formatting



Add title_format to config.json (other existing settings stay as they are):

```json
"title_format": {
  "include_organization": true,
  "include_account": false,
  "organization_map": {
    "Example Company Ltd.": "Example",
    "Example Subsidiary": "Subsidiary"
  },
  "separator": " | "
}
```

For a validated caption like
Echo | Example Company Ltd. | self@example.invalid | Microsoft Teams
the displayed title becomes Echo | Example.
Set include_organization to false for Echo alone. Set include_account to true only
if you deliberately want the account suffix. separator may be changed to " — ".
Mapping keys match full organization names without case sensitivity. Unmapped
organizations retain their name. An empty mapped value omits that organization.
Mappings are literal names, not regular expressions.

The topic is preserved (including pipes inside the topic). The trailing email is
classified as the current account only in a caption with a preceding topic. An
organization without an email suffix is recognized only when it explicitly matches
a mapping key; unknown pipe-separated topic text is not guessed to be an organization.
These rules are based on the observed desktop caption format; new Teams formats
may need another parser profile.

When selected in extended mode, data.meeting_title stores only the topic; data.organization retains the permitted
canonical organization so replay does not chain aliases. The shortened label is used
in data.title. Disabled account/organization fields are omitted from emitted metadata
as well as titles. Account collection is off by default; email in an actual meeting
topic is not indiscriminately removed. collect_meeting_title=false still disables all
caption metadata. The collector reads the permitted whole caption transiently to parse
its parts, then discards the unused fields; no raw caption field is persisted.
See [the data contract](DATA-CONTRACT.md) for emitted fields.

After editing settings, stop/start this module through AW's Modules menu (or restart AW).
Formatting of the latest session retained in the delivery checkpoint is reapplied in
place on startup, preserving its AW event ID, session ID and duration. Earlier history
is not bulk rewritten. Turning a field back on cannot recover previously discarded
historical metadata; later observations can supply it again.
