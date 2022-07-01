/// Languagetool HTTP API
/// Inspired from https://languagetool.org/http-api/, which however doesn't seem to match exactly
/// what the server returns.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Language {
    name: String,
    code: String,
}
impl Language {
    pub fn from_code(code: &str) -> Self {
        Self {
            code: code.replace('_', "-"),
            name: "".into(),
        }
    }
}
impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}

impl PartialEq for Language {
    fn eq(&self, other: &Self) -> bool {
        self.code.to_lowercase() == other.code.to_lowercase()
    }
}

impl Default for Language {
    fn default() -> Self {
        Self {
            name: "English".into(),
            code: "en-US".into(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn language() {
        let l = Language::from_code("EN_US");
        assert_eq!(l, Language::default());
    }
}

#[derive(Debug, Deserialize)]
pub struct TextData {
    pub text: String,
}

/// API request. Either text or data need to be provided
#[derive(Debug, Deserialize)]
pub struct Request {
    text: Option<String>,
    data: Option<String>,
    language: String,
}
impl Request {
    pub fn language(&self) -> Language {
        if self.language == "auto" {
            return Default::default();
        }
        Language {
            code: self.language.clone(),
            name: "".into(),
        }
    }
    pub fn text(self) -> Option<String> {
        if let Some(text) = self.text {
            Some(text)
        } else if let Some(data) = self.data {
            let data: Option<TextData> = serde_json::from_str(&data).ok();
            data.map(|d| d.text)
        } else {
            None
        }
    }
}
#[derive(Serialize, Debug)]
pub struct Response {
    pub matches: Vec<Match>,
    pub language: LanguageResponse,
}
#[derive(Serialize, Debug)]
pub struct LanguageResponse {
    #[serde(flatten)]
    language: Language,
    detected_language: Language,
}
impl From<Language> for LanguageResponse {
    fn from(source: Language) -> Self {
        Self {
            language: source.clone(),
            detected_language: source,
        }
    }
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MatchType {
    pub type_name: String,
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RuleCategory {
    id: String,
    name: String,
}
#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    id: String,
    sub_id: usize,
    description: String,
    issue_type: String,
    urls: Option<Vec<String>>,
    category: RuleCategory,
    is_premium: bool,
}
impl Rule {
    pub fn spelling() -> Self {
        Self {
            // This will get rendered by the browser extension as a spelling error
            id: "MORFOLOGIK_RULE".into(),
            ..Default::default()
        }
    }
    pub fn style() -> Self {
        Self {
            // This will get rendered by the browser extension as a style hint
            issue_type: "style".into(),
            ..Default::default()
        }
    }
    pub fn duplication() -> Self {
        Self {
            issue_type: "duplication".into(),
            ..Default::default()
        }
    }

    pub fn from_id(id: String) -> Self {
        Self {
            id,
            ..Default::default()
        }
    }
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Replacement {
    value: String,
    short_description: Option<String>,
}
impl From<&nlprule::types::Token<'_>> for Replacement {
    fn from(token: &nlprule::types::Token) -> Self {
        Self {
            value: token.word().as_str().to_string(),
            short_description: None,
        }
    }
}
impl From<String> for Replacement {
    fn from(value: String) -> Self {
        Self {
            value,
            short_description: None,
        }
    }
}

#[derive(Serialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub message: String,
    pub short_message: String,
    pub offset: usize,
    pub length: usize,
    pub replacements: Vec<Replacement>,
    pub sentence: String,
    pub context_for_sure_match: usize,
    pub ignore_for_incomplete_sentence: bool,
    pub r#type: MatchType,
    pub rule: Rule,
}
