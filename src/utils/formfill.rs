//! Port of the reference crawler `pkg/utils/formfill.go` — automatic form filling suggestions
//! based on input types, values, placeholders, and a configurable YAML dataset.

use serde::{Deserialize, Serialize};

/// Suggestions for form filling (reference crawler `FormFillData`, loadable via `-fc` YAML).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormFillData {
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_phone")]
    pub phone: String,
    #[serde(default = "default_placeholder")]
    pub placeholder: String,
}

fn default_phone() -> String {
    "2124567890".to_string()
}
fn default_placeholder() -> String {
    "celestia".to_string()
}

impl Default for FormFillData {
    fn default() -> Self {
        FormFillData {
            email: "celestia@example.org".to_string(),
            color: "#e66465".to_string(),
            password: "CelestiaP@assw0rd1".to_string(),
            phone: default_phone(),
            placeholder: default_placeholder(),
        }
    }
}

/// An input element of a form (reference crawler `FormInput`).
#[derive(Debug, Clone, Default)]
pub struct FormInput {
    pub input_type: String,
    pub name: String,
    pub value: String,
    pub placeholder: String,
    pub min: Option<i64>,
    pub max: Option<i64>,
    pub step: Option<i64>,
}

/// A select element of a form (reference crawler `FormSelect`).
#[derive(Debug, Clone, Default)]
pub struct FormSelect {
    pub name: String,
    /// `(value, selected)` pairs.
    pub options: Vec<(String, bool)>,
}

/// A textarea element of a form (reference crawler `FormTextArea`).
#[derive(Debug, Clone, Default)]
pub struct FormTextArea {
    pub name: String,
}

/// Any form field.
#[derive(Debug, Clone)]
pub enum FormField {
    Input(FormInput),
    Select(FormSelect),
    TextArea(FormTextArea),
}

/// Compute fill suggestions for form fields preserving insertion order
/// (reference crawler `FormFillSuggestions`).
pub fn form_fill_suggestions(fields: &[FormField], data: &FormFillData) -> Vec<(String, String)> {
    let mut merged: Vec<(String, String)> = Vec::new();

    for field in fields {
        match field {
            FormField::Input(input) => {
                match input.input_type.as_str() {
                    "radio" => {
                        if !merged.iter().any(|(k, _)| *k == input.name) {
                            set_value(&mut merged, &input.name, &input.value);
                        }
                        continue;
                    }
                    "checkbox" => {
                        set_value(&mut merged, &input.name, &input.value);
                        continue;
                    }
                    _ => {
                        if !input.value.is_empty() {
                            set_value(&mut merged, &input.name, &input.value);
                            continue;
                        }
                        // Placeholder text wins over the type-based default
                        // (reference crawler pass 1 sets Value from the
                        // placeholder and pass 2 skips non-empty values).
                        if !input.placeholder.is_empty() {
                            set_value(&mut merged, &input.name, &input.placeholder);
                            continue;
                        }
                    }
                }
                match input.input_type.as_str() {
                    "email" => set_value(&mut merged, &input.name, &data.email),
                    "color" => set_value(&mut merged, &input.name, &data.color),
                    "number" | "range" => {
                        let min = input.min.unwrap_or(1);
                        let max = input.max.unwrap_or(10);
                        let step = input.step.unwrap_or(1);
                        let mut val = min + step;
                        if val > max {
                            val = max - step;
                        }
                        set_value(&mut merged, &input.name, &val.to_string());
                    }
                    "password" | "tel" => set_value(&mut merged, &input.name, &data.password),
                    _ => set_value(&mut merged, &input.name, &data.placeholder),
                }
            }
            FormField::Select(select) => {
                let selected = select
                    .options
                    .iter()
                    .find(|(_, sel)| *sel)
                    .or_else(|| select.options.first());
                if let Some((value, _)) = selected {
                    set_value(&mut merged, &select.name, value);
                }
            }
            FormField::TextArea(ta) => {
                set_value(&mut merged, &ta.name, &data.placeholder);
            }
        }
    }
    merged
}

/// Upsert a key/value preserving insertion order; empty keys are skipped
/// (celestia OrderedMap.Set semantics).
fn set_value(merged: &mut Vec<(String, String)>, key: &str, value: &str) {
    if key.is_empty() {
        return;
    }
    if let Some(entry) = merged.iter_mut().find(|(k, _)| k == key) {
        entry.1 = value.to_string();
    } else {
        merged.push((key.to_string(), value.to_string()));
    }
}

/// Load form fill data from a YAML config file (reference crawler
/// `readCustomFormConfig` + `data.Resolve()`): values using faker function
/// syntax (`rand_email()`) are resolved to generated data.
#[cfg(not(target_arch = "wasm32"))]
pub fn load_form_config(path: &str) -> Result<FormFillData, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read form config: {e}"))?;
    let mut data: FormFillData =
        serde_yaml::from_str(&contents).map_err(|e| format!("could not decode form config: {e}"))?;
    data.email = resolve_faker(&data.email);
    data.color = resolve_faker(&data.color);
    data.password = resolve_faker(&data.password);
    data.phone = resolve_faker(&data.phone);
    data.placeholder = resolve_faker(&data.placeholder);
    Ok(data)
}

/// Resolve a faker DSL value (reference crawler `utils/formfill.go` Resolve /
/// `resolveField`): `rand_email()`-style function calls produce generated
/// values; anything else passes through unchanged.
pub fn resolve_faker(value: &str) -> String {
    let trimmed = value.trim();
    if !trimmed.ends_with("()") {
        return value.to_string();
    }
    let func = trimmed.trim_end_matches("()");
    match func {
        "rand_email" => format!("user{}@example.org", rand_number(1000, 9999)),
        "rand_name" | "rand_full_name" => format!("User{}", rand_number(100, 999)),
        "rand_first_name" => "John".to_string(),
        "rand_last_name" => "Doe".to_string(),
        "rand_user_name" => format!("user{}", rand_number(100, 999)),
        "rand_company" => "Acme Corp".to_string(),
        "rand_password" => format!("P@ss{}{}", rand_number(1000, 9999), "aB1!"),
        "rand_phone" => format!("+1555{:07}", rand_number(0, 9_999_999)),
        "rand_ip" | "rand_ipv4" => format!(
            "{}.{}.{}.{}",
            rand_number(1, 254),
            rand_number(0, 255),
            rand_number(0, 255),
            rand_number(1, 254)
        ),
        "rand_ipv6" => format!("2001:db8::{}", rand_number(1, 9999)),
        "rand_url" => format!("https://example.com/{}", rand_number(1000, 9999)),
        "rand_domain" => "example.org".to_string(),
        "rand_port" => rand_number(1024, 65535).to_string(),
        "rand_number" | "rand_int" => rand_number(1, 100).to_string(),
        "rand_string" => format!("str{}", rand_number(100000, 999999)),
        "rand_bool" => (rand_number(0, 1) == 1).to_string(),
        "rand_date" => format!("2024-{:02}-{:02}", rand_number(1, 12), rand_number(1, 28)),
        "rand_city" => "Springfield".to_string(),
        "rand_country" => "United States".to_string(),
        "rand_address" | "rand_street_address" => format!("{} Main St", rand_number(1, 999)),
        "rand_zip" => format!("{:05}", rand_number(0, 99999)),
        _ => value.to_string(),
    }
}

/// Small pseudo-random int in [min, max] (no external rng dep).
fn rand_number(min: i64, max: i64) -> i64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut s = STATE.load(Ordering::Relaxed);
    if s == 0 {
        s = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15)
            | 1;
    }
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    STATE.store(s, Ordering::Relaxed);
    min + (s % ((max - min + 1).max(1) as u64)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_suggestions_by_type() {
        let data = FormFillData::default();
        let fields = vec![
            FormField::Input(FormInput {
                input_type: "email".into(),
                name: "mail".into(),
                ..Default::default()
            }),
            FormField::Input(FormInput {
                input_type: "password".into(),
                name: "pass".into(),
                ..Default::default()
            }),
        ];
        let sugg = form_fill_suggestions(&fields, &data);
        assert!(sugg.contains(&("mail".into(), "celestia@example.org".into())));
        assert!(sugg.contains(&("pass".into(), "CelestiaP@assw0rd1".into())));
    }

    #[test]
    fn test_existing_value_wins() {
        let data = FormFillData::default();
        let fields = vec![FormField::Input(FormInput {
            input_type: "text".into(),
            name: "q".into(),
            value: "prefilled".into(),
            ..Default::default()
        })];
        let sugg = form_fill_suggestions(&fields, &data);
        assert_eq!(sugg, vec![("q".to_string(), "prefilled".to_string())]);
    }

    #[test]
    fn test_number_range() {
        let data = FormFillData::default();
        let fields = vec![FormField::Input(FormInput {
            input_type: "number".into(),
            name: "n".into(),
            min: Some(5),
            max: Some(20),
            step: Some(2),
            ..Default::default()
        })];
        let sugg = form_fill_suggestions(&fields, &data);
        assert!(sugg.contains(&("n".into(), "7".into())));
    }

    #[test]
    fn test_select_picks_selected_then_first() {
        let data = FormFillData::default();
        let fields = vec![FormField::Select(FormSelect {
            name: "s".into(),
            options: vec![("a".into(), false), ("b".into(), true)],
        })];
        let sugg = form_fill_suggestions(&fields, &data);
        assert!(sugg.contains(&("s".into(), "b".into())));
    }

    #[test]
    fn test_placeholder_wins_over_type_default() {
        let data = FormFillData::default();
        let fields = vec![FormField::Input(FormInput {
            input_type: "text".into(),
            name: "q".into(),
            placeholder: "Search…".into(),
            ..Default::default()
        })];
        let sugg = form_fill_suggestions(&fields, &data);
        // Reference crawler: the placeholder text is the fill value.
        assert_eq!(sugg, vec![("q".to_string(), "Search…".to_string())]);
    }

    #[test]
    fn test_resolve_faker() {
        assert_eq!(resolve_faker("static"), "static");
        let email = resolve_faker("rand_email()");
        assert!(email.contains("@example.org"), "{email}");
        assert!(resolve_faker("rand_ip()").contains('.'));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn test_load_form_config_yaml() {
        let yaml = "email: x@y.z\npassword: secret\n";
        let parsed: FormFillData = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(parsed.email, "x@y.z");
        assert_eq!(parsed.password, "secret");
    }
}
