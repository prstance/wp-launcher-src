//! Before a launch: the packages' files that are deltas against the player's own
//! disc (delta.rs, the "delta" patches), made once into the package's folder,
//! where its Riivolution XML expects them. A package update replaces the folder,
//! and so makes them again.

use crate::delta::{self, Error};
use crate::game;
use crate::i18n::tr;
use crate::mods::{DeltaFile, Installed};
use crate::settings::Disc;
use crate::tools;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// A folder of the disc with at least this many files to take is extracted whole.
const WHOLE_FOLDER: usize = 8;

struct Job {
    package: String,
    dir: PathBuf,
    file: DeltaFile,
}

/// dolphin-tool's name of a disc file, in the DATA partition.
fn single(disc: &str) -> String {
    if disc.eq_ignore_ascii_case("main.dol") {
        "sys/main.dol".to_string()
    } else {
        disc.trim_start_matches('/').to_string()
    }
}

/// Where dolphin-tool puts it, under its output folder.
fn extracted(out: &Path, single: &str) -> PathBuf {
    match single {
        "sys/main.dol" => out.join("DATA/sys/main.dol"),
        path => out.join("DATA/files").join(path),
    }
}

/// The files still to make, of the packages the game starts with.
fn missing(packages: &[&Installed]) -> Vec<Job> {
    packages
        .iter()
        .flat_map(|p| {
            p.manifest.deltas().filter(|d| !p.dir.join(&d.file).is_file()).map(|d| Job { package: p.manifest.name.to_string(), dir: p.dir.clone(), file: d.clone() })
        })
        .collect()
}

/// Takes the disc files `jobs` need into `out`: a folder in one go when many come from it.
async fn extract(app: &AppHandle, disc: &Disc, jobs: &[Job], out: &Path) -> Result<(), String> {
    let tool = tools::dolphin_tool(app).ok_or_else(|| tr!("Dolphin est absent de ce build du launcher."))?;
    let mut folders: BTreeMap<String, usize> = BTreeMap::new();
    for job in jobs {
        let path = single(&job.file.disc);
        if let Some((folder, _)) = path.rsplit_once('/') {
            *folders.entry(folder.to_string()).or_default() += 1;
        }
    }
    let whole: Vec<&String> = folders.iter().filter(|(f, n)| **n >= WHOLE_FOLDER && f.as_str() != "sys").map(|(f, _)| f).collect();
    let mut wanted: Vec<String> = whole.iter().map(|f| f.to_string()).collect();
    for job in jobs {
        let path = single(&job.file.disc);
        if !whole.iter().any(|f| path.rsplit_once('/').is_some_and(|(parent, _)| parent == f.as_str())) && !wanted.contains(&path) {
            wanted.push(path);
        }
    }
    for path in wanted {
        let output = game::command(&tool)
            .args(["extract", "-q", "-i", &disc.path, "-p", "DATA", "-s", &path, "-o"])
            .arg(out)
            .output()
            .await
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err(tr!("Impossible de lire {file} sur ton ISO.", file = path));
        }
    }
    Ok(())
}

/// Makes the files of `jobs` from the extracted ones, a few at a time.
fn make(jobs: &[Job], out: &Path) -> Result<(), String> {
    let threads = std::thread::available_parallelism().map_or(2, |n| n.get()).min(8);
    let chunk = jobs.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let workers: Vec<_> = jobs.chunks(chunk).map(|part| scope.spawn(move || part.iter().try_for_each(|job| make_one(job, out)))).collect();
        workers.into_iter().try_for_each(|w| w.join().unwrap_or_else(|_| Err("delta".to_string())))
    })
}

fn make_one(job: &Job, out: &Path) -> Result<(), String> {
    let disc_file = single(&job.file.disc);
    let source = std::fs::read(extracted(out, &disc_file)).map_err(|_| tr!("Impossible de lire {file} sur ton ISO.", file = disc_file))?;
    let patch = std::fs::read(job.dir.join(&job.file.delta)).map_err(|e| e.to_string())?;
    let made = delta::apply(&patch, &source).map_err(|e| match e {
        Error::OtherSource => tr!(
            "« {name} » a besoin du fichier d’origine {file}, que ton ISO n’a pas : il lui faut le disque d’origine, sans mod.",
            name = job.package,
            file = disc_file
        ),
        Error::Broken => tr!("« {name} » est abîmé ({file}) : réinstalle-le.", name = job.package, file = job.file.delta),
    })?;
    let to = job.dir.join(&job.file.file);
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // (written aside first: a file that is there is a finished one)
    let part = to.with_extension("wpd-part");
    std::fs::write(&part, made).map_err(|e| e.to_string())?;
    std::fs::rename(&part, &to).map_err(|e| e.to_string())
}

/// Whether some files are still to make.
pub fn needed(packages: &[&Installed]) -> bool {
    !missing(packages).is_empty()
}

/// Makes the files still missing; quick when there are none.
pub async fn prepare(app: &AppHandle, disc: &Disc, packages: &[&Installed]) -> Result<(), String> {
    let jobs = missing(packages);
    if jobs.is_empty() {
        return Ok(());
    }
    let out = game::data_dir(app, "mods")?.join(".extract");
    let _ = std::fs::remove_dir_all(&out);
    let result = async {
        extract(app, disc, &jobs, &out).await?;
        let at = out.clone();
        tokio::task::spawn_blocking(move || make(&jobs, &at)).await.map_err(|e| e.to_string())?
    }
    .await;
    let _ = std::fs::remove_dir_all(&out);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_disc_files_as_dolphin_tool() {
        assert_eq!(single("main.dol"), "sys/main.dol");
        assert_eq!(single("/rel/mr001.rel.lz"), "rel/mr001.rel.lz");
        assert_eq!(extracted(Path::new("/x"), "sys/main.dol"), Path::new("/x/DATA/sys/main.dol"));
        assert_eq!(extracted(Path::new("/x"), "rel/a.rel.lz"), Path::new("/x/DATA/files/rel/a.rel.lz"));
    }
}

#[cfg(test)]
mod real {
    use super::*;
    use crate::mods::installed;

    /// WP_TEST_PACKAGE (a .wpver or .wpmod with deltas), WP_TEST_ISO, WP_TEST_FILES (the
    /// package's files as they were): every file made again from the disc.
    #[test]
    #[ignore]
    fn makes_a_package_again_from_the_disc() {
        let (package, iso, files) = (std::env::var("WP_TEST_PACKAGE").unwrap(), std::env::var("WP_TEST_ISO").unwrap(), std::env::var("WP_TEST_FILES").unwrap());
        let dir = std::env::temp_dir().join("wp-rebuild-real");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let kind = if package.ends_with(".wpmod") { &crate::mods::WPMOD } else { &crate::versions::WPVER };
        crate::mods::install(kind, Path::new(&package), &dir).unwrap();
        let all = installed(kind, &dir, &Default::default());
        let packages: Vec<&Installed> = all.iter().collect();
        let jobs = missing(&packages);
        let out = dir.join(".extract");
        let started = std::time::Instant::now();
        let tool = Path::new("tools/dolphin/usr/bin/dolphin-tool");
        for job in &jobs {
            let path = single(&job.file.disc);
            let ok = std::process::Command::new(tool).args(["extract", "-q", "-i", &iso, "-p", "DATA", "-s", &path, "-o"]).arg(&out).status().unwrap();
            assert!(ok.success(), "{path}");
        }
        println!("{} fichiers extraits en {:?}", jobs.len(), started.elapsed());
        make(&jobs, &out).unwrap();
        println!("refaits en {:?}", started.elapsed());
        assert!(!needed(&packages));
        for job in &jobs {
            let (made, was) = (std::fs::read(job.dir.join(&job.file.file)).unwrap(), std::fs::read(Path::new(&files).join(&job.file.file)).unwrap());
            if job.file.file.ends_with(".lz") {
                assert_eq!(crate::lz::decompress(&made).unwrap(), crate::lz::decompress(&was).unwrap(), "{}", job.file.file);
                assert!(made.len() <= was.len() + was.len() / 10, "{} : {} au lieu de {}", job.file.file, made.len(), was.len());
            } else {
                assert!(made == was, "{}", job.file.file);
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
