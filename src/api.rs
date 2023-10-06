/// Languagetool HTTP API
/// Inspired from https://languagetool.org/http-api/, which however doesn't seem to match exactly
/// what the server returns.
use anyhow::Context;
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
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum AnnotationElement {
    Text {
        text: String,
    },
    Markup {
        markup: String,

        // Interpret the markup as this string for analysis.
        // E.g. "\n\n" when `markup` is `<p>`
        interpret_as: Option<String>,
    },
}

impl AnnotationElement {
    pub fn text(&self) -> &str {
        match self {
            AnnotationElement::Text { text } => text,
            // All whitespace markup should really be handled as what it is to preserve context...
            AnnotationElement::Markup { markup, .. }
                if !markup.is_empty() && markup.trim().is_empty() =>
            {
                markup
            }
            AnnotationElement::Markup { interpret_as, .. } => interpret_as.as_deref().unwrap_or(""),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Annotations {
    pub annotation: Vec<AnnotationElement>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Data {
    Text { text: String },
    Annotations(Annotations),
}

impl Annotations {
    /// Obtains the text behind the annotations, removing markup
    pub fn text(&self) -> String {
        self.annotation
            .iter()
            .map(|v| v.text().to_owned())
            .reduce(|mut acc, v| {
                acc.push_str(&v);
                acc
            })
            .unwrap_or_default()
    }

    /// Obtains the length of the text contained in the annotations
    pub fn text_len(&self) -> usize {
        self.annotation
            .iter()
            .map(|v| match v {
                AnnotationElement::Text { text } => text.len(),
                // All whitespace markup should really be handled as what it is to preserve context...
                AnnotationElement::Markup { markup, .. }
                    if !markup.is_empty() && markup.trim().is_empty() =>
                {
                    markup.len()
                }
                AnnotationElement::Markup { interpret_as, .. } => {
                    interpret_as.as_deref().unwrap_or("").len()
                }
            })
            .sum()
    }

    /// Translate a textual span into the span of the text containing markup
    pub fn translate_span(&self, start: usize, end: usize) -> (usize, usize) {
        let mut text_offset = 0;
        let mut markup_offset = 0;

        let mut mapped_start: Option<usize> = None;
        let mut mapped_end: Option<usize> = None;

        for annotation in &self.annotation {
            let (fragment_text_len, fragment_markup_len) = match annotation {
                AnnotationElement::Text { text } => (text.len(), text.len()),
                AnnotationElement::Markup { markup, .. }
                    if !markup.is_empty() && markup.trim().is_empty() =>
                {
                    (markup.len(), markup.len())
                }
                AnnotationElement::Markup {
                    markup,
                    interpret_as,
                } => (interpret_as.as_deref().unwrap_or("").len(), markup.len()),
            };

            let try_set_if_none = |mapped: &mut Option<usize>, original: usize| {
                if mapped.is_none()
                    && (original >= text_offset)
                    && (original < text_offset + fragment_text_len)
                {
                    *mapped = Some(markup_offset + (original - text_offset));
                }
            };
            try_set_if_none(&mut mapped_start, start);
            try_set_if_none(&mut mapped_end, end);

            text_offset += fragment_text_len;
            markup_offset += fragment_markup_len;
        }
        (
            mapped_start.unwrap_or(0),
            mapped_end.unwrap_or(markup_offset),
        )
    }
}

/// API request. Either text or data need to be provided
#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    text: Option<String>,
    data: Option<String>,
    language: String,
}

impl Request {
    pub fn new<S: Into<String>>(text: String, language: S) -> Self {
        Self {
            text: Some(text),
            data: None,
            language: language.into(),
        }
    }
    pub fn language(&self) -> Language {
        if self.language == "auto" {
            return Default::default();
        }
        Language {
            code: self.language.clone(),
            name: "".into(),
        }
    }

    pub fn annotations(&self) -> anyhow::Result<Annotations> {
        if let Some(text) = &self.text {
            Ok(Annotations {
                annotation: vec![AnnotationElement::Text {
                    text: text.to_string(),
                }],
            })
        } else if let Some(data) = &self.data {
            let data: Data = serde_json::from_str(data)
                .with_context(|| format!("Unexpected json contents in `data`: {:?}", data))?;

            Ok(match data {
                Data::Annotations(annotations) => annotations,
                Data::Text { text } => Annotations {
                    annotation: vec![AnnotationElement::Text { text }],
                },
            })
        } else {
            Err(anyhow::anyhow!("Neither `text` nor `data` are valid"))
        }
    }
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    pub matches: Vec<Match>,
    pub language: LanguageResponse,
}
#[derive(Serialize, Deserialize, Debug)]
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

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct MatchType {
    pub type_name: String,
}

#[derive(Serialize, Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RuleCategory {
    id: String,
    name: String,
}
#[derive(Serialize, Deserialize, Default, Debug)]
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
    pub fn is_spelling(&self) -> bool {
        self.id == "MORFOLOGIK_RULE"
    }
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

#[derive(Serialize, Deserialize, Default, Debug)]
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

#[derive(Serialize, Deserialize, Default, Debug)]
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

impl Match {
    /// Remove likely false positives
    pub fn filter(&self) -> bool {
        let rule = &self.rule.id;
        [
            "TYPOGRAPHY/EN_QUOTES",
            // This triggers on lists
            "PUNCTUATION/DASH_RULE",
        ]
        .iter()
        .all(|r| !rule.starts_with(r))
    }
}
