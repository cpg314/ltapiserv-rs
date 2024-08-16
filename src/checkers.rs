use std::collections::HashSet;
/// LanguageTool rules (using [`nlprule`]) and spell checking (using [`symspell`])
use std::fmt::Write;
use std::path::Path;

use anyhow::Context;
use bincode::Options;
use log::*;
use serde::{Deserialize, Serialize};

use crate::api;

/// Maximum edit distance for Symspell lookups
const MAX_EDIT_DISTANCE: usize = 3;

/// Convert an nlprule suggestion to an [`api::Match`]
fn suggestion_to_match(
    source: nlprule::types::Suggestion,
    annotation: &api::Annotations,
) -> api::Match {
    debug!("Grammar: {:#?}", source);

    let (start, end) =
        annotation.translate_span(source.span().start().char, source.span().end().char);

    api::Match {
        message: source.message().into(),
        replacements: source
            .replacements()
            .iter()
            .map(|r| r.clone().into())
            .collect(),
        offset: start,
        length: end - start,
        rule: api::Rule::from_id(source.source().into()),
        ..Default::default()
    }
}

#[derive(Serialize, Deserialize)]
pub struct Checkers {
    tokenizer: nlprule::Tokenizer,
    rules: nlprule::Rules,
    spelling: symspell::SymSpell<symspell::AsciiStringStrategy>,
    custom_dictionary: HashSet<String>,
    pub language: api::Language,
}
impl Checkers {
    /// Initialize from a tar.gz archive containing a {language_code}/ folder with:
    ///
    /// - [`nlprule`] data: rules.bin, tokenizer.bin
    /// - Dictionary for [`symspell`]: frequency_dict.txt
    pub fn from_archive(archive: &Path) -> anyhow::Result<Self> {
        Self::from_archive_bytes(&std::fs::read(archive)?)
    }
    /// Initialize from a folder containing the files documented in [`Checkers::from_archive`].
    pub fn from_folder(folder: &Path, language: api::Language) -> anyhow::Result<Self> {
        let rules = folder.join("rules.bin");
        let tokenizer = folder.join("tokenizer.bin");
        let dictionary = folder.join("frequency_dict.txt");
        for f in [&rules, &tokenizer, &dictionary] {
            anyhow::ensure!(f.exists(), "{:?} not found", f.file_name().unwrap());
        }
        let mut spelling = symspell::SymSpellBuilder::<symspell::AsciiStringStrategy>::default()
            .max_dictionary_edit_distance(MAX_EDIT_DISTANCE as i64)
            .build()?;
        spelling.load_dictionary(dictionary.to_str().unwrap(), 0, 1, " ");
        Ok(Self {
            tokenizer: nlprule::Tokenizer::new(tokenizer)?,
            rules: nlprule::Rules::new(rules)?,
            custom_dictionary: Default::default(),
            spelling,
            language,
        })
    }
    /// Add a custom dictionary (one word per line)
    pub fn add_dictionary(&mut self, filename: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(
            filename
                .parent()
                .context("Invalid dictionary path (should be a filename)")?,
        )?;
        if !filename.is_file() {
            std::fs::write(filename, "")
                .with_context(|| format!("Failed to initialize dictionary at {:?}", filename))?;
        } else {
            self.custom_dictionary.extend(
                std::fs::read_to_string(filename)?
                    .lines()
                    .flat_map(|l| l.split_ascii_whitespace())
                    .map(str::to_ascii_lowercase),
            );
        }
        info!(
            "Added dictionary {:?}, currently {} custom words",
            filename,
            self.custom_dictionary.len()
        );
        Ok(())
    }
    fn from_archive_bytes_impl(archive: &[u8]) -> anyhow::Result<Self> {
        info!("Subsequent initializations will be significantly faster.");
        // Unpack
        let archive = flate2::read::GzDecoder::new(archive);
        let mut archive = tar::Archive::new(archive);
        let tempdir = tempfile::tempdir()?;
        archive.unpack(tempdir.path())?;

        let language_ident = regex::Regex::new(r"^[a-z]+_[A-Z]+$").unwrap();
        let folders: Vec<_> = std::fs::read_dir(tempdir.path())?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map_or(false, |t| t.is_dir()))
            .filter(|e| {
                e.file_name()
                    .to_str()
                    .map_or(false, |filename| language_ident.is_match(filename))
            })
            .collect();
        let folder = match folders.first() {
            None => {
                anyhow::bail!("Found no language folders");
            }
            Some(_) if folders.len() > 1 => {
                anyhow::bail!("Found more than one language folder: {:?}", folders);
            }
            Some(f) => f.path(),
        };
        let language = api::Language::from_code(folder.file_name().unwrap().to_str().unwrap());
        Self::from_folder(&folder, language)
    }
    /// Initialize from tar.gz archive bytes with caching.
    pub fn from_archive_bytes(archive: &[u8]) -> anyhow::Result<Self> {
        // Try to read from cache
        let hash = blake3::hash(archive).to_hex();
        let cache = dirs::cache_dir()
            .ok_or_else(|| anyhow::anyhow!("Failed to create cache directory."))?
            .join(env!("CARGO_PKG_NAME"));
        std::fs::create_dir_all(&cache)?;
        let cache = cache.join(hash.as_str());
        info!("Data path is {:?}", cache);

        if cache.exists() {
            debug!("Reading from cache at {}", cache.display());
            match bincode::DefaultOptions::new()
                .with_fixint_encoding()
                .allow_trailing_bytes()
                .with_limit(200_000_000)
                .deserialize_from(std::io::BufReader::new(std::fs::File::open(&cache)?))
            {
                Ok(x) => {
                    return Ok(x);
                }
                Err(e) => {
                    warn!(
                        "Reading from cache at {} failed ({}), opening again",
                        cache.display(),
                        e
                    );
                }
            }
        }
        // Cache did not exist or failed parsing
        let out = Self::from_archive_bytes_impl(archive)?;
        debug!("Saving to cache");
        bincode::serialize_into(
            std::io::BufWriter::new(std::fs::File::create(&cache)?),
            &out,
        )?;
        debug!("Saved to cache at {}", cache.display());
        Ok(out)
    }
    /// Compute suggestions on a text
    pub fn suggest(&self, annotations: &api::Annotations) -> Vec<api::Match> {
        let mut suggestions = Vec::new();

        let text = annotations.text();
        for sentence in self.tokenizer.pipe(&text) {
            debug!("Processing sentence {:#?}", sentence);

            // Grammar suggestions from nlprule
            suggestions.extend(
                self.rules
                    .apply(&sentence)
                    .into_iter()
                    .map(|s| suggestion_to_match(s, annotations))
                    .filter(api::Match::filter),
            );
            // Repetitions
            let tokens = sentence.tokens();
            // Spelling and repetitions, processing the sentence token by token.
            for (i, token) in tokens.iter().enumerate() {
                let word = token.word();
                let word_str = unidecode::unidecode(word.as_str());

                let next_token = tokens.get(i + 1);
                if !word_str.chars().all(char::is_alphabetic) {
                    continue;
                }

                // Repetitions
                if let Some((start, end)) =
                    next_token.filter(|t| t.word() == token.word()).map(|t| {
                        annotations.translate_span(token.span().start().char, t.span().end().char)
                    })
                {
                    suggestions.push(api::Match {
                        rule: api::Rule::duplication(),
                        message: "Possible typo: you repeated a word".into(),
                        replacements: vec![token.into()],
                        offset: start,
                        length: end - start,
                        ..Default::default()
                    })
                }

                let word_str_lowercase = word_str.to_lowercase();
                // Spelling
                if !(self.custom_dictionary.contains(&word_str_lowercase)
                     || self.custom_dictionary.contains(word_str_lowercase.trim_end_matches('s'))
                )
                    // Skip short words
                    && word_str.chars().count() >= 3
                    // Skips all-caps words that are likely acronyms
                    && !word_str.chars().all(|x| x.is_uppercase())
                    // Skip if next character is an apostrophe (contraction/possession) for now
                    && next_token
                        .map_or(true, |c| c.word().as_str() != "'" && c.word().as_str() != "’")
                {
                    let mut results = self.spelling.lookup(
                        &word_str_lowercase,
                        symspell::Verbosity::Closest,
                        MAX_EDIT_DISTANCE as i64,
                    );
                    if results.len() == 1 && results[0].distance == 0 {
                        // Exists in dictionary
                        continue;
                    }
                    // Take 5 best results
                    results.reverse();
                    results.truncate(5);
                    debug!("Spelling: '{}' -> {:?}", word_str, results);
                    let mut message = "Possible spelling mistake.".to_string();
                    if let Some(result) = results.first() {
                        write!(message, " Did you mean {}?", result.term).unwrap();
                    }
                    // TODO: Restore case
                    let (start, end) = annotations
                        .translate_span(token.span().start().char, token.span().end().char);
                    suggestions.push(api::Match {
                        message,
                        rule: api::Rule::spelling(),
                        replacements: results
                            .into_iter()
                            .map(|result| result.term.into())
                            .collect(),
                        offset: start,
                        length: end - start,
                        ..Default::default()
                    });
                }
            }
        }
        debug!("{:#?}", suggestions);
        suggestions
    }
}
