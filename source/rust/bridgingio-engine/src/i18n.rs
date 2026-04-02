use std::collections::{BTreeSet, HashMap};

pub const OPERATOR_LOCALE_EN_US: &str = "en-US";
pub const OPERATOR_LOCALE_ZH_CN: &str = "zh-CN";

const EN_US_CATALOG: &str = include_str!("../resources/i18n/en-US.toml");
const ZH_CN_CATALOG: &str = include_str!("../resources/i18n/zh-CN.toml");

#[derive(Clone, Debug)]
pub struct Catalog {
    locale: String,
    primary: HashMap<String, String>,
    fallback_en_us: HashMap<String, String>,
}

impl Catalog {
    pub fn load(locale: &str) -> Result<Self, String> {
        let normalized = normalize_operator_locale(locale).to_string();
        let fallback_en_us = parse_catalog(EN_US_CATALOG)?;
        let primary = if normalized == OPERATOR_LOCALE_ZH_CN {
            parse_catalog(ZH_CN_CATALOG)?
        } else {
            fallback_en_us.clone()
        };
        Ok(Self {
            locale: normalized,
            primary,
            fallback_en_us,
        })
    }

    pub fn locale(&self) -> &str {
        &self.locale
    }

    pub fn t(&self, key: &str) -> String {
        self.primary
            .get(key)
            .or_else(|| self.fallback_en_us.get(key))
            .cloned()
            .unwrap_or_else(|| key.to_string())
    }

    pub fn tf(&self, key: &str, vars: &[(&str, &str)]) -> String {
        let mut output = self.t(key);
        for (name, value) in vars {
            let token = format!("{{{name}}}");
            output = output.replace(&token, value);
        }
        output
    }

    pub fn available_keys(&self) -> BTreeSet<String> {
        self.primary.keys().cloned().collect()
    }
}

pub fn normalize_operator_locale(input: &str) -> &'static str {
    let trimmed = input.trim();
    if trimmed.eq_ignore_ascii_case("zh-cn") {
        OPERATOR_LOCALE_ZH_CN
    } else {
        OPERATOR_LOCALE_EN_US
    }
}

pub fn is_supported_operator_locale(input: &str) -> bool {
    input.trim().eq_ignore_ascii_case("en-us") || input.trim().eq_ignore_ascii_case("zh-cn")
}

pub fn supported_operator_locales() -> &'static [&'static str] {
    &[OPERATOR_LOCALE_EN_US, OPERATOR_LOCALE_ZH_CN]
}

pub fn validate_catalog_key_parity() -> Result<(), String> {
    let en = parse_catalog(EN_US_CATALOG)?;
    let zh = parse_catalog(ZH_CN_CATALOG)?;

    let en_keys = en.keys().cloned().collect::<BTreeSet<_>>();
    let zh_keys = zh.keys().cloned().collect::<BTreeSet<_>>();

    let missing_in_zh = en_keys
        .difference(&zh_keys)
        .cloned()
        .collect::<Vec<_>>();
    let missing_in_en = zh_keys
        .difference(&en_keys)
        .cloned()
        .collect::<Vec<_>>();

    if missing_in_zh.is_empty() && missing_in_en.is_empty() {
        return Ok(());
    }

    let mut message = String::from("catalog key mismatch");
    if !missing_in_zh.is_empty() {
        message.push_str(&format!("; missing in zh-CN: {}", missing_in_zh.join(", ")));
    }
    if !missing_in_en.is_empty() {
        message.push_str(&format!("; missing in en-US: {}", missing_in_en.join(", ")));
    }
    Err(message)
}

fn parse_catalog(raw: &str) -> Result<HashMap<String, String>, String> {
    let mut map = HashMap::new();
    for raw_line in raw.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() || (line.starts_with('[') && line.ends_with(']')) {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!("invalid catalog line: {line}"));
        };
        let key = key.trim();
        let value = parse_quoted(value.trim())?;
        map.insert(key.to_string(), value);
    }
    Ok(map)
}

fn parse_quoted(raw: &str) -> Result<String, String> {
    if raw.len() < 2 || !raw.starts_with('"') || !raw.ends_with('"') {
        return Err(format!("catalog value must be quoted: {raw}"));
    }
    let mut output = String::new();
    let mut chars = raw[1..raw.len() - 1].chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }
        let Some(escaped) = chars.next() else {
            return Err("invalid escape at end of catalog value".to_string());
        };
        match escaped {
            'n' => output.push('\n'),
            't' => output.push('\t'),
            'r' => output.push('\r'),
            '"' => output.push('"'),
            '\\' => output.push('\\'),
            other => {
                return Err(format!(
                    "unsupported escape sequence in catalog value: \\{other}"
                ))
            }
        }
    }
    Ok(output)
}

fn strip_comment(raw_line: &str) -> &str {
    let mut in_quotes = false;
    for (index, ch) in raw_line.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '#' if !in_quotes => return &raw_line[..index],
            _ => {}
        }
    }
    raw_line
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_operator_locale, validate_catalog_key_parity, Catalog, OPERATOR_LOCALE_EN_US,
        OPERATOR_LOCALE_ZH_CN,
    };

    #[test]
    fn locale_normalization_accepts_zh_cn() {
        assert_eq!(normalize_operator_locale("zh-CN"), OPERATOR_LOCALE_ZH_CN);
        assert_eq!(normalize_operator_locale("ZH-cn"), OPERATOR_LOCALE_ZH_CN);
        assert_eq!(normalize_operator_locale("unknown"), OPERATOR_LOCALE_EN_US);
    }

    #[test]
    fn catalog_falls_back_to_en_us() {
        let catalog = Catalog::load(OPERATOR_LOCALE_ZH_CN).expect("load zh catalog");
        assert!(!catalog.t("menu.root.title").is_empty());
        assert_eq!(catalog.t("missing.key.for.tests"), "missing.key.for.tests");
    }

    #[test]
    fn catalog_keys_match_between_en_and_zh() {
        validate_catalog_key_parity().expect("catalog keys should match");
    }
}
