//! Device-owned, optional acceleration only. Every pipeline still supplies its current
//! shader, layout, specialization and fixed-function state to the Vulkan driver.
use ash::vk;
use std::{
    fs::File,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const MAGIC: &[u8; 8] = b"RFLAPC01";
const HEADER_SIZE: usize = 8 + 32 + 32;
const MAX_BYTES: u64 = 256 * 1024 * 1024;
const LOG: &str = "re_flora::run_summary";

struct Storage {
    path: PathBuf,
    key: [u8; 32],
    vendor: u32,
    device: u32,
    uuid: [u8; vk::UUID_SIZE],
}

pub(super) struct PipelineCache {
    handle: vk::PipelineCache,
    storage: Option<Storage>,
    dirty: bool,
}

impl PipelineCache {
    pub(super) fn new(device: &ash::Device, properties: &vk::PhysicalDeviceProperties) -> Self {
        let storage = if std::env::var("RE_FLORA_PIPELINE_CACHE").as_deref() == Ok("off") {
            log::info!(target: LOG, "[PIPELINE_CACHE] status=disabled");
            None
        } else {
            match Storage::for_process(properties) {
                Ok(storage) => Some(storage),
                Err(err) => {
                    log::warn!("[PIPELINE_CACHE] persistence unavailable: {err}");
                    None
                }
            }
        };
        let bytes = storage.as_ref().and_then(|storage| match storage.load() {
            Ok(bytes) => {
                log::info!(target: LOG, "[PIPELINE_CACHE] status=hit bytes={} path={}", bytes.len(), storage.path.display());
                Some(bytes)
            }
            Err(err) => {
                log::info!(target: LOG, "[PIPELINE_CACHE] status=miss reason={err} path={}", storage.path.display());
                None
            }
        });
        let create = |bytes: &[u8]| unsafe {
            device.create_pipeline_cache(&vk::PipelineCacheCreateInfo::default().initial_data(bytes), None)
        };
        let handle = match create(bytes.as_deref().unwrap_or_default()) {
            Ok(handle) => handle,
            Err(err) => {
                log::warn!("[PIPELINE_CACHE] driver rejected cache: {err}; retrying empty");
                create(&[]).unwrap_or_else(|err| {
                    log::warn!("[PIPELINE_CACHE] creation unavailable: {err}; using uncached pipelines");
                    vk::PipelineCache::null()
                })
            }
        };
        Self { handle, storage, dirty: false }
    }

    // Device's mutex covers every pipeline creation and snapshot, including future
    // concurrent callers. Never hand out an unguarded cache handle.
    pub(super) fn with_handle<T>(&mut self, create: impl FnOnce(vk::PipelineCache) -> T) -> T {
        let result = create(self.handle);
        self.dirty = true;
        result
    }

    pub(super) fn save(&mut self, device: &ash::Device) {
        if !self.dirty || self.handle == vk::PipelineCache::null() {
            return;
        }
        let Some(storage) = &self.storage else { return };
        let result = unsafe { device.get_pipeline_cache_data(self.handle) }
            .map_err(io::Error::other)
            .and_then(|bytes| {
                storage.save(&bytes)?;
                Ok(bytes.len())
            });
        match result {
            Ok(size) => {
                self.dirty = false;
                log::info!(target: LOG, "[PIPELINE_CACHE] status=saved bytes={size} path={}", storage.path.display());
            }
            Err(err) => log::warn!("[PIPELINE_CACHE] save failed (rendering unaffected): {err}"),
        }
    }

    pub(super) fn destroy(&mut self, device: &ash::Device) {
        self.save(device);
        unsafe { device.destroy_pipeline_cache(self.handle, None) };
    }
}

impl Storage {
    fn for_process(properties: &vk::PhysicalDeviceProperties) -> io::Result<Self> {
        let exe = std::env::current_exe()?;
        // Hash the actual executable, not a Git revision, mtime, or Cargo version:
        // uncommitted Rust, embedded SPIR-V, compiler options and dependencies count.
        // Linux /proc/self/exe also survives replacement of the executable on disk.
        let image = if cfg!(target_os = "linux") { Path::new("/proc/self/exe") } else { &exe };
        let mut hasher = blake3::Hasher::new();
        let mut file = File::open(image)?;
        let mut buffer = [0; 64 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 { break; }
            hasher.update(&buffer[..count]);
        }
        let key = build_key(hasher.finalize().as_bytes(), properties);
        // One bounded slot per executable path/device, replaced across builds.
        // Different worktrees and installed copies do not evict one another.
        let mut slot = blake3::Hasher::new();
        slot.update(exe.as_os_str().as_encoded_bytes());
        slot.update(&properties.vendor_id.to_le_bytes());
        slot.update(&properties.device_id.to_le_bytes());
        let path = cache_root()?.join("re-flora/pipelines-v1").join(format!("{}.bin", slot.finalize().to_hex()));
        log::info!(target: LOG, "[PIPELINE_CACHE] build_key={}", blake3::Hash::from(key).to_hex());
        Ok(Self { path, key, vendor: properties.vendor_id, device: properties.device_id, uuid: properties.pipeline_cache_uuid })
    }

    fn load(&self) -> io::Result<Vec<u8>> {
        let file = File::open(&self.path)?;
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
        self.decode(&bytes).map(<[u8]>::to_vec)
    }

    fn decode<'a>(&self, bytes: &'a [u8]) -> io::Result<&'a [u8]> {
        if bytes.len() < HEADER_SIZE + 32 || bytes.len() as u64 > MAX_BYTES {
            return Err(invalid("invalid cache length"));
        }
        if &bytes[..8] != MAGIC || bytes[8..40] != self.key {
            return Err(invalid("build/device/driver changed or unknown format"));
        }
        let data = &bytes[HEADER_SIZE..];
        if bytes[40..HEADER_SIZE] != *blake3::hash(data).as_bytes() {
            return Err(invalid("cache checksum mismatch"));
        }
        self.validate_vulkan_header(data)?;
        Ok(data)
    }

    fn validate_vulkan_header(&self, data: &[u8]) -> io::Result<()> {
        if data.len() < 32 { return Err(invalid("truncated Vulkan header")); }
        let word = |offset| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        if word(0) != 32 || word(4) != 1 || word(8) != self.vendor || word(12) != self.device || data[16..32] != self.uuid {
            return Err(invalid("incompatible Vulkan cache header"));
        }
        Ok(())
    }

    fn save(&self, data: &[u8]) -> io::Result<()> {
        self.validate_vulkan_header(data)?;
        if data.len() as u64 + HEADER_SIZE as u64 > MAX_BYTES {
            return Err(invalid("cache exceeds size limit"));
        }
        let parent = self.path.parent().ok_or_else(|| invalid("cache parent missing"))?;
        std::fs::create_dir_all(parent)?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(MAGIC)?;
        temporary.write_all(&self.key)?;
        temporary.write_all(blake3::hash(data).as_bytes())?;
        temporary.write_all(data)?;
        temporary.as_file().sync_all()?;
        // Unique temporary files and atomic replacement: concurrent app processes
        // may lose cache hits, but can never publish partially written cache data.
        temporary.persist(&self.path).map_err(|err| err.error)?;
        Ok(())
    }
}

fn build_key(executable_hash: &[u8; 32], properties: &vk::PhysicalDeviceProperties) -> [u8; 32] {
    let mut key = blake3::Hasher::new();
    key.update(MAGIC);
    key.update(executable_hash);
    for value in [properties.vendor_id, properties.device_id, properties.driver_version, properties.api_version, usize::BITS] {
        key.update(&value.to_le_bytes());
    }
    key.update(&properties.pipeline_cache_uuid);
    *key.finalize().as_bytes()
}

fn invalid(message: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, message) }

fn cache_root() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("RE_FLORA_PIPELINE_CACHE_DIR").filter(|s| !s.is_empty()) {
        return Ok(path.into());
    }
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Caches"));
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let root = std::env::var_os("XDG_CACHE_HOME").filter(|p| Path::new(p).is_absolute())
        .map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".cache")));
    root.ok_or_else(|| invalid("no user cache directory"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(path: PathBuf) -> (Storage, Vec<u8>) {
        let storage = Storage { path, key: [7; 32], vendor: 123, device: 456, uuid: [9; 16] };
        let mut data = Vec::new();
        for word in [32u32, 1, storage.vendor, storage.device] { data.extend(word.to_le_bytes()); }
        data.extend(storage.uuid);
        data.extend(b"opaque driver data");
        (storage, data)
    }

    #[test]
    fn saved_cache_round_trips_and_changed_build_or_corruption_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let (mut storage, data) = fixture(dir.path().join("cache.bin"));
        assert_eq!(storage.load().unwrap_err().kind(), io::ErrorKind::NotFound);
        storage.save(&data).unwrap();
        assert_eq!(storage.load().unwrap(), data);
        storage.key[0] ^= 1;
        assert!(storage.load().is_err());
        storage.key[0] ^= 1;
        let bytes = std::fs::read(&storage.path).unwrap();
        for end in 0..bytes.len() { assert!(storage.decode(&bytes[..end]).is_err()); }
        for offset in 0..bytes.len() {
            let mut damaged = bytes.clone();
            damaged[offset] ^= 1;
            assert!(storage.decode(&damaged).is_err(), "offset={offset}");
        }
    }

    #[test]
    fn device_driver_uuid_and_executable_all_invalidate_identity() {
        let base = vk::PhysicalDeviceProperties::default();
        let key = build_key(&[1; 32], &base);
        assert_ne!(key, build_key(&[2; 32], &base));
        for field in 0..5 {
            let mut props = base;
            match field {
                0 => props.vendor_id += 1,
                1 => props.device_id += 1,
                2 => props.driver_version += 1,
                3 => props.api_version += 1,
                _ => props.pipeline_cache_uuid[0] += 1,
            }
            assert_ne!(key, build_key(&[1; 32], &props));
        }
    }

    #[test]
    fn incompatible_driver_header_is_never_written_and_failed_save_preserves_old_data() {
        let dir = tempfile::tempdir().unwrap();
        let (storage, data) = fixture(dir.path().join("cache.bin"));
        storage.save(&data).unwrap();
        for offset in [0, 4, 8, 12, 16] {
            let mut bad = data.clone();
            bad[offset] ^= 1;
            assert!(storage.save(&bad).is_err());
            assert_eq!(storage.load().unwrap(), data);
        }
        let (unwritable, _) = fixture(storage.path.join("cannot-be-a-directory"));
        assert!(unwritable.save(&data).is_err());
        assert_eq!(storage.load().unwrap(), data);
    }

    #[test]
    fn concurrent_writers_publish_only_complete_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache.bin");
        std::thread::scope(|scope| {
            for n in 0..8 {
                let path = path.clone();
                scope.spawn(move || {
                    let (storage, mut data) = fixture(path);
                    data.push(n);
                    for _ in 0..4 { storage.save(&data).unwrap(); assert!(storage.load().is_ok()); }
                });
            }
        });
        assert!(fixture(path).0.load().is_ok());
    }
}
