//! Port of the reference crawler `pkg/output/custom_field.go` — custom field
//! configuration (`-flc`): YAML-defined regexes extracted from responses into
//! `Request.CustomFields`. When no config is given, the reference crawler's
//! default email field applies.

use serde::Deserialize;

/// A custom field extraction rule (reference crawler `output.CustomFieldConfig`).
#[derive(Debug, Clone, Deserialize)]
pub struct CustomFieldConfig {
    pub name: String,
    #[serde(default = "default_type")]
    #[allow(dead_code)]
    pub r#type: String,
    /// Which response part to scan: `body`, `header`, or `response` (both).
    #[serde(default)]
    pub part: String,
    /// Capturing group to use (default 0 = whole match).
    #[serde(default)]
    pub group: usize,
    #[serde(default)]
    pub regex: Vec<String>,
}

fn default_type() -> String {
    "regex".to_string()
}

/// The reference crawler's built-in default config (email extraction).
pub fn default_field_configs() -> Vec<CompiledFieldConfig> {
    vec![CompiledFieldConfig {
        name: "email".to_string(),
        part: "response".to_string(),
        group: 0,
        regexes: vec![regex::Regex::new(r"([a-zA-Z0-9._-]+@[a-zA-Z0-9._-]+\.[a-zA-Z0-9_-]+)")
            .expect("default email regex")],
    }]
}

/// A custom field config with compiled regexes.
#[derive(Debug, Clone)]
pub struct CompiledFieldConfig {
    pub name: String,
    pub part: String,
    pub group: usize,
    pub regexes: Vec<regex::Regex>,
}

/// Load a field-config YAML file (reference crawler `loadCustomFields`).
/// Invalid names, duplicate names, or names colliding with the 14 built-in
/// fields are rejected.
pub fn load_field_config(path: &str) -> Result<Vec<CompiledFieldConfig>, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("customfield: could not read field config: {e}"))?;
    let data: Vec<CustomFieldConfig> = serde_yaml::from_str(&content)
        .map_err(|e| format!("customfield: could not decode field config: {e}"))?;
    let mut seen = std::collections::HashSet::new();
    let mut compiled = Vec::new();
    for item in data {
        if !regex::Regex::new(r"^[A-Za-z0-9_-]+$")
            .unwrap()
            .is_match(&item.name)
        {
            return Err(format!("customfield: wrong custom field name {}", item.name));
        }
        if crate::output::fields::FIELD_NAMES.contains(&item.name.as_str()) {
            return Err(format!(
                "customfield: could not register custom field. \"{}\" already pre-defined field",
                item.name
            ));
        }
        if !seen.insert(item.name.clone()) {
            return Err(format!(
                "customfield: could not register custom field. \"{}\" custom field already exists",
                item.name
            ));
        }
        let mut regexes = Vec::new();
        for rg in &item.regex {
            regexes.push(
                regex::Regex::new(rg)
                    .map_err(|e| format!("customfield: could not parse regex in field config: {e}"))?,
            );
        }
        let part = if item.part.is_empty() { "response".to_string() } else { item.part };
        compiled.push(CompiledFieldConfig {
            name: item.name,
            part,
            group: item.group,
            regexes,
        });
    }
    Ok(compiled)
}

/// Extract custom fields from a response body and headers
/// (reference crawler `customFieldRegexParser`). Returns a map of
/// field name → matched values.
pub fn extract_custom_fields(
    configs: &[CompiledFieldConfig],
    body: &str,
    headers: &std::collections::HashMap<String, String>,
) -> std::collections::HashMap<String, Vec<String>> {
    let mut out: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for config in configs {
        let mut results: Vec<String> = Vec::new();
        for re in &config.regexes {
            let mut matches: Vec<Vec<String>> = Vec::new();
            if config.part == "body" || config.part == "response" {
                matches.extend(re.captures_iter(body).map(|c| {
                    c.iter()
                        .map(|m| m.map(|m| m.as_str().to_string()).unwrap_or_default())
                        .collect()
                }));
            }
            if config.part == "header" || config.part == "response" {
                for (key, value) in headers {
                    let header = format!("{key}: {value}");
                    matches.extend(re.captures_iter(&header).map(|c| {
                        c.iter()
                            .map(|m| m.map(|m| m.as_str().to_string()).unwrap_or_default())
                            .collect()
                    }));
                }
            }
            for m in matches {
                if m.len() < config.group + 1 {
                    continue;
                }
                let matched = m[config.group].clone();
                if !matched.is_empty() {
                    results.push(matched);
                }
            }
        }
        if !results.is_empty() {
            out.insert(config.name.clone(), results);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_email_extraction() {
        let configs = default_field_configs();
        let fields = extract_custom_fields(
            &configs,
            "contact us at hello@example.com or sales@test.io",
            &Default::default(),
        );
        assert_eq!(
            fields.get("email").unwrap(),
            &vec!["hello@example.com".to_string(), "sales@test.io".to_string()]
        );
    }

    #[test]
    fn test_header_part_extraction() {
        let configs = vec![CompiledFieldConfig {
            name: "server".to_string(),
            part: "header".to_string(),
            group: 1,
            regexes: vec![regex::Regex::new(r"Server: (\S+)").unwrap()],
        }];
        let mut headers = std::collections::HashMap::new();
        headers.insert("Server".to_string(), "nginx/1.2".to_string());
        let fields = extract_custom_fields(&configs, "", &headers);
        assert_eq!(fields.get("server").unwrap(), &vec!["nginx/1.2".to_string()]);
    }

    #[test]
    fn test_load_field_config_rejects_builtin_name() {
        let tmp = std::env::temp_dir().join("bc_field_cfg.yaml");
        std::fs::write(&tmp, "- name: url\n  part: body\n  regex:\n    - \"(x)\"\n").unwrap();
        assert!(load_field_config(tmp.to_str().unwrap()).is_err());
        let _ = std::fs::remove_file(&tmp);
    }
}
