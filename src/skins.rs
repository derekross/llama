//! Wearing a skin: from the Museum (md5 or link) or from a local file.
//! The CLI, the bar widget and drag-and-drop all come through here; the
//! running window notices the state file change and re-dresses itself.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::museum;
use crate::skin::{self, Skin};
use crate::store::{self, State};

/// Wears a Museum skin (md5 or link) or a local .wsz/.zip/folder.
pub fn wear(target: &str) -> Result<State> {
    let mut st = State::load();
    let path = Path::new(target);
    if path.exists() {
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        // Keep a copy, so the skin survives the original being moved.
        let dest = keep(path, &name, &store::skins_dir())
            .with_context(|| format!("{target} is not a usable skin"))?;
        let name = std::fs::read_to_string(dest.with_extension("name"))
            .ok()
            .filter(|_| dest.is_file())
            .unwrap_or(name);
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

/// Checks a local skin and copies what Llama uses of it into the skins
/// folder `dir`, returning the copy. A `.wsz`/`.zip` is copied whole (it
/// is at most `skin::MAX_WSZ`); a folder keeps only the files Llama reads.
fn keep(path: &Path, name: &str, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let files = skin::read_skin_files(path)?;
    if !Skin::from_files(name.to_string(), files.clone()).has_main() {
        bail!("no readable main.bmp");
    }
    // A skin already in the skins folder is worn where it is.
    if let (Ok(p), Ok(d)) = (path.canonicalize(), dir.canonicalize())
        && p.starts_with(&d)
    {
        return Ok(p);
    }
    let stem = slot_stem(name);
    if path.is_dir() {
        let dest = free_slot(dir, &stem, "", |p| {
            p.is_dir() && skin::read_skin_files(p).is_ok_and(|f| f == files)
        })?;
        if !dest.exists() {
            write_folder(&dest, &files)?;
        }
        return Ok(dest);
    }
    let bytes = skin::read_capped(path, skin::MAX_WSZ)?;
    let dest = free_slot(dir, &stem, ".wsz", |p| {
        skin::read_capped(p, skin::MAX_WSZ).is_ok_and(|b| b == bytes)
    })?;
    if !dest.exists() {
        store::write_atomic(&dest, &bytes)?;
    }
    let name_path = dest.with_extension("name");
    if !name_path.exists() {
        store::write_atomic(&name_path, name.as_bytes())?;
    }
    Ok(dest)
}

/// The file stem to keep a local skin under. A bare md5 is how Museum
/// installs are named, so a local file with one gets a suffix instead of
/// standing in for (or being replaced by) the Museum's copy.
fn slot_stem(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        "skin".into()
    } else if museum::parse_md5(name).as_deref() == Some(name) {
        format!("{name}-local")
    } else {
        name.to_string()
    }
}

/// `dir/stem.ext`, or `stem-2.ext`, `stem-3.ext`… : the first path that is
/// free or already holds the same skin (`same`), so a different skin of
/// the same name never overwrites an installed one.
fn free_slot(dir: &Path, stem: &str, ext: &str, same: impl Fn(&Path) -> bool) -> Result<PathBuf> {
    for n in 1..1000 {
        let file = if n == 1 {
            format!("{stem}{ext}")
        } else {
            format!("{stem}-{n}{ext}")
        };
        let p = dir.join(file);
        // A .wsz slot is taken by a leftover .name file too.
        let free = !p.exists() && (ext.is_empty() || !p.with_extension("name").exists());
        if free || p.exists() && same(&p) {
            return Ok(p);
        }
    }
    bail!("too many skins named {stem}")
}

/// Writes a folder skin through a temp folder and a rename.
fn write_folder(dest: &Path, files: &HashMap<String, Vec<u8>>) -> Result<()> {
    let mut tmp = dest.as_os_str().to_owned();
    tmp.push(format!(".tmp{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let res = files
        .iter()
        .try_for_each(|(name, bytes)| std::fs::write(tmp.join(name), bytes))
        .and_then(|()| std::fs::rename(&tmp, dest));
    if let Err(e) = res {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Option<PathBuf> {
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/skins/{name}.wsz"));
        if !p.exists() {
            eprintln!(
                "skipping: {} is missing; run tests/fetch-skins.sh",
                p.display()
            );
            return None;
        }
        Some(p)
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("llama-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn same_name_gets_a_suffix_same_bytes_reuse() {
        let (Some(base), Some(green)) = (fixture("base-2.91"), fixture("Green-Dimension-V2"))
        else {
            return;
        };
        let t = scratch("wear-keep");
        let skins = t.join("skins");
        let (a, b) = (t.join("a"), t.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::copy(&base, a.join("foo.wsz")).unwrap();
        std::fs::copy(&green, b.join("foo.wsz")).unwrap();

        let first = keep(&a.join("foo.wsz"), "foo", &skins).unwrap();
        assert_eq!(first, skins.join("foo.wsz"));
        assert_eq!(keep(&a.join("foo.wsz"), "foo", &skins).unwrap(), first);
        let second = keep(&b.join("foo.wsz"), "foo", &skins).unwrap();
        assert_eq!(second, skins.join("foo-2.wsz"));
        assert_eq!(
            std::fs::read(&first).unwrap(),
            std::fs::read(&base).unwrap()
        );
        // Wearing an installed copy again uses it in place.
        assert_eq!(
            keep(&second, "foo-2", &skins).unwrap(),
            second.canonicalize().unwrap()
        );

        // A local file named like a Museum md5 never takes the Museum's slot.
        let md5 = museum::BASE_MD5;
        std::fs::copy(&green, a.join(format!("{md5}.wsz"))).unwrap();
        let p = keep(&a.join(format!("{md5}.wsz")), md5, &skins).unwrap();
        assert_eq!(p, skins.join(format!("{md5}-local.wsz")));
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn archive_without_main_is_refused() {
        let t = scratch("wear-nomain");
        let mut w = zip::ZipWriter::new(std::fs::File::create(t.join("x.wsz")).unwrap());
        w.start_file("pledit.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut w, b"[Text]\nNormal=#00FF00\n").unwrap();
        w.finish().unwrap();
        assert!(keep(&t.join("x.wsz"), "x", &t.join("skins")).is_err());
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn folder_skin_is_copied() {
        let Some(base) = fixture("base-2.91") else {
            return;
        };
        let t = scratch("wear-folder");
        let src = t.join("My Skin");
        std::fs::create_dir_all(&src).unwrap();
        for (name, bytes) in skin::read_skin_files(&base).unwrap() {
            std::fs::write(src.join(name.to_uppercase()), bytes).unwrap();
        }
        std::fs::write(src.join("readme.txt"), b"hi").unwrap();
        let skins = t.join("skins");
        let dest = keep(&src, "My Skin", &skins).unwrap();
        assert_eq!(dest, skins.join("My Skin"));
        assert!(!dest.join("readme.txt").exists());
        assert!(Skin::load(&dest).unwrap().has_main());
        assert_eq!(keep(&src, "My Skin", &skins).unwrap(), dest);
        std::fs::remove_dir_all(&t).unwrap();
    }
}
