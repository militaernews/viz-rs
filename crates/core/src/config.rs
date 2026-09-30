use anyhow::{Context, Result};
use std::str::FromStr;

pub fn required(name: &str) -> Result<String> {
    std::env::var(name)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .with_context(|| format!("environment variable {name} is required"))
}

pub fn optional(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

pub fn parsed<T>(name: &str) -> Result<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    let raw = required(name)?;
    raw.trim().parse().map_err(|e| anyhow::anyhow!("environment variable {name}={raw:?} is invalid: {e}"))
}

pub fn parsed_or<T>(name: &str, default: T) -> Result<T>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    match optional(name) {
        Some(_) => parsed(name),
        None => Ok(default),
    }
}
