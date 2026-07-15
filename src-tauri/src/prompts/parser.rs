use std::collections::{BTreeMap, BTreeSet};

use saphyr::{LoadableYamlNode, Yaml};
use saphyr_parser::{Event, Parser};
use serde_json::{Number, Value};
use sha2::{Digest, Sha256};

use crate::errors::{AppError, AppResult};

use super::types::{ParsedPrompt, PromptMetadata};

pub const MAX_PROMPT_BYTES: usize = 1024 * 1024;
const MAX_FRONT_MATTER_BYTES: usize = 64 * 1024;
const MAX_YAML_DEPTH: usize = 16;
const MAX_YAML_EVENTS: usize = 2_048;
const MAX_METADATA_KEYS: usize = 64;
const MAX_LIST_ITEMS: usize = 32;

pub(crate) fn parse_prompt_document(
    document: &str,
    fallback_name: &str,
) -> AppResult<ParsedPrompt> {
    let document = document.strip_prefix('\u{feff}').unwrap_or(document);
    if document.len() > MAX_PROMPT_BYTES {
        return Err(AppError::InvalidPrompt(format!(
            "the document exceeds the {MAX_PROMPT_BYTES}-byte limit"
        )));
    }

    let (front_matter, content) = split_front_matter(document)?;
    if content.trim().is_empty() {
        return Err(AppError::InvalidPrompt(
            "the prompt content cannot be empty".into(),
        ));
    }
    let metadata = match front_matter {
        Some(yaml) => parse_metadata(yaml, fallback_name)?,
        None => PromptMetadata {
            name: Some(validate_string(fallback_name, "name", 120)?),
            ..PromptMetadata::default()
        },
    };
    let canonical_metadata = serde_json::to_vec(&metadata).map_err(|error| {
        AppError::InvalidPrompt(format!("metadata could not be canonicalized: {error}"))
    })?;
    let mut hasher = Sha256::new();
    hasher.update(canonical_metadata);
    hasher.update([0]);
    hasher.update(content.as_bytes());

    Ok(ParsedPrompt {
        metadata,
        content: content.to_string(),
        raw_document: document.to_string(),
        source_hash: finalize_sha256(hasher),
    })
}

fn finalize_sha256(hasher: Sha256) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = hasher.finalize();
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn split_front_matter(document: &str) -> AppResult<(Option<&str>, &str)> {
    let Some(first_line_end) = document.find('\n').map(|index| index + 1) else {
        return if document_line(document) == "---" {
            Err(AppError::InvalidPrompt(
                "the YAML front matter is missing its closing delimiter".into(),
            ))
        } else {
            Ok((None, document))
        };
    };
    if document_line(&document[..first_line_end]) != "---" {
        return Ok((None, document));
    }

    let mut offset = first_line_end;
    for line in document[first_line_end..].split_inclusive('\n') {
        if offset.saturating_sub(first_line_end) > MAX_FRONT_MATTER_BYTES {
            return Err(AppError::InvalidPrompt(format!(
                "YAML front matter exceeds the {MAX_FRONT_MATTER_BYTES}-byte limit"
            )));
        }
        if document_line(line) == "---" {
            let content_start = offset + line.len();
            return Ok((
                Some(&document[first_line_end..offset]),
                &document[content_start..],
            ));
        }
        offset += line.len();
    }
    if offset < document.len() && document_line(&document[offset..]) == "---" {
        return Ok((Some(&document[first_line_end..offset]), ""));
    }
    Err(AppError::InvalidPrompt(
        "the YAML front matter is missing its closing delimiter".into(),
    ))
}

fn document_line(line: &str) -> &str {
    line.strip_suffix('\n')
        .unwrap_or(line)
        .strip_suffix('\r')
        .unwrap_or_else(|| line.strip_suffix('\n').unwrap_or(line))
}

fn parse_metadata(yaml: &str, fallback_name: &str) -> AppResult<PromptMetadata> {
    validate_yaml_events(yaml)?;
    if yaml.trim().is_empty() {
        return Ok(PromptMetadata {
            name: Some(validate_string(fallback_name, "name", 120)?),
            ..PromptMetadata::default()
        });
    }
    let documents = Yaml::load_from_str(yaml).map_err(|error| {
        AppError::InvalidPrompt(format!("YAML front matter is invalid: {error}"))
    })?;
    if documents.len() != 1 {
        return Err(AppError::InvalidPrompt(
            "YAML front matter must contain exactly one document".into(),
        ));
    }
    let mapping = documents[0].as_mapping().ok_or_else(|| {
        AppError::InvalidPrompt("YAML front matter must be a key-value mapping".into())
    })?;
    if mapping.len() > MAX_METADATA_KEYS {
        return Err(AppError::InvalidPrompt(format!(
            "YAML front matter exceeds the {MAX_METADATA_KEYS}-key limit"
        )));
    }
    let mut values = BTreeMap::new();
    for (key, value) in mapping {
        let key = key
            .as_str()
            .ok_or_else(|| AppError::InvalidPrompt("YAML metadata keys must be strings".into()))?;
        if values
            .insert(key.to_string(), yaml_to_json(value, 0)?)
            .is_some()
        {
            return Err(AppError::InvalidPrompt(format!(
                "YAML metadata key {key} is duplicated"
            )));
        }
    }
    metadata_from_values(values, fallback_name)
}

fn validate_yaml_events(yaml: &str) -> AppResult<()> {
    let mut depth = 0_usize;
    let mut events = 0_usize;
    let mut documents = 0_usize;
    for event in Parser::new_from_str(yaml) {
        let (event, _) = event.map_err(|error| {
            AppError::InvalidPrompt(format!("YAML front matter is invalid: {error}"))
        })?;
        events += 1;
        if events > MAX_YAML_EVENTS {
            return Err(AppError::InvalidPrompt(format!(
                "YAML front matter exceeds the {MAX_YAML_EVENTS}-event limit"
            )));
        }
        match event {
            Event::DocumentStart(_) => {
                documents += 1;
                if documents > 1 {
                    return Err(AppError::InvalidPrompt(
                        "multiple YAML documents are not accepted".into(),
                    ));
                }
            }
            Event::Alias(_) => {
                return Err(AppError::InvalidPrompt(
                    "YAML aliases are not accepted".into(),
                ));
            }
            Event::Scalar(_, _, anchor, tag) => {
                reject_yaml_extensions(anchor, tag.is_some())?;
            }
            Event::SequenceStart(anchor, tag) | Event::MappingStart(anchor, tag) => {
                reject_yaml_extensions(anchor, tag.is_some())?;
                depth += 1;
                if depth > MAX_YAML_DEPTH {
                    return Err(AppError::InvalidPrompt(format!(
                        "YAML nesting exceeds the {MAX_YAML_DEPTH}-level limit"
                    )));
                }
            }
            Event::SequenceEnd | Event::MappingEnd => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

fn reject_yaml_extensions(anchor: usize, has_tag: bool) -> AppResult<()> {
    if anchor != 0 {
        return Err(AppError::InvalidPrompt(
            "YAML anchors are not accepted".into(),
        ));
    }
    if has_tag {
        return Err(AppError::InvalidPrompt(
            "custom YAML tags are not accepted".into(),
        ));
    }
    Ok(())
}

fn yaml_to_json(value: &Yaml<'_>, depth: usize) -> AppResult<Value> {
    if depth > MAX_YAML_DEPTH {
        return Err(AppError::InvalidPrompt(format!(
            "YAML nesting exceeds the {MAX_YAML_DEPTH}-level limit"
        )));
    }
    if value.is_null() {
        return Ok(Value::Null);
    }
    if let Some(value) = value.as_bool() {
        return Ok(Value::Bool(value));
    }
    if let Some(value) = value.as_integer() {
        return Ok(Value::Number(Number::from(value)));
    }
    if let Some(value) = value.as_floating_point() {
        return Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| AppError::InvalidPrompt("YAML numbers must be finite".into()));
    }
    if let Some(value) = value.as_str() {
        return Ok(Value::String(value.to_string()));
    }
    if let Some(sequence) = value.as_sequence() {
        return sequence
            .iter()
            .map(|item| yaml_to_json(item, depth + 1))
            .collect::<AppResult<Vec<_>>>()
            .map(Value::Array);
    }
    if let Some(mapping) = value.as_mapping() {
        let mut object = serde_json::Map::new();
        for (key, value) in mapping {
            let key = key.as_str().ok_or_else(|| {
                AppError::InvalidPrompt("nested YAML metadata keys must be strings".into())
            })?;
            object.insert(key.to_string(), yaml_to_json(value, depth + 1)?);
        }
        return Ok(Value::Object(object));
    }
    Err(AppError::InvalidPrompt(
        "YAML aliases, tags, and unresolved values are not accepted".into(),
    ))
}

fn metadata_from_values(
    mut values: BTreeMap<String, Value>,
    fallback_name: &str,
) -> AppResult<PromptMetadata> {
    let name = take_string(&mut values, "name", 120)?.unwrap_or(validate_string(
        fallback_name,
        "name",
        120,
    )?);
    let declared_version = take_string_or_number(&mut values, "version", 64)?;
    let description = take_string(&mut values, "description", 2_000)?;
    let tags = take_string_list(&mut values, "tags", 64)?;
    let recommended_models = take_string_list(&mut values, "recommended_models", 120)?;
    let temperature = take_number(&mut values, "temperature", 0.0, 2.0)?;
    let top_p = take_number(&mut values, "top_p", 0.0, 1.0)?;
    let top_k = take_u32(&mut values, "top_k", 1, 1_000)?;
    let context_reserve = take_u32(&mut values, "context_reserve", 0, 1_048_576)?;
    let collection = take_string(&mut values, "collection", 80)?;

    Ok(PromptMetadata {
        name: Some(name),
        declared_version,
        description,
        tags,
        recommended_models,
        temperature,
        top_p,
        top_k,
        context_reserve,
        collection,
        extra: values,
    })
}

fn take_string(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    maximum: usize,
) -> AppResult<Option<String>> {
    values
        .remove(key)
        .map(|value| match value {
            Value::String(value) => validate_string(&value, key, maximum),
            _ => Err(AppError::InvalidPrompt(format!(
                "metadata field {key} must be a string"
            ))),
        })
        .transpose()
}

fn take_string_or_number(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    maximum: usize,
) -> AppResult<Option<String>> {
    values
        .remove(key)
        .map(|value| match value {
            Value::String(value) => validate_string(&value, key, maximum),
            Value::Number(value) => validate_string(&value.to_string(), key, maximum),
            _ => Err(AppError::InvalidPrompt(format!(
                "metadata field {key} must be a string or number"
            ))),
        })
        .transpose()
}

fn take_string_list(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    maximum_item_length: usize,
) -> AppResult<Vec<String>> {
    let Some(value) = values.remove(key) else {
        return Ok(Vec::new());
    };
    let Value::Array(items) = value else {
        return Err(AppError::InvalidPrompt(format!(
            "metadata field {key} must be a list of strings"
        )));
    };
    if items.len() > MAX_LIST_ITEMS {
        return Err(AppError::InvalidPrompt(format!(
            "metadata field {key} exceeds the {MAX_LIST_ITEMS}-item limit"
        )));
    }
    let mut unique = BTreeSet::new();
    for item in items {
        let Value::String(item) = item else {
            return Err(AppError::InvalidPrompt(format!(
                "metadata field {key} must contain only strings"
            )));
        };
        unique.insert(validate_string(&item, key, maximum_item_length)?);
    }
    Ok(unique.into_iter().collect())
}

fn take_number(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    minimum: f64,
    maximum: f64,
) -> AppResult<Option<f64>> {
    values
        .remove(key)
        .map(|value| {
            let number = value.as_f64().ok_or_else(|| {
                AppError::InvalidPrompt(format!("metadata field {key} must be a number"))
            })?;
            if !number.is_finite() || !(minimum..=maximum).contains(&number) {
                return Err(AppError::InvalidPrompt(format!(
                    "metadata field {key} must be between {minimum} and {maximum}"
                )));
            }
            Ok(number)
        })
        .transpose()
}

fn take_u32(
    values: &mut BTreeMap<String, Value>,
    key: &str,
    minimum: u32,
    maximum: u32,
) -> AppResult<Option<u32>> {
    values
        .remove(key)
        .map(|value| {
            let number = value.as_u64().ok_or_else(|| {
                AppError::InvalidPrompt(format!("metadata field {key} must be a whole number"))
            })?;
            let number = u32::try_from(number).map_err(|_| {
                AppError::InvalidPrompt(format!("metadata field {key} is too large"))
            })?;
            if !(minimum..=maximum).contains(&number) {
                return Err(AppError::InvalidPrompt(format!(
                    "metadata field {key} must be between {minimum} and {maximum}"
                )));
            }
            Ok(number)
        })
        .transpose()
}

fn validate_string(value: &str, field: &str, maximum: usize) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > maximum || value.contains('\0') {
        return Err(AppError::InvalidPrompt(format!(
            "metadata field {field} must contain 1 to {maximum} characters"
        )));
    }
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_bom_free_crlf_content_and_unknown_metadata() {
        let document = "\u{feff}---\r\nname: Code Review\r\ntags: [rust, review]\r\ncustom: enabled\r\n---\r\nLine one\r\nLine two\r\n";
        let parsed = parse_prompt_document(document, "fallback").unwrap();
        assert_eq!(parsed.content, "Line one\r\nLine two\r\n");
        assert_eq!(parsed.metadata.name.as_deref(), Some("Code Review"));
        assert_eq!(parsed.metadata.tags, ["review", "rust"]);
        assert_eq!(parsed.metadata.extra["custom"], "enabled");
        assert!(!parsed.raw_document.starts_with('\u{feff}'));
    }

    #[test]
    fn uses_the_filename_for_plain_text_and_hashes_deterministically() {
        let first = parse_prompt_document("Keep this exact.\r\n", "Plain Prompt").unwrap();
        let second = parse_prompt_document("Keep this exact.\r\n", "Plain Prompt").unwrap();
        assert_eq!(first.metadata.name.as_deref(), Some("Plain Prompt"));
        assert_eq!(first.content, "Keep this exact.\r\n");
        assert_eq!(first.source_hash, second.source_hash);
    }

    #[test]
    fn rejects_malformed_alias_tag_and_deep_yaml() {
        assert!(parse_prompt_document("---\nname: broken\nBody", "fallback").is_err());
        assert!(parse_prompt_document(
            "---\ndefaults: &defaults [one]\ntags: *defaults\n---\nBody",
            "fallback"
        )
        .is_err());
        assert!(
            parse_prompt_document("---\nname: !custom profile\n---\nBody", "fallback").is_err()
        );
        let nested = format!(
            "---\ncustom: {}value{}\n---\nBody",
            "[".repeat(17),
            "]".repeat(17)
        );
        assert!(parse_prompt_document(&nested, "fallback").is_err());
    }

    #[test]
    fn validates_known_metadata_bounds() {
        assert!(parse_prompt_document("---\ntemperature: 4\n---\nBody", "fallback").is_err());
        assert!(parse_prompt_document("---\ntop_k: 4.5\n---\nBody", "fallback").is_err());
        assert!(parse_prompt_document("---\ntags: rust\n---\nBody", "fallback").is_err());
    }
}
