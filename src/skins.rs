//! Wearing a skin: from the Museum (md5 or link) or from a local file.
//! The CLI, the bar widget and drag-and-drop all come through here; the
//! running window notices the state file change and re-dresses itself.

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::museum;
use crate::skin::Skin;
use crate::store::{self, State};

/// Wears a Museum skin (md5 or link) or a local .wsz/.zip/folder.
pub fn wear(target: &str) -> Result<State> {
    let mut st = State::load();
    let path = Path::new(target);
    if path.exists() {
        Skin::load(path).with_context(|| format!("{target} is not a usable skin"))?;
        let dir = store::skins_dir();
        std::fs::create_dir_all(&dir)?;
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        // Keep a copy, so the skin survives the original being moved.
        let dest = if path.is_dir() {
            path.canonicalize()?
        } else {
            dir.join(format!("{name}.wsz"))
        };
        if !path.is_dir() && path.canonicalize()? != dest.canonicalize().unwrap_or_default() {
            std::fs::copy(path, &dest)?;
            let _ = std::fs::write(dest.with_extension("name"), &name);
        }
        st.skin = Some(dest);
        st.skin_name = Some(name.replace('_', " "));
        st.skin_md5 = None;
    } else if let Some(md5) = museum::parse_md5(target) {
        let (p, name) = museum::install(&md5)?;
        st.skin = Some(p);
        st.skin_name = Some(name);
        st.skin_md5 = Some(md5);
    } else {
        bail!("{target}: not a file, a Museum md5, or a skins.webamp.org link");
    }
    st.save()?;
    Ok(st)
}
