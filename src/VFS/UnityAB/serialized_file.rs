// Adapted from Myrtle (assets/unpacker/src/unity).
use std::io;

use super::endian_reader::{EndianReader, invalid_data};
use super::type_tree::{TypeTreeNode, read_type_tree_blob};

#[allow(dead_code)]
pub struct SerializedFile {
    pub objects: Vec<ObjectInfo>,
    pub types: Vec<SerializedType>,
    /// `m_RefTypes` (version >= 20): the type trees of `[SerializeReference]` payloads stored in
    /// `ManagedReferencesRegistry` fields, matched by class/namespace/assembly.
    pub ref_types: Vec<SerializedType>,
    pub unity_version: String,
    pub target_platform: i32,
    pub enable_type_tree: bool,
    pub data: Vec<u8>,
    pub big_endian: bool,
    /// External file dependencies (`m_Externals`), indexed by a `PPtr`'s `m_FileID`.
    pub externals: Vec<FileIdentifier>,
}

/// One entry of a `SerializedFile`'s `m_Externals` dependency table.
#[allow(dead_code)]
pub struct FileIdentifier {
    pub path: String,
}

#[allow(dead_code)]
pub struct ObjectInfo {
    pub path_id: i64,
    pub byte_start: u64,
    pub byte_size: u32,
    pub type_index: usize,
    pub class_id: i32,
}

#[allow(dead_code)]
pub struct SerializedType {
    pub class_id: i32,
    pub type_tree: Option<TypeTreeNode>,
    pub script_id: [u8; 16],
    pub class_name: String,
    pub namespace: String,
    pub assembly_name: String,
}

/// Read a non-negative element count.
fn read_count(r: &mut EndianReader, what: &str) -> Result<usize, io::Error> {
    let count = r.read_i32()?;
    usize::try_from(count).map_err(|_| invalid_data(format!("negative {what} count: {count}")))
}

fn read_serialized_type(
    r: &mut EndianReader,
    version: u32,
    enable_type_tree: bool,
    is_ref_type: bool,
) -> Result<SerializedType, io::Error> {
    let class_id = r.read_i32()?;

    let _is_stripped = if version >= 16 { r.read_bool()? } else { false };
    let script_type_index = if version >= 17 { r.read_i16()? } else { -1 };

    // `m_ScriptID` (16 bytes) is present for SCRIPT types only: MonoBehaviour
    // (class 114) since v16, negative class ids before that.
    let mut script_id = [0u8; 16];
    if (is_ref_type && script_type_index >= 0)
        || (version >= 16 && class_id == 114)
        || (version < 16 && class_id < 0)
    {
        script_id.copy_from_slice(&r.read_bytes(16)?); // m_ScriptID
    }
    r.read_bytes(16)?; // m_OldTypeHash

    let mut serialized_type = SerializedType {
        class_id,
        type_tree: None,
        script_id,
        class_name: String::new(),
        namespace: String::new(),
        assembly_name: String::new(),
    };
    if enable_type_tree {
        serialized_type.type_tree = read_type_tree_blob(r)?;
        if version >= 21 {
            if is_ref_type {
                serialized_type.class_name = r.read_cstring()?;
                serialized_type.namespace = r.read_cstring()?;
                serialized_type.assembly_name = r.read_cstring()?;
            } else {
                // Type dependencies array: count (i32) + count * i32
                let dep_count = r.read_i32()?;
                if dep_count > 0 {
                    r.read_bytes((dep_count as usize) * 4)?;
                }
            }
        }
    }
    Ok(serialized_type)
}

impl SerializedFile {
    pub fn parse(data: Vec<u8>) -> Result<Self, io::Error> {
        let mut r = EndianReader::new(&data, true);

        let _metadata_size = r.read_u32()?;
        let mut file_size = u64::from(r.read_u32()?);
        let version = r.read_u32()?;

        let mut data_offset = u64::from(r.read_u32()?);
        let endian = r.read_u8()?;
        let _ = r.read_bytes(3)?;

        let big_endian = endian != 0;

        if version >= 22 {
            let _metadata_size = r.read_u32()?;
            file_size = r.read_u64()?;
            data_offset = r.read_u64()?;
            let _ = r.read_u64()?;
        }

        // Reject blobs that are not SerializedFiles before trusting any count in them.
        if !(1..100).contains(&version) || data_offset == 0 || file_size <= data_offset {
            return Err(invalid_data("invalid SerializedFile header"));
        }

        r.set_big_endian(big_endian);

        let unity_version = r.read_cstring()?;
        let target_platform = r.read_i32()?;
        let enable_type_tree = r.read_bool()?;
        let type_count = read_count(&mut r, "type")?;

        let mut types = Vec::with_capacity(type_count.min(r.remaining()));
        for _ in 0..type_count {
            types.push(read_serialized_type(&mut r, version, enable_type_tree, false)?);
        }

        let object_count = read_count(&mut r, "object")?;
        let mut objects = Vec::with_capacity(object_count.min(r.remaining()));
        for _ in 0..object_count {
            if version >= 14 {
                r.align(4);
            }
            let path_id = r.read_i64()?;
            let mut byte_start = if version >= 22 {
                r.read_u64()?
            } else {
                u64::from(r.read_u32()?)
            };
            byte_start = byte_start
                .checked_add(data_offset) // absolute offset into data
                .ok_or_else(|| invalid_data("object offset overflow"))?;
            let byte_size = r.read_u32()?;
            let raw_type_index = r.read_i32()?;
            let type_index = usize::try_from(raw_type_index)
                .ok()
                .filter(|&index| index < types.len())
                .ok_or_else(|| invalid_data(format!("invalid type index: {raw_type_index}")))?;
            let class_id = types[type_index].class_id;

            objects.push(ObjectInfo {
                path_id,
                byte_start,
                byte_size,
                type_index,
                class_id,
            });
        }

        // ScriptTypes (`m_ScriptTypes`) — present since version 11; skipped.
        if version >= 11 {
            let script_count = r.read_i32()?;
            for _ in 0..script_count.max(0) {
                let _local_file_index = r.read_i32()?;
                if version >= 14 {
                    r.align(4);
                    let _local_id = r.read_i64()?;
                } else {
                    let _local_id = i64::from(r.read_i32()?);
                }
            }
        }

        // Externals (`m_Externals`): the dependency table PPtrs point into.
        let mut externals = Vec::new();
        if let Ok(ext_count) = r.read_i32() {
            for _ in 0..ext_count.max(0) {
                if version >= 6 {
                    let _temp_empty = r.read_cstring()?;
                }
                if version >= 5 {
                    let _guid = r.read_bytes(16)?;
                    let _type = r.read_i32()?;
                }
                let path = r.read_cstring()?;
                externals.push(FileIdentifier { path });
            }
        }

        // RefTypes (`m_RefTypes`) — present since version 20.
        let mut ref_types = Vec::new();
        if version >= 20 {
            let ref_type_count = read_count(&mut r, "ref type")?;
            for _ in 0..ref_type_count {
                ref_types.push(read_serialized_type(&mut r, version, enable_type_tree, true)?);
            }
        }

        Ok(Self {
            objects,
            types,
            ref_types,
            unity_version,
            target_platform,
            enable_type_tree,
            data,
            big_endian,
            externals,
        })
    }
}
