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
const MAX_SKIN: u64 = crate::skin::MAX_WSZ;
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
    // No redirects: they could lead off the hosts checked above.
    let resp = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(30))
        .user_agent(UA)
        .redirects(0)
        .build()
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

/// Checks that `bytes` are the Museum skin `md5` (the Museum's md5 is the
/// file's) and that it is usable: an error page or a truncated download is
/// never kept.
fn verify(md5: &str, bytes: &[u8]) -> Result<()> {
    let got = md5_hex(bytes);
    if got != md5 {
        bail!("download of {md5} came back as {got}");
    }
    let skin = crate::skin::Skin::from_wsz(md5, bytes)?;
    if !skin.has_main() {
        bail!("{md5} has no readable main.bmp");
    }
    Ok(())
}

/// Whether the file at `path` is the Museum skin `md5`.
fn on_disk_ok(path: &std::path::Path, md5: &str) -> bool {
    crate::skin::read_capped(path, MAX_SKIN).is_ok_and(|b| verify(md5, &b).is_ok())
}

/// Downloads a Museum skin into the skins folder (once) and returns its path
/// and display name.
pub fn install(md5: &str) -> Result<(PathBuf, String)> {
    let dir = store::skins_dir();
    let path = dir.join(format!("{md5}.wsz"));
    let name_path = dir.join(format!("{md5}.name"));
    if on_disk_ok(&path, md5) {
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
    verify(md5, &bytes).context("the Museum's file is not a usable skin")?;
    store::write_atomic(&path, &bytes)?;
    let _ = store::write_atomic(&name_path, r.name().as_bytes());
    Ok((path, r.name()))
}

/// Makes sure the base skin is on disk, fetching it on first run, or again
/// if the copy there is not the base skin.
pub fn ensure_base() -> Result<PathBuf> {
    let path = store::base_skin_path();
    if !on_disk_ok(&path, BASE_MD5) {
        if path.exists() {
            log::warn!("{} is damaged, fetching it again", path.display());
            let _ = std::fs::remove_file(&path);
        }
        let bytes = download(&format!("https://r2.webampskins.org/skins/{BASE_MD5}.wsz"))?;
        verify(BASE_MD5, &bytes).context("the Museum's base skin is not usable")?;
        store::write_atomic(&path, &bytes)?;
    }
    Ok(path)
}

/// MD5 (RFC 1321) as lower-case hex. Only to match the Museum's file ids,
/// not for anything security-sensitive beyond "is this the file we asked for".
fn md5_hex(data: &[u8]) -> String {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64)
        .map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32)
        .collect();
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_le_bytes());
    let mut h: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    for chunk in msg.as_chunks::<64>().0 {
        let m: Vec<u32> = chunk
            .as_chunks::<4>()
            .0
            .iter()
            .map(|w| u32::from_le_bytes(*w))
            .collect();
        let [mut a, mut b, mut c, mut d] = h;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter()
        .flat_map(|x| x.to_le_bytes())
        .map(|b| format!("{b:02x}"))
        .collect()
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

    #[test]
    fn md5_matches_rfc_1321() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            md5_hex(
                b"12345678901234567890123456789012345678901234567890123456789012345678901234567890"
            ),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn verify_rejects_wrong_bytes() {
        assert!(verify(BASE_MD5, b"<html>not found</html>").is_err());
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/skins/base-2.91.wsz");
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!(
                "skipping: {} is missing; run tests/fetch-skins.sh",
                path.display()
            );
            return;
        };
        verify(BASE_MD5, &bytes).unwrap();
        assert!(verify("4308a2fc648033bf5fe7c4d56a5c8823", &bytes).is_err());
        assert!(on_disk_ok(&path, BASE_MD5));
    }
}
