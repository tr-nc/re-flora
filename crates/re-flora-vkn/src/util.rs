use anyhow::Result;
use std::collections::HashMap;
use std::env;
use std::hash::Hash;
use std::path::{Path, PathBuf};

pub trait MergeWithEq<K, V>
where
    K: Eq + Hash + Clone,
    V: Eq + Clone,
{
    fn merge_with_eq(&self, other: &HashMap<K, V>) -> Result<HashMap<K, V>>;
}

impl<K, V> MergeWithEq<K, V> for HashMap<K, V>
where
    K: Eq + Hash + Clone + std::fmt::Debug,
    V: Eq + Clone + std::fmt::Debug,
{
    fn merge_with_eq(&self, other: &HashMap<K, V>) -> Result<HashMap<K, V>> {
        let mut merged = HashMap::with_capacity(self.len() + other.len());

        for (k, v_self) in self {
            if let Some(v_other) = other.get(k) {
                if v_self != v_other {
                    return Err(anyhow::anyhow!(
                        "value mismatch for key {:?}: left={:?}, right={:?}",
                        k,
                        v_self,
                        v_other
                    ));
                }
            }
            merged.insert(k.clone(), v_self.clone());
        }

        for (k, v_other) in other {
            if !merged.contains_key(k) {
                merged.insert(k.clone(), v_other.clone());
            }
        }

        Ok(merged)
    }
}

pub fn project_root() -> PathBuf {
    if let Ok(root) = env::var("RE_FLORA_ROOT") {
        let root = PathBuf::from(root);
        if is_project_root(&root) {
            return root;
        }
    }

    if let Ok(current_dir) = env::current_dir() {
        if let Some(root) = find_project_root(current_dir) {
            return root;
        }
    }

    if let Ok(current_exe) = env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            if let Some(root) = find_project_root(exe_dir.to_path_buf()) {
                return root;
            }
        }
    }

    compile_time_project_root()
}

fn compile_time_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("re-flora-vkn must live under <project>/crates/re-flora-vkn")
        .to_path_buf()
}

fn find_project_root(start: PathBuf) -> Option<PathBuf> {
    for candidate in start.ancestors() {
        if is_project_root(candidate) {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

fn is_project_root(path: &Path) -> bool {
    path.join("assets").is_dir() && path.join("config").join("gui.toml").is_file()
}
