use std::fs;
use std::path::{Path, PathBuf};

/// Directory names inside the Overwatch client that hold GPU shader data of its embedded browser.
const CLIENT_SHADER_DIRS: [&str; 6] = [
    "GrShaderCache",
    "ShaderCache",
    "GPUCache",
    "DawnWebGPUCache",
    "DawnGraphiteCache",
    "GraphiteDawnCache",
];

const CLIENT_SEARCH_DEPTH: usize = 6;

/// Vendor folder and cache sub-folders under `%LOCALAPPDATA%`.
const DRIVER_CACHES: [(&str, &[&str]); 3] = [
    ("NVIDIA", &["DXCache", "GLCache"]),
    ("AMD", &["DxCache", "DxcCache", "VkCache", "GLCache"]),
    ("Intel", &["ShaderCache"]),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Driver,
    System,
    Client,
}

pub struct Group {
    pub name: String,
    pub size: u64,
}

pub struct Target {
    pub kind: Kind,
    pub name: &'static str,
    pub tag: String,
    /// Folders whose contents are deleted. The folders themselves stay.
    pub dirs: Vec<PathBuf>,
    /// Folder that the open button shows.
    pub open: PathBuf,
    pub size: u64,
    /// Per-window folders of the client, largest first. Empty for other kinds.
    pub groups: Vec<Group>,
}

#[derive(Default)]
pub struct CleanResult {
    pub freed: u64,
    pub skipped: u32,
}

pub fn discover(local_app_data: &Path) -> Vec<Target> {
    let mut targets = Vec::new();
    targets.extend(driver_target(local_app_data));
    targets.extend(system_target(local_app_data));
    targets.extend(client_target(local_app_data));
    targets
}

fn driver_target(local: &Path) -> Option<Target> {
    let mut vendors = Vec::new();
    let mut dirs = Vec::new();
    let mut open = None;
    for (vendor, subs) in DRIVER_CACHES {
        let found: Vec<PathBuf> = subs
            .iter()
            .map(|s| local.join(vendor).join(s))
            .filter(|p| p.is_dir())
            .collect();
        if found.is_empty() {
            continue;
        }
        vendors.push(vendor);
        open.get_or_insert_with(|| local.join(vendor));
        dirs.extend(found);
    }
    let size = dirs.iter().map(|d| dir_size(d)).sum();
    Some(Target {
        kind: Kind::Driver,
        name: "显卡驱动缓存",
        tag: format!("{} · 所有游戏", vendors.join(" · ")),
        dirs,
        open: open?,
        size,
        groups: Vec::new(),
    })
}

fn system_target(local: &Path) -> Option<Target> {
    let dir = local.join("D3DSCache");
    if !dir.is_dir() {
        return None;
    }
    Some(Target {
        kind: Kind::System,
        name: "Windows 着色器缓存",
        tag: "所有游戏".to_string(),
        size: dir_size(&dir),
        open: dir.clone(),
        dirs: vec![dir],
        groups: Vec::new(),
    })
}

fn client_target(local: &Path) -> Option<Target> {
    let root = local.join("Blizzard Entertainment").join("Overwatch");
    if !root.is_dir() {
        return None;
    }
    let mut found = Vec::new();
    find_client_dirs(&root, 1, &mut found);

    let mut groups: Vec<Group> = Vec::new();
    for dir in &found {
        let name = group_name(&root, dir);
        let size = dir_size(dir);
        match groups.iter_mut().find(|g| g.name == name) {
            Some(g) => g.size += size,
            None => groups.push(Group { name, size }),
        }
    }
    groups.sort_by(|a, b| b.size.cmp(&a.size));
    Some(Target {
        kind: Kind::Client,
        name: "守望先锋客户端界面缓存",
        tag: "仅守望先锋".to_string(),
        size: groups.iter().map(|g| g.size).sum(),
        open: root,
        dirs: found,
        groups,
    })
}

fn find_client_dirs(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        if !kind.is_dir() {
            continue;
        }
        let name = entry.file_name();
        if CLIENT_SHADER_DIRS.iter().any(|n| name == *n) {
            out.push(entry.path());
        } else if depth < CLIENT_SEARCH_DEPTH {
            find_client_dirs(&entry.path(), depth + 1, out);
        }
    }
}

/// `<root>/<build id>/<window>/...` belongs to group `<window>`.
fn group_name(root: &Path, dir: &Path) -> String {
    let rel = dir.strip_prefix(root).unwrap_or(dir);
    let parts: Vec<_> = rel.components().collect();
    let index = if parts.len() >= 3 { 1 } else { 0 };
    parts[index].as_os_str().to_string_lossy().into_owned()
}

pub fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// Deletes everything inside each dir. Symbolic links are left alone.
pub fn clean(dirs: &[PathBuf]) -> CleanResult {
    let mut result = CleanResult::default();
    for dir in dirs {
        clean_contents(dir, &mut result);
    }
    result
}

fn clean_contents(dir: &Path, result: &mut CleanResult) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            clean_contents(&path, result);
            let _ = fs::remove_dir(&path);
        } else if kind.is_file() {
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            match fs::remove_file(&path) {
                Ok(()) => result.freed += len,
                Err(_) => result.skipped += 1,
            }
        }
    }
}

pub fn format_size(bytes: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    let mb = bytes as f64 / MB;
    if bytes == 0 {
        "0 MB".to_string()
    } else if mb < 1.0 {
        format!("{:.1} MB", mb.max(0.1))
    } else if mb < 1024.0 {
        format!("{} MB", mb.round() as u64)
    } else {
        format!("{:.1} GB", mb / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn sandbox(name: &str) -> PathBuf {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-tmp")
            .join(format!("{name}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: PathBuf, len: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0u8; len]).unwrap();
    }

    #[test]
    fn discovers_and_cleans_all_targets() {
        let local = sandbox("full");
        write(local.join("NVIDIA/DXCache/a.nvph"), 1000);
        write(local.join("NVIDIA/GLCache/b/c.bin"), 500);
        write(local.join("AMD/DxCache/d.bin"), 200);
        write(local.join("D3DSCache/h1/x.idx"), 300);
        let ow = local.join("Blizzard Entertainment/Overwatch/111");
        write(ow.join("Event A/GrShaderCache/data_1"), 4000);
        write(ow.join("Event A/Default/GPUCache/data_1"), 1000);
        write(ow.join("Event B/ShaderCache/data_1"), 100);
        write(ow.join("Event B/Default/Cache/Cache_Data/keep"), 9999);
        write(ow.join("Store/catalog"), 7777);

        let targets = discover(&local);
        assert_eq!(targets.len(), 3);

        let driver = &targets[0];
        assert_eq!(driver.kind, Kind::Driver);
        assert_eq!(driver.tag, "NVIDIA · AMD · 所有游戏");
        assert_eq!(driver.size, 1700);
        assert_eq!(driver.open, local.join("NVIDIA"));

        assert_eq!(targets[1].size, 300);

        let client = &targets[2];
        assert_eq!(client.size, 5100);
        assert_eq!(client.groups[0].name, "Event A");
        assert_eq!(client.groups[0].size, 5000);
        assert_eq!(client.groups[1].name, "Event B");

        let dirs: Vec<PathBuf> = targets.iter().flat_map(|t| t.dirs.clone()).collect();
        let result = clean(&dirs);
        assert_eq!(result.freed, 1700 + 300 + 5100);
        assert_eq!(result.skipped, 0);

        assert!(local.join("NVIDIA/DXCache").is_dir());
        assert_eq!(dir_size(&local.join("D3DSCache")), 0);
        assert!(ow.join("Event B/Default/Cache/Cache_Data/keep").is_file());
        assert!(ow.join("Store/catalog").is_file());
    }

    #[test]
    fn hides_missing_targets() {
        let local = sandbox("empty");
        assert!(discover(&local).is_empty());
    }

    #[test]
    fn formats_sizes() {
        assert_eq!(format_size(0), "0 MB");
        assert_eq!(format_size(200_000), "0.2 MB");
        assert_eq!(format_size(268_435_456), "256 MB");
        assert_eq!(format_size(2_684_354_560), "2.5 GB");
    }
}
