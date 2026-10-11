//! The Winamp Skin Museum (skins.webamp.org): ~100k classic skins behind an
//! unauthenticated GraphQL API.

use std::io::Read;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::store;

const API: &str = "https://api.webamp.org/graphql";
const UA: &str = concat!(
    "llama/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/derekross/llama)"
);
const MAX_SKIN: u64 = 16 << 20;
pub const BASE_MD5: &str = "2f245e741b6bd4e1a122a28489f0c7b7";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkinRef {
    pub md5: String,
    pub filename: String,
    #[serde(default)]
    pub nsfw: bool,
    #[serde(default)]
    pub screenshot_url: String,
    #[serde(default)]
    pub download_url: String,
}

impl SkinRef {
    pub fn name(&self) -> String {
        let f = self
            .filename
            .trim_end_matches(".wsz")
            .trim_end_matches(".zip")
            .trim_end_matches(".WSZ");
        f.replace('_', " ")
    }
}

const FIELDS: &str = "md5 filename nsfw screenshot_url download_url";

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent(UA)
        .build()
}

fn query(gql: &str, vars: Value) -> Result<Value> {
    let resp: Value = agent()
        .post(API)
        .send_json(json!({"query": gql, "variables": vars}))
        .context("Skin Museum unreachable")?
        .into_json()?;
    if let Some(e) = resp.get("errors") {
        bail!("Skin Museum: {e}");
    }
    Ok(resp["data"].clone())
}

fn refs(v: &Value) -> Vec<SkinRef> {
    let list: Vec<SkinRef> = serde_json::from_value(v.clone()).unwrap_or_default();
    list.into_iter().filter(|s| !s.nsfw).collect()
}

/// The Museum's own ranking, best first.
pub fn featured(limit: usize, offset: usize) -> Result<Vec<SkinRef>> {
    let q = format!(
        "query($n:Int!,$o:Int!){{skins(sort:MUSEUM,first:$n,offset:$o){{nodes{{{FIELDS}}}}}}}"
    );
    let d = query(&q, json!({"n": limit, "o": offset}))?;
    Ok(refs(&d["skins"]["nodes"]))
}

pub fn search(text: &str, limit: usize, offset: usize) -> Result<Vec<SkinRef>> {
    let q = format!(
        "query($q:String!,$n:Int!,$o:Int!){{search_skins(query:$q,first:$n,offset:$o){{{FIELDS}}}}}"
    );
    let d = query(&q, json!({"q": text, "n": limit, "o": offset}))?;
    Ok(refs(&d["search_skins"]))
}

pub fn by_md5(md5: &str) -> Result<SkinRef> {
    let q = format!("query($m:String!){{fetch_skin_by_md5(md5:$m){{{FIELDS}}}}}");
    let d = query(&q, json!({"m": md5}))?;
    serde_json::from_value(d["fetch_skin_by_md5"].clone())
        .map_err(|_| anyhow!("no skin {md5} in the Museum"))
}

/// Accepts a bare md5 or a skins.webamp.org link.
pub fn parse_md5(s: &str) -> Option<String> {
    let s = s.trim().trim_end_matches('/');
    let tail = s.rsplit('/').next().unwrap_or(s);
    let tail = tail
        .split(['?', '#'])
        .next()
        .unwrap_or(tail)
        .to_ascii_lowercase();
    (tail.len() == 32 && tail.chars().all(|c| c.is_ascii_hexdigit())).then_some(tail)
}

fn download(url: &str) -> Result<Vec<u8>> {
    if !url.starts_with("https://r2.webampskins.org/")
        && !url.starts_with("https://skins.webamp.org/")
    {
        bail!("refusing to download from {url}");
    }
    let resp = agent()
        .get(url)
        .call()
        .with_context(|| format!("downloading {url}"))?;
    let mut buf = Vec::new();
    resp.into_reader()
        .take(MAX_SKIN + 1)
        .read_to_end(&mut buf)?;
    if buf.len() as u64 > MAX_SKIN {
        bail!("{url} is too large to be a skin");
    }
    Ok(buf)
}

/// Downloads a Museum skin into the skins folder (once) and returns its path
/// and display name.
pub fn install(md5: &str) -> Result<(PathBuf, String)> {
    let dir = store::skins_dir();
    let path = dir.join(format!("{md5}.wsz"));
    let name_path = dir.join(format!("{md5}.name"));
    if path.exists() {
        let name = std::fs::read_to_string(&name_path).unwrap_or_else(|_| md5.to_string());
        return Ok((path, name));
    }
    let r = by_md5(md5)?;
    let url = if r.download_url.is_empty() {
        format!("https://r2.webampskins.org/skins/{md5}.wsz")
    } else {
        r.download_url.clone()
    };
    let bytes = download(&url)?;
    crate::skin::Skin::from_wsz(md5, &bytes).context("the Museum's file is not a usable skin")?;
    store::write_atomic(&path, &bytes)?;
    let _ = std::fs::write(&name_path, r.name());
    Ok((path, r.name()))
}

/// Makes sure the base skin is on disk, fetching it on first run.
pub fn ensure_base() -> Result<PathBuf> {
    let path = store::base_skin_path();
    if !path.exists() {
        let bytes = download(&format!("https://r2.webampskins.org/skins/{BASE_MD5}.wsz"))?;
        store::write_atomic(&path, &bytes)?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_from_links() {
        let m = "2f245e741b6bd4e1a122a28489f0c7b7";
        assert_eq!(parse_md5(m).as_deref(), Some(m));
        assert_eq!(
            parse_md5(&format!("https://skins.webamp.org/skin/{m}/")).as_deref(),
            Some(m)
        );
        assert_eq!(
            parse_md5(&format!(
                "https://skins.webamp.org/skin/{}?x=1",
                m.to_uppercase()
            ))
            .as_deref(),
            Some(m)
        );
        assert_eq!(parse_md5("not-a-skin"), None);
    }
}
