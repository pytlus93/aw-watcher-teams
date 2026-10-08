use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// These switches apply to display AND emitted metadata. No raw account caption is stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TitleFormat {
    pub include_organization: bool,
    pub include_account: bool,
    pub organization_map: BTreeMap<String, String>,
    pub separator: String,
}
impl Default for TitleFormat {
    fn default() -> Self {
        Self {
            include_organization: true,
            include_account: false,
            organization_map: BTreeMap::new(),
            separator: " | ".into(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    pub topic: String,
    pub organization: Option<String>,
    pub account: Option<String>,
}
impl TitleFormat {
    pub fn validate(&self) -> Result<(), String> {
        if self.separator.chars().any(char::is_control) || self.separator.chars().count() > 16 {
            return Err("Title separator must be at most 16 printable characters".into());
        }
        let mut keys = std::collections::BTreeSet::new();
        for (key, value) in &self.organization_map {
            if key.trim().is_empty()
                || key.chars().any(char::is_control)
                || value.chars().any(char::is_control)
                || value.chars().count() > 120
                || !keys.insert(key.trim().to_lowercase())
            {
                return Err(
                    "Organization mappings must have unique nonempty names and printable labels"
                        .into(),
                );
            }
        }
        Ok(())
    }
    pub fn organization_label(&self, value: Option<&str>) -> Option<String> {
        if !self.include_organization {
            return None;
        }
        let value = value?.trim();
        let mapped = self
            .organization_map
            .iter()
            .find(|(key, _)| key.trim().to_lowercase() == value.to_lowercase())
            .map(|(_, label)| label.trim())
            .unwrap_or(value);
        let label: String = mapped
            .chars()
            .filter(|c| !c.is_control())
            .take(120)
            .collect();
        if label.is_empty() {
            None
        } else {
            Some(label)
        }
    }
    pub fn apply(&self, mut context: Context) -> Context {
        // Retain permitted canonical organization metadata so mappings do not chain on replay.
        context.organization = if self
            .organization_label(context.organization.as_deref())
            .is_some()
        {
            context.organization.map(|org| {
                org.trim()
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(240)
                    .collect()
            })
        } else {
            None
        };
        if !self.include_account {
            context.account = None;
        }
        context
    }
    pub fn display(&self, context: &Context) -> String {
        let mut parts = vec![context.topic.as_str()];
        let organization = self.organization_label(context.organization.as_deref());
        if let Some(org) = &organization {
            parts.push(org);
        }
        if self.include_account {
            if let Some(account) = &context.account {
                parts.push(account);
            }
        }
        parts.join(&self.separator)
    }
    pub fn parse(&self, raw: Option<&str>) -> Option<Context> {
        let cleaned = crate::readable_title(raw)?;
        let mut parts: Vec<&str> = cleaned.split('|').map(str::trim).collect();
        let mut organization = None;
        let mut account = None;
        // Validated Teams shape: topic | organization | account-email | Microsoft Teams.
        // Only treat an email as an account suffix when there is a preceding topic.
        if parts.len() >= 2 && parts.last().is_some_and(|last| is_account(last)) {
            account = parts.pop().map(str::to_owned);
            if parts.len() >= 2 {
                organization = parts.pop().map(str::to_owned);
            }
        } else if parts.len() >= 2
            && parts.last().is_some_and(|last| {
                self.organization_map
                    .keys()
                    .any(|key| key.trim().to_lowercase() == last.to_lowercase())
            })
        {
            // Explicit map resolves organization captions with no account suffix.
            organization = parts.pop().map(str::to_owned);
        }
        let topic: String = parts.join(" | ").trim().chars().take(240).collect();
        if topic.is_empty() {
            return None;
        }
        Some(self.apply(Context {
            topic,
            organization,
            account,
        }))
    }
}
fn is_account(value: &str) -> bool {
    let mut parts = value.split('@');
    matches!((parts.next(),parts.next(),parts.next()),(Some(user),Some(domain),None)
        if !user.is_empty() && !domain.is_empty() && !value.chars().any(char::is_whitespace))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn policy() -> TitleFormat {
        TitleFormat {
            organization_map: BTreeMap::from([("Example Company Ltd.".into(), "Example".into())]),
            ..Default::default()
        }
    }
    fn caption() -> &'static str {
        "Echo | Example Company Ltd. | synthetic@example.invalid | Microsoft Teams"
    }
    #[test]
    fn shortened_org_and_hidden_account() {
        let p = policy();
        let context = p.parse(Some(caption())).unwrap();
        assert_eq!(context.topic, "Echo");
        assert_eq!(
            context.organization.as_deref(),
            Some("Example Company Ltd.")
        );
        assert_eq!(context.account, None);
        assert_eq!(p.display(&context), "Echo | Example");
    }
    #[test]
    fn topic_only() {
        let p = TitleFormat {
            include_organization: false,
            ..policy()
        };
        let c = p.parse(Some(caption())).unwrap();
        assert_eq!(p.display(&c), "Echo");
        assert_eq!(c.organization, None);
    }
    #[test]
    fn explicit_account_opt_in() {
        let p = TitleFormat {
            include_account: true,
            ..policy()
        };
        assert_eq!(
            p.display(&p.parse(Some(caption())).unwrap()),
            "Echo | Example | synthetic@example.invalid"
        );
    }
    #[test]
    fn custom_separator() {
        let p = TitleFormat {
            separator: " — ".into(),
            ..policy()
        };
        assert_eq!(
            p.display(&p.parse(Some(caption())).unwrap()),
            "Echo — Example"
        );
    }
    #[test]
    fn empty_mapping_omits_org() {
        let p = TitleFormat {
            organization_map: BTreeMap::from([("Example Company Ltd.".into(), "".into())]),
            ..Default::default()
        };
        assert_eq!(p.display(&p.parse(Some(caption())).unwrap()), "Echo");
    }
    #[test]
    fn unknown_org_preserved() {
        let p = TitleFormat::default();
        assert_eq!(
            p.display(&p.parse(Some(caption())).unwrap()),
            "Echo | Example Company Ltd."
        );
    }
    #[test]
    fn mapping_case_insensitive() {
        let p = policy();
        let c = p
            .parse(Some(
                "Echo | example company ltd. | synthetic@example.invalid",
            ))
            .unwrap();
        assert_eq!(p.display(&c), "Echo | Example");
    }
    #[test]
    fn topic_pipes_preserved() {
        let p = policy();
        let c=p.parse(Some("Review | phase two | Example Company Ltd. | synthetic@example.invalid | Microsoft Teams")).unwrap();
        assert_eq!(c.topic, "Review | phase two");
        assert_eq!(p.display(&c), "Review | phase two | Example");
    }
    #[test]
    fn mapped_org_without_account() {
        let p = policy();
        assert_eq!(
            p.display(&p.parse(Some("Review | Example Company Ltd.")).unwrap()),
            "Review | Example"
        );
    }
    #[test]
    fn unmapped_pipe_is_topic_not_guessed_org() {
        let p = TitleFormat {
            include_organization: false,
            ..Default::default()
        };
        assert_eq!(
            p.display(&p.parse(Some("Review | phase two")).unwrap()),
            "Review | phase two"
        );
    }
    #[test]
    fn email_inside_topic_not_removed() {
        let p = policy();
        let c=p.parse(Some("Contact synthetic@example.invalid tomorrow | Example Company Ltd. | self@example.invalid")).unwrap();
        assert_eq!(c.topic, "Contact synthetic@example.invalid tomorrow");
        assert_eq!(c.account, None);
    }
    #[test]
    fn old_config_defaults_hide_account() {
        let p: TitleFormat = serde_json::from_str("{}").unwrap();
        assert!(!p.include_account);
        assert!(p.include_organization);
    }
    #[test]
    fn bad_config_rejected() {
        let p = TitleFormat {
            separator: "\n".into(),
            ..policy()
        };
        assert!(p.validate().is_err());
        assert!(serde_json::from_str::<TitleFormat>(r#"{"include_acount":true}"#).is_err());
    }
}
