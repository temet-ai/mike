pub fn deserialize_permissive_bool<'de, D>(de: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let raw = serde::Deserialize::deserialize(de)?;
    match raw {
        toml::Value::Boolean(b) => Ok(b),
        toml::Value::String(s) => parse_permissive_bool_str(&s).map_err(D::Error::custom),
        toml::Value::Integer(i) => Ok(i != 0),
        other => Err(D::Error::custom(format!(
            "expected bool or string for permissive bool, got {other}"
        ))),
    }
}

pub fn deserialize_permissive_optional_bool<'de, D>(de: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let opt: Option<toml::Value> = serde::Deserialize::deserialize(de)?;
    match opt {
        None => Ok(None),
        Some(toml::Value::Boolean(b)) => Ok(Some(b)),
        Some(toml::Value::String(s)) => parse_permissive_bool_str(&s)
            .map(Some)
            .map_err(D::Error::custom),
        Some(toml::Value::Integer(i)) => Ok(Some(i != 0)),
        Some(other) => Err(D::Error::custom(format!(
            "expected bool or string for permissive bool, got {other}"
        ))),
    }
}

pub fn parse_permissive_bool_str(raw: &str) -> Result<bool, String> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "" | "0" | "false" | "no" | "off" => Ok(false),
        other => Err(format!(
            "expected bool flag value (1/true/yes/on or 0/false/no/off/empty), got `{other}`"
        )),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_permissive_bool_str_accepts_numeric_spelling() -> anyhow::Result<()> {
        assert!(super::parse_permissive_bool_str("1").map_err(|e| anyhow::anyhow!(e))?);
        assert!(!super::parse_permissive_bool_str("0").map_err(|e| anyhow::anyhow!(e))?);
        Ok(())
    }

    #[test]
    fn parse_permissive_bool_str_accepts_text_aliases_case_insensitive() -> anyhow::Result<()> {
        for yes in ["true", "TRUE", "True", "yes", "YES", "on", "ON"] {
            assert!(
                super::parse_permissive_bool_str(yes).map_err(|e| anyhow::anyhow!(e))?,
                "expected `{yes}` to parse as true"
            );
        }
        for no in ["false", "FALSE", "no", "NO", "off", "OFF"] {
            assert!(
                !super::parse_permissive_bool_str(no).map_err(|e| anyhow::anyhow!(e))?,
                "expected `{no}` to parse as false"
            );
        }
        Ok(())
    }

    #[test]
    fn parse_permissive_bool_str_rejects_unknown_strings() -> anyhow::Result<()> {
        assert!(super::parse_permissive_bool_str("maybe").is_err());
        assert!(!super::parse_permissive_bool_str("").map_err(|e| anyhow::anyhow!(e))?);
        Ok(())
    }
}
