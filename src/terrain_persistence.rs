use anyhow::{Context, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;

pub const TERRAIN_FORMAT_VERSION: u32 = 1;
pub const TERRAIN_VOXEL_SCHEMA_ID: u32 = 1;
pub const TERRAIN_BYTES_PER_VOXEL: u32 = 1;
pub const TERRAIN_RAW_CODEC: u32 = 0;

const MAGIC: [u8; 8] = *b"RFLTRN\0\x01";
const HEADER_LEN: usize = 64;
const RECORD_HEADER_LEN: usize = 40;
const MAX_CHUNK_COUNT: u64 = 4096;
const MAX_CHUNK_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_FILE_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainSnapshotMetadata {
    pub chunk_dim: [u32; 3],
    pub voxel_dim_per_chunk: [u32; 3],
    pub voxel_schema_id: u32,
}

impl TerrainSnapshotMetadata {
    pub fn new(chunk_dim: [u32; 3], voxel_dim_per_chunk: [u32; 3]) -> Self {
        Self {
            chunk_dim,
            voxel_dim_per_chunk,
            voxel_schema_id: TERRAIN_VOXEL_SCHEMA_ID,
        }
    }

    pub fn chunk_count(self) -> Result<u64> {
        checked_product(self.chunk_dim, "chunk dimensions")
    }

    pub fn chunk_byte_len(self) -> Result<u64> {
        let voxel_count = checked_product(self.voxel_dim_per_chunk, "voxel dimensions")?;
        voxel_count
            .checked_mul(TERRAIN_BYTES_PER_VOXEL as u64)
            .context("voxel byte length overflow")
    }

    fn validate(self) -> Result<()> {
        let chunk_count = self.chunk_count()?;
        anyhow::ensure!(chunk_count > 0, "chunk dimensions must be non-zero");
        anyhow::ensure!(
            chunk_count <= MAX_CHUNK_COUNT,
            "chunk count {} exceeds maximum {}",
            chunk_count,
            MAX_CHUNK_COUNT
        );
        anyhow::ensure!(
            self.voxel_schema_id == TERRAIN_VOXEL_SCHEMA_ID,
            "unsupported voxel schema {}",
            self.voxel_schema_id
        );

        let chunk_bytes = self.chunk_byte_len()?;
        anyhow::ensure!(chunk_bytes > 0, "voxel dimensions must be non-zero");
        anyhow::ensure!(
            chunk_bytes <= MAX_CHUNK_BYTES,
            "chunk payload {} bytes exceeds maximum {}",
            chunk_bytes,
            MAX_CHUNK_BYTES
        );
        let expected_file_bytes = (HEADER_LEN as u64)
            .checked_add(
                chunk_count
                    .checked_mul((RECORD_HEADER_LEN as u64).saturating_add(chunk_bytes))
                    .context("snapshot file length overflow")?,
            )
            .context("snapshot file length overflow")?;
        anyhow::ensure!(
            expected_file_bytes <= MAX_TOTAL_FILE_BYTES,
            "snapshot file {} bytes exceeds maximum {}",
            expected_file_bytes,
            MAX_TOTAL_FILE_BYTES
        );
        Ok(())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct TerrainSnapshotChunk {
    pub coordinate: [u32; 3],
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerrainSnapshotSummary {
    pub metadata: TerrainSnapshotMetadata,
    pub chunk_count: u64,
    pub payload_bytes: u64,
}

pub struct TerrainSnapshotWriter {
    file: NamedTempFile,
    destination: PathBuf,
    metadata: TerrainSnapshotMetadata,
    seen: Vec<bool>,
    written_chunks: u64,
    payload_bytes: u64,
}

impl TerrainSnapshotWriter {
    pub fn create(path: impl AsRef<Path>, metadata: TerrainSnapshotMetadata) -> Result<Self> {
        metadata.validate()?;
        let destination = path.as_ref().to_path_buf();
        let parent = destination.parent().unwrap_or_else(|| Path::new("."));
        let file = NamedTempFile::new_in(parent).with_context(|| {
            format!("create temporary terrain snapshot in {}", parent.display())
        })?;
        let chunk_count = metadata.chunk_count()? as usize;
        let mut writer = Self {
            file,
            destination,
            metadata,
            seen: vec![false; chunk_count],
            written_chunks: 0,
            payload_bytes: 0,
        };
        writer.write_header()?;
        Ok(writer)
    }

    pub fn write_chunk(&mut self, coordinate: [u32; 3], bytes: &[u8]) -> Result<()> {
        let index = self.coordinate_index(coordinate)?;
        anyhow::ensure!(
            !self.seen[index],
            "duplicate terrain chunk {:?}",
            coordinate
        );
        anyhow::ensure!(
            bytes.len() as u64 == self.metadata.chunk_byte_len()?,
            "terrain chunk {:?} has {} bytes, expected {}",
            coordinate,
            bytes.len(),
            self.metadata.chunk_byte_len()?
        );
        anyhow::ensure!(
            self.written_chunks < self.metadata.chunk_count()?,
            "too many terrain chunks"
        );

        let decoded_len = bytes.len() as u64;
        let mut record = [0u8; RECORD_HEADER_LEN];
        put_u32(&mut record, 0, coordinate[0]);
        put_u32(&mut record, 4, coordinate[1]);
        put_u32(&mut record, 8, coordinate[2]);
        put_u32(&mut record, 12, TERRAIN_RAW_CODEC);
        put_u64(&mut record, 16, decoded_len);
        put_u64(&mut record, 24, decoded_len);
        put_u32(&mut record, 32, checksum(bytes));
        let record_checksum = checksum(&record[..36]);
        put_u32(&mut record, 36, record_checksum);

        self.file
            .as_file_mut()
            .write_all(&record)
            .context("write terrain chunk record")?;
        self.file
            .as_file_mut()
            .write_all(bytes)
            .with_context(|| format!("write terrain chunk {:?}", coordinate))?;

        self.seen[index] = true;
        self.written_chunks += 1;
        self.payload_bytes += decoded_len;
        Ok(())
    }

    pub fn finish(mut self) -> Result<TerrainSnapshotSummary> {
        anyhow::ensure!(
            self.written_chunks == self.metadata.chunk_count()?,
            "snapshot is missing terrain chunks: wrote {}, expected {}",
            self.written_chunks,
            self.metadata.chunk_count()?
        );
        anyhow::ensure!(
            self.seen.iter().all(|seen| *seen),
            "snapshot has missing terrain chunks"
        );

        self.file
            .as_file_mut()
            .flush()
            .context("flush terrain snapshot")?;
        self.file
            .as_file()
            .sync_all()
            .context("synchronize terrain snapshot")?;
        self.file
            .persist(&self.destination)
            .map_err(|error| error.error)
            .with_context(|| format!("atomically replace {}", self.destination.display()))?;

        sync_parent_directory(&self.destination)?;
        Ok(TerrainSnapshotSummary {
            metadata: self.metadata,
            chunk_count: self.written_chunks,
            payload_bytes: self.payload_bytes,
        })
    }

    fn write_header(&mut self) -> Result<()> {
        let mut header = [0u8; HEADER_LEN];
        header[..MAGIC.len()].copy_from_slice(&MAGIC);
        put_u32(&mut header, 8, TERRAIN_FORMAT_VERSION);
        put_u32(&mut header, 12, HEADER_LEN as u32);
        put_u32(&mut header, 16, self.metadata.chunk_dim[0]);
        put_u32(&mut header, 20, self.metadata.chunk_dim[1]);
        put_u32(&mut header, 24, self.metadata.chunk_dim[2]);
        put_u32(&mut header, 28, self.metadata.voxel_dim_per_chunk[0]);
        put_u32(&mut header, 32, self.metadata.voxel_dim_per_chunk[1]);
        put_u32(&mut header, 36, self.metadata.voxel_dim_per_chunk[2]);
        put_u32(&mut header, 40, TERRAIN_BYTES_PER_VOXEL);
        put_u32(&mut header, 44, self.metadata.voxel_schema_id);
        put_u32(&mut header, 48, self.metadata.chunk_count()? as u32);
        put_u32(&mut header, 52, RECORD_HEADER_LEN as u32);
        put_u32(&mut header, 56, 0);
        let header_checksum = checksum(&header[..60]);
        put_u32(&mut header, 60, header_checksum);
        self.file
            .as_file_mut()
            .write_all(&header)
            .context("write terrain snapshot header")?;
        Ok(())
    }

    fn coordinate_index(&self, coordinate: [u32; 3]) -> Result<usize> {
        coordinate_index(self.metadata.chunk_dim, coordinate)
    }
}

pub struct TerrainSnapshotReader {
    file: File,
    path: PathBuf,
    metadata: TerrainSnapshotMetadata,
    seen: Vec<bool>,
    read_chunks: u64,
    payload_bytes: u64,
    file_len: u64,
    finished: bool,
}

impl TerrainSnapshotReader {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut file = File::open(&path).with_context(|| format!("open {}", path.display()))?;
        let file_len = file
            .metadata()
            .with_context(|| format!("stat {}", path.display()))?
            .len();
        anyhow::ensure!(
            file_len >= HEADER_LEN as u64,
            "terrain snapshot is truncated before the header"
        );

        let mut header = [0u8; HEADER_LEN];
        file.read_exact(&mut header)
            .with_context(|| format!("read terrain snapshot header from {}", path.display()))?;
        let metadata = parse_header(&header)?;
        metadata.validate()?;
        let expected_file_len = (HEADER_LEN as u64)
            .checked_add(
                metadata
                    .chunk_count()?
                    .checked_mul(
                        (RECORD_HEADER_LEN as u64)
                            .checked_add(metadata.chunk_byte_len()?)
                            .context("snapshot file length overflow")?,
                    )
                    .context("snapshot file length overflow")?,
            )
            .context("snapshot file length overflow")?;
        anyhow::ensure!(
            file_len == expected_file_len,
            "terrain snapshot length {} does not match expected {}",
            file_len,
            expected_file_len
        );

        Ok(Self {
            file,
            path,
            metadata,
            seen: vec![false; metadata.chunk_count()? as usize],
            read_chunks: 0,
            payload_bytes: 0,
            file_len,
            finished: false,
        })
    }

    pub fn validate(
        path: impl AsRef<Path>,
        expected: TerrainSnapshotMetadata,
    ) -> Result<TerrainSnapshotSummary> {
        let mut reader = Self::open(path)?;
        anyhow::ensure!(
            reader.metadata == expected,
            "terrain snapshot metadata {:?} does not match expected {:?}",
            reader.metadata,
            expected
        );
        reader.drain()?;
        reader.summary()
    }

    pub fn metadata(&self) -> TerrainSnapshotMetadata {
        self.metadata
    }

    pub fn read_next_chunk(&mut self) -> Result<Option<TerrainSnapshotChunk>> {
        if self.finished {
            return Ok(None);
        }
        if self.read_chunks == self.metadata.chunk_count()? {
            self.ensure_end()?;
            self.finished = true;
            return Ok(None);
        }

        let mut record = [0u8; RECORD_HEADER_LEN];
        let record_number = self.read_chunks + 1;
        let record_count = self.metadata.chunk_count()?;
        self.file.read_exact(&mut record).with_context(|| {
            format!(
                "read terrain chunk record {}/{} from {}",
                record_number,
                record_count,
                self.path.display()
            )
        })?;
        anyhow::ensure!(
            checksum(&record[..36]) == read_u32(&record, 36),
            "terrain chunk record checksum failed for record {}",
            self.read_chunks
        );

        let coordinate = [
            read_u32(&record, 0),
            read_u32(&record, 4),
            read_u32(&record, 8),
        ];
        let index = coordinate_index(self.metadata.chunk_dim, coordinate)?;
        anyhow::ensure!(
            !self.seen[index],
            "duplicate terrain chunk {:?}",
            coordinate
        );
        anyhow::ensure!(
            read_u32(&record, 12) == TERRAIN_RAW_CODEC,
            "unsupported terrain chunk codec {} for {:?}",
            read_u32(&record, 12),
            coordinate
        );

        let decoded_len = read_u64(&record, 16);
        let stored_len = read_u64(&record, 24);
        let expected_len = self.metadata.chunk_byte_len()?;
        anyhow::ensure!(
            decoded_len == expected_len && stored_len == expected_len,
            "terrain chunk {:?} has decoded/stored lengths {}/{}; expected {}",
            coordinate,
            decoded_len,
            stored_len,
            expected_len
        );
        anyhow::ensure!(stored_len <= MAX_CHUNK_BYTES, "terrain chunk is too large");

        let mut bytes = vec![0u8; stored_len as usize];
        self.file
            .read_exact(&mut bytes)
            .with_context(|| format!("read terrain chunk {:?}", coordinate))?;
        anyhow::ensure!(
            checksum(&bytes) == read_u32(&record, 32),
            "terrain chunk payload checksum failed for {:?}",
            coordinate
        );

        self.seen[index] = true;
        self.read_chunks += 1;
        self.payload_bytes += stored_len;
        Ok(Some(TerrainSnapshotChunk { coordinate, bytes }))
    }

    pub fn finish(&mut self) -> Result<TerrainSnapshotSummary> {
        self.drain()?;
        self.summary()
    }

    fn drain(&mut self) -> Result<()> {
        while self.read_next_chunk()?.is_some() {}
        anyhow::ensure!(
            self.seen.iter().all(|seen| *seen),
            "terrain snapshot has missing chunks"
        );
        Ok(())
    }

    fn ensure_end(&mut self) -> Result<()> {
        let mut trailing = [0u8; 1];
        let count = self
            .file
            .read(&mut trailing)
            .context("check terrain snapshot EOF")?;
        anyhow::ensure!(
            count == 0,
            "terrain snapshot contains trailing data after all chunks"
        );
        Ok(())
    }

    fn summary(&self) -> Result<TerrainSnapshotSummary> {
        anyhow::ensure!(
            self.finished,
            "terrain snapshot reader did not reach a validated EOF"
        );
        anyhow::ensure!(
            self.file_len >= HEADER_LEN as u64,
            "terrain snapshot file length is invalid"
        );
        Ok(TerrainSnapshotSummary {
            metadata: self.metadata,
            chunk_count: self.read_chunks,
            payload_bytes: self.payload_bytes,
        })
    }
}

pub fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn parse_header(header: &[u8; HEADER_LEN]) -> Result<TerrainSnapshotMetadata> {
    anyhow::ensure!(
        header[..MAGIC.len()] == MAGIC,
        "invalid terrain snapshot magic"
    );
    anyhow::ensure!(
        read_u32(header, 8) == TERRAIN_FORMAT_VERSION,
        "unsupported terrain snapshot version {}",
        read_u32(header, 8)
    );
    anyhow::ensure!(
        read_u32(header, 12) == HEADER_LEN as u32,
        "unsupported terrain snapshot header length {}",
        read_u32(header, 12)
    );
    anyhow::ensure!(
        checksum(&header[..60]) == read_u32(header, 60),
        "terrain snapshot header checksum failed"
    );
    anyhow::ensure!(
        read_u32(header, 40) == TERRAIN_BYTES_PER_VOXEL,
        "unsupported terrain bytes per voxel {}",
        read_u32(header, 40)
    );
    anyhow::ensure!(
        read_u32(header, 52) == RECORD_HEADER_LEN as u32,
        "unsupported terrain record header length {}",
        read_u32(header, 52)
    );
    let metadata = TerrainSnapshotMetadata {
        chunk_dim: [
            read_u32(header, 16),
            read_u32(header, 20),
            read_u32(header, 24),
        ],
        voxel_dim_per_chunk: [
            read_u32(header, 28),
            read_u32(header, 32),
            read_u32(header, 36),
        ],
        voxel_schema_id: read_u32(header, 44),
    };
    anyhow::ensure!(
        read_u32(header, 48) as u64 == metadata.chunk_count()?,
        "terrain snapshot chunk count metadata does not match dimensions"
    );
    metadata.validate()?;
    Ok(metadata)
}

fn coordinate_index(chunk_dim: [u32; 3], coordinate: [u32; 3]) -> Result<usize> {
    anyhow::ensure!(
        coordinate[0] < chunk_dim[0]
            && coordinate[1] < chunk_dim[1]
            && coordinate[2] < chunk_dim[2],
        "terrain chunk coordinate {:?} is outside {:?}",
        coordinate,
        chunk_dim
    );
    let index = (coordinate[0] as u64)
        .checked_add(
            (coordinate[1] as u64)
                .checked_mul(chunk_dim[0] as u64)
                .context("chunk index overflow")?,
        )
        .and_then(|value| {
            value.checked_add(
                (coordinate[2] as u64)
                    .checked_mul(chunk_dim[0] as u64)
                    .and_then(|value| value.checked_mul(chunk_dim[1] as u64))?,
            )
        })
        .context("chunk index overflow")?;
    usize::try_from(index).context("chunk index does not fit usize")
}

fn checked_product(values: [u32; 3], label: &str) -> Result<u64> {
    values
        .into_iter()
        .map(u64::from)
        .try_fold(1u64, |product, value| {
            product
                .checked_mul(value)
                .with_context(|| format!("{label} product overflow"))
        })
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn sync_parent_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        File::open(parent)
            .with_context(|| format!("open snapshot parent directory {}", parent.display()))?
            .sync_all()
            .with_context(|| {
                format!("synchronize snapshot parent directory {}", parent.display())
            })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, OpenOptions};
    use std::io::{Seek, SeekFrom, Write};
    use tempfile::tempdir;

    fn metadata() -> TerrainSnapshotMetadata {
        TerrainSnapshotMetadata::new([2, 1, 2], [2, 2, 2])
    }

    fn chunks() -> Vec<TerrainSnapshotChunk> {
        (0..2)
            .flat_map(|z| (0..1).flat_map(move |y| (0..2).map(move |x| [x, y, z])))
            .map(|coordinate| TerrainSnapshotChunk {
                coordinate,
                bytes: (0..8)
                    .map(|index| index as u8 + coordinate[0] as u8 + coordinate[2] as u8 * 3)
                    .collect(),
            })
            .collect()
    }

    fn write_snapshot(path: &Path, order: &[usize]) -> Result<TerrainSnapshotSummary> {
        let all_chunks = chunks();
        let mut writer = TerrainSnapshotWriter::create(path, metadata())?;
        for &index in order {
            let chunk = &all_chunks[index];
            writer.write_chunk(chunk.coordinate, &chunk.bytes)?;
        }
        writer.finish()
    }

    #[test]
    fn raw_format_is_deterministic_and_roundtrips() {
        let dir = tempdir().unwrap();
        let first = dir.path().join("first.rfterrain");
        let second = dir.path().join("second.rfterrain");
        let order = [0, 1, 2, 3];
        let first_summary = write_snapshot(&first, &order).unwrap();
        let second_summary = write_snapshot(&second, &order).unwrap();
        assert_eq!(first_summary, second_summary);
        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());

        let mut reader = TerrainSnapshotReader::open(&first).unwrap();
        assert_eq!(reader.metadata(), metadata());
        let mut loaded = Vec::new();
        while let Some(chunk) = reader.read_next_chunk().unwrap() {
            loaded.push(chunk);
        }
        assert_eq!(reader.finish().unwrap(), first_summary);
        assert_eq!(loaded, chunks());
    }

    #[test]
    fn reader_accepts_records_in_any_order() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("unordered.rfterrain");
        write_snapshot(&path, &[3, 1, 0, 2]).unwrap();
        let mut reader = TerrainSnapshotReader::open(&path).unwrap();
        let mut loaded = Vec::new();
        while let Some(chunk) = reader.read_next_chunk().unwrap() {
            loaded.push(chunk);
        }
        loaded.sort_by_key(|chunk| chunk.coordinate);
        let mut expected = chunks();
        expected.sort_by_key(|chunk| chunk.coordinate);
        assert_eq!(loaded, expected);
    }

    #[test]
    fn failed_replacement_preserves_previous_destination() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("stable.rfterrain");
        write_snapshot(&path, &[0, 1, 2, 3]).unwrap();
        let previous = fs::read(&path).unwrap();

        let mut writer = TerrainSnapshotWriter::create(&path, metadata()).unwrap();
        writer.write_chunk([0, 0, 0], &[0; 7]).unwrap_err();
        drop(writer);
        assert_eq!(fs::read(&path).unwrap(), previous);

        let mut incomplete = TerrainSnapshotWriter::create(&path, metadata()).unwrap();
        incomplete.write_chunk([0, 0, 0], &[0; 8]).unwrap();
        incomplete.finish().unwrap_err();
        assert_eq!(fs::read(&path).unwrap(), previous);
    }

    #[test]
    fn corrupted_header_and_payload_are_rejected() {
        let dir = tempdir().unwrap();
        let header_path = dir.path().join("bad-header.rfterrain");
        write_snapshot(&header_path, &[0, 1, 2, 3]).unwrap();
        let mut header_file = OpenOptions::new().write(true).open(&header_path).unwrap();
        header_file.seek(SeekFrom::Start(0)).unwrap();
        header_file.write_all(b"BADMAGIC").unwrap();
        drop(header_file);
        assert!(TerrainSnapshotReader::open(&header_path).is_err());

        let payload_path = dir.path().join("bad-payload.rfterrain");
        write_snapshot(&payload_path, &[0, 1, 2, 3]).unwrap();
        let mut payload_file = OpenOptions::new().write(true).open(&payload_path).unwrap();
        payload_file
            .seek(SeekFrom::Start((HEADER_LEN + RECORD_HEADER_LEN) as u64))
            .unwrap();
        payload_file.write_all(&[0xff]).unwrap();
        drop(payload_file);
        assert!(TerrainSnapshotReader::open(&payload_path)
            .unwrap()
            .finish()
            .is_err());
    }

    #[test]
    fn metadata_mismatch_is_rejected_before_reading_chunks() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("metadata.rfterrain");
        write_snapshot(&path, &[0, 1, 2, 3]).unwrap();
        let incompatible = TerrainSnapshotMetadata::new([1, 1, 4], [2, 2, 2]);
        assert!(TerrainSnapshotReader::validate(&path, incompatible).is_err());
    }

    #[test]
    fn trailing_data_is_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("trailing.rfterrain");
        write_snapshot(&path, &[0, 1, 2, 3]).unwrap();
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(&[0x42]).unwrap();
        drop(file);
        assert!(TerrainSnapshotReader::open(&path).is_err());
    }

    #[test]
    fn checksum_matches_standard_crc32_vector() {
        assert_eq!(checksum(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn metadata_bounds_are_enforced() {
        let too_many = TerrainSnapshotMetadata::new([17, 17, 17], [1, 1, 1]);
        assert!(too_many.validate().is_err());
        let too_large = TerrainSnapshotMetadata::new([1, 1, 1], [512, 512, 512]);
        assert!(too_large.validate().is_err());
    }
}
