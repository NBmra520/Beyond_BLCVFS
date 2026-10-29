//! Rebuild Endfield SerializedFile nodes (`CAB-*` + `.resS`) into standalone
//! UnityFS `.ab` bundles that stock AssetStudio (game type Normal) can read.
//!
//! The package layer already decrypts the per-file payload, but each `.ab`
//! payload is still an encrypted VFS container. [`export_bundles`] peels that
//! container (nodes = `CAB-*` plus an optional `.resS`), strips the
//! Endfield-only fields described by the embedded type trees, then wraps the
//! result in a fresh UnityFS v7 container.
//!
//! Note that this rebuild is lossy: the Endfield-only fields are dropped, so
//! some information is lost. For the complete bundle, export the raw bytes
//! without decryption and load them in AnimeStudio (game type
//! `ArknightsEndfield`).

mod crypto_constants;
mod endian_reader;
mod lz4_inv;
mod serialized_file;
mod type_tree;
mod vfs_aes;
mod vfs_decrypt;

use endian_reader::EndianReader;
use serialized_file::SerializedFile;
use type_tree::{TypeTreeNode, read_type_tree_blob};
use std::io;
use std::io::Write;

const BLOCK_SIZE: usize = 128 * 1024;
const MAGIC: &[u8; 8] = b"UnityFS\0";
/// Zero pad used to align a bundle entry without growing the entry itself.
const PADDING: [u8; 16] = [0; 16];

/// A SerializedFile's `(version, unity_version)`, or `None` when `data` is not
/// a valid SerializedFile.
pub fn serialized_file_info(data: &[u8]) -> Option<(u32, String)> {
    if data.len() < 48 {
        return None;
    }
    let version = u32::from_be_bytes(data[8..12].try_into().ok()?);
    if !(1..100).contains(&version) {
        return None;
    }

    let (header_len, file_size, data_offset) = if version >= 22 {
        (
            48usize,
            u64::from_be_bytes(data[24..32].try_into().ok()?),
            u64::from_be_bytes(data[32..40].try_into().ok()?),
        )
    } else {
        (
            20usize,
            u64::from(u32::from_be_bytes(data[4..8].try_into().ok()?)),
            u64::from(u32::from_be_bytes(data[12..16].try_into().ok()?)),
        )
    };

    if file_size != data.len() as u64 || data_offset == 0 || data_offset >= file_size {
        return None;
    }

    let rest = &data[header_len..];
    let end = rest.iter().position(|&b| b == 0)?;
    let unity_version = std::str::from_utf8(&rest[..end]).ok()?.to_string();
    if unity_version.is_empty() {
        return None;
    }

    Some((version, unity_version))
}

/// True when `path` names a lone SerializedFile node (paired with an optional
/// `.resS`/`.resource` sibling stream): either a `.ab` file or a bare `CAB-*`.
pub fn is_bundle_node(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.ends_with(".resS") || name.ends_with(".resource") {
        return false;
    }
    name.ends_with(".ab") || name.starts_with("CAB-")
}

/// Output path for a rebuilt bundle: keep an existing `.ab`, otherwise append it.
pub fn ab_output_name(path: &str) -> String {
    if path.ends_with(".ab") {
        path.to_string()
    } else {
        format!("{path}.ab")
    }
}

pub struct RebuiltBundle {
    pub bytes: Vec<u8>,
    pub stripped: usize,
    pub entries: usize,
    pub blocks: usize,
    pub verified: bool,
}

/// Rebuild one SerializedFile into a UnityFS bundle. `resource` is the sibling
/// `.resS` stream referenced by the bundle's directory entry.
pub fn rebuild(
    name: &str,
    serialized: &[u8],
    resource: Option<&[u8]>,
) -> io::Result<RebuiltBundle> {
    let (version, unity_version) = serialized_file_info(serialized)
        .ok_or_else(|| invalid("not a SerializedFile"))?;

    let mut sf = serialized.to_vec();
    let stripped = strip_endfield_fields(&mut sf, version)?;
    let serialized_len = sf.len();

    let mut entries = vec![DirectoryEntry {
        offset: 0,
        size: serialized_len as i64,
        flags: 4, // SerializedFile
        path: name.to_string(),
    }];
    let mut segments: Vec<&[u8]> = Vec::with_capacity(3);
    segments.push(&sf);
    if let Some(resource) = resource {
        // The resource stream starts on a 16-byte boundary; the serialized file
        // entry itself keeps its exact size so its header `m_FileSize` stays in
        // sync with the entry stream length (AssetStudio validates that).
        let aligned_len = (serialized_len + 15) & !15;
        entries.push(DirectoryEntry {
            offset: aligned_len as i64,
            size: resource.len() as i64,
            flags: 0, // resource stream
            path: format!("{name}.resS"),
        });
        if aligned_len > serialized_len {
            segments.push(&PADDING[..aligned_len - serialized_len]);
        }
        segments.push(resource);
    }

    let mut bytes = Vec::new();
    let (_, blocks, verified) =
        write_bundle(&mut bytes, &segments, &entries, 7, "5.x.x", &unity_version, 0x40)?;

    Ok(RebuiltBundle {
        bytes,
        stripped,
        entries: entries.len(),
        blocks,
        verified,
    })
}

/// One file produced by [`export_bundles`]; `name` is a bare file name.
pub struct ExportedFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// True when `data` starts with an Endfield VFS container header.
pub fn is_container(data: &[u8]) -> bool {
    vfs_decrypt::is_valid_header(data)
}

/// Decrypt every VFS container found in `data`, returning its directory nodes.
fn container_nodes(data: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut nodes = Vec::new();
    let mut pos = 0;
    while pos + 8 <= data.len() {
        if !vfs_decrypt::is_valid_header(&data[pos..]) {
            pos += 1;
            continue;
        }
        match vfs_decrypt::process_vfs_at(data, pos) {
            Ok((vfs, output, consumed)) => {
                nodes.extend(vfs_decrypt::extract_files(&vfs, &output));
                if consumed == 0 {
                    break;
                }
                pos += consumed;
            }
            Err(_) => pos += 8,
        }
    }
    nodes
}

/// Turn one VFS file into the files that should be written to disk.
///
/// * `decrypt == false` — the bytes are returned untouched (raw export).
/// * `decrypt == true` — Endfield VFS containers are decrypted and every
///   SerializedFile node is rebuilt into a standalone UnityFS bundle.
pub fn export_bundles(data: &[u8], path: &str, decrypt: bool) -> io::Result<Vec<ExportedFile>> {
    let file_name = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);

    if !decrypt {
        return Ok(vec![ExportedFile {
            name: file_name.to_string(),
            bytes: data.to_vec(),
        }]);
    }

    if is_container(data) {
        let nodes = container_nodes(data);
        if nodes.is_empty() {
            return Err(invalid("no directory nodes found in VFS container"));
        }
        let stem = file_name
            .strip_suffix(".ab")
            .or_else(|| file_name.strip_suffix(".chk"))
            .unwrap_or(file_name);

        let mut out = Vec::new();
        for (node_name, node) in &nodes {
            if node_name.ends_with(".resS") || node_name.ends_with(".resource") {
                continue;
            }
            if node.starts_with(MAGIC) {
                out.push(ExportedFile {
                    name: format!("{stem}_{node_name}.ab"),
                    bytes: node.clone(),
                });
                continue;
            }
            if serialized_file_info(node).is_none() {
                continue;
            }
            let sibling = nodes
                .iter()
                .find(|(name, _)| {
                    name == &format!("{node_name}.resS") || name == &format!("{node_name}.resource")
                })
                .map(|(_, bytes)| bytes.as_slice());
            let built = rebuild(node_name, node, sibling)?;
            out.push(ExportedFile {
                name: format!("{stem}_{node_name}.ab"),
                bytes: built.bytes,
            });
        }
        if out.is_empty() {
            return Err(invalid("no reconstructable bundle in VFS container"));
        }
        // Keep the original name when the container holds a single bundle.
        if out.len() == 1 {
            out[0].name = file_name.to_string();
        }
        return Ok(out);
    }

    if serialized_file_info(data).is_some() {
        let built = rebuild(file_name, data, None)?;
        return Ok(vec![ExportedFile {
            name: ab_output_name(file_name),
            bytes: built.bytes,
        }]);
    }

    if data.starts_with(MAGIC) {
        return Ok(vec![ExportedFile {
            name: file_name.to_string(),
            bytes: data.to_vec(),
        }]);
    }

    Err(invalid("not a VFS container or SerializedFile"))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

struct DirectoryEntry {
    offset: i64,
    size: i64,
    flags: u32,
    path: String,
}

/// Field positions of every object record (`byte_start`, `byte_size`) in the
/// SerializedFile metadata table, in metadata order.
fn object_table_records(data: &[u8], file_version: u32) -> Option<Vec<(usize, usize)>> {
    let mut r = EndianReader::new(data, true);

    let _metadata_size = r.read_u32().ok()?;
    let _file_size = r.read_u32().ok()?;
    let version = r.read_u32().ok()?;
    let _data_offset = r.read_u32().ok()?;
    let endian = r.read_u8().ok()?;
    let _ = r.read_bytes(3).ok()?;

    if version >= 22 {
        let _ = r.read_u32().ok()?;
        let _ = r.read_u64().ok()?;
        let _ = r.read_u64().ok()?;
        let _ = r.read_u64().ok()?;
    }

    r.set_big_endian(endian != 0);

    let _unity_version = r.read_cstring().ok()?;
    let _target_platform = r.read_i32().ok()?;
    let enable_type_tree = r.read_bool().ok()?;

    let type_count = r.read_i32().ok()?;
    if type_count < 0 {
        return None;
    }
    for _ in 0..type_count {
        let _class_id = r.read_i32().ok()?;
        if version >= 16 {
            let _is_stripped = r.read_bool().ok()?;
        }
        let _script_type_index = if version >= 17 { r.read_i16().ok()? } else { -1 };
        // `m_ScriptID` is serialized for MonoBehaviour (114) and legacy scripts.
        if (version >= 16 && _class_id == 114) || (version < 16 && _class_id < 0) {
            let _ = r.read_bytes(16).ok()?;
        }
        let _ = r.read_bytes(16).ok()?; // m_OldTypeHash
        if enable_type_tree {
            let _ = read_type_tree_blob(&mut r).ok()?;
            if version >= 21 {
                let dep_count = r.read_i32().ok()?;
                if dep_count > 0 {
                    let _ = r.read_bytes((dep_count as usize) * 4).ok()?;
                }
            }
        }
    }

    let object_count = r.read_i32().ok()?;
    if object_count < 0 {
        return None;
    }
    let mut records = Vec::with_capacity(object_count as usize);
    for _ in 0..object_count {
        if version >= 14 {
            r.align(4);
        }
        let _path_id = r.read_i64().ok()?;
        let start_pos = r.position();
        if version >= 22 {
            let _ = r.read_u64().ok()?;
        } else {
            let _ = r.read_u32().ok()?;
        }
        let size_pos = r.position();
        let _ = r.read_u32().ok()?;
        let _ = r.read_i32().ok()?;
        records.push((start_pos, size_pos));
    }

    if version != file_version {
        return None;
    }
    Some(records)
}

/// Remove the Endfield-only fields from every object so stock AssetStudio
/// (game type Normal) can deserialize the file. Returns how many fields were
/// rewritten.
fn strip_endfield_fields(sf: &mut Vec<u8>, version: u32) -> io::Result<usize> {
    let parsed = SerializedFile::parse(sf.clone())?;
    if parsed.objects.is_empty() {
        return Ok(0);
    }

    let big_endian = parsed.big_endian;
    let records =
        object_table_records(sf, version).ok_or_else(|| invalid("cannot locate SerializedFile object table"))?;
    if records.len() != parsed.objects.len() {
        return Err(invalid(format!(
            "object table walk found {} records, expected {}",
            records.len(),
            parsed.objects.len()
        )));
    }

    let data_offset = if version >= 22 {
        u64::from_be_bytes(
            sf[32..40]
                .try_into()
                .map_err(|_| invalid("bad SerializedFile header"))?,
        )
    } else {
        u64::from(u32::from_be_bytes(
            sf[12..16]
                .try_into()
                .map_err(|_| invalid("bad SerializedFile header"))?,
        ))
    };

    // Collect every Endfield-only field the vanilla readers do not expect.
    // The type trees embedded in the SerializedFile describe the whole object
    // layout, including the extra nodes, so walking them lets us delete the
    // fields without hard-coding each class's field order.
    let mut removals: Vec<(usize, usize, usize)> = Vec::new(); // (position, len, object index)
    for (index, object) in parsed.objects.iter().enumerate() {
        let start = usize::try_from(object.byte_start).unwrap_or(usize::MAX);
        let end = start.saturating_add(object.byte_size as usize);
        if start >= end || end > sf.len() {
            continue;
        }
        let Some(type_index) = parsed.types.get(object.type_index) else {
            continue;
        };
        let Some(tree) = type_index.type_tree.as_ref() else {
            continue;
        };
        let Some((walk_end, fields)) =
            walk_object(sf, start, tree, type_index.class_id, big_endian)
        else {
            continue;
        };
        if walk_end != end {
            continue;
        }
        removals.extend(fields.into_iter().map(|(pos, len)| (pos, len, index)));
    }

    if removals.is_empty() {
        return Ok(0);
    }

    let mut positions: Vec<(usize, usize)> = removals.iter().map(|(p, l, _)| (*p, *l)).collect();
    positions.sort_unstable();
    positions.dedup();
    for (position, len) in positions.iter().rev() {
        sf.drain(*position..*position + *len);
    }

    for (index, object) in parsed.objects.iter().enumerate() {
        let shift: u64 = positions
            .iter()
            .filter(|(p, _)| (*p as u64) < object.byte_start)
            .map(|(_, len)| *len as u64)
            .sum();
        let removed: u32 = removals
            .iter()
            .filter(|(_, _, i)| *i == index)
            .map(|(_, len, _)| *len as u32)
            .sum();
        let new_start = object.byte_start - shift;
        let new_size = object.byte_size.saturating_sub(removed);

        let (start_pos, size_pos) = records[index];
        let relative = new_start - data_offset;
        if version >= 22 {
            let bytes = if big_endian {
                relative.to_be_bytes()
            } else {
                relative.to_le_bytes()
            };
            sf[start_pos..start_pos + 8].copy_from_slice(&bytes);
        } else {
            let bytes = if big_endian {
                (relative as u32).to_be_bytes()
            } else {
                (relative as u32).to_le_bytes()
            };
            sf[start_pos..start_pos + 4].copy_from_slice(&bytes);
        }
        let size_bytes = if big_endian {
            new_size.to_be_bytes()
        } else {
            new_size.to_le_bytes()
        };
        sf[size_pos..size_pos + 4].copy_from_slice(&size_bytes);
    }

    let new_len = sf.len() as u64;
    if version >= 22 {
        sf[24..32].copy_from_slice(&new_len.to_be_bytes());
    } else {
        sf[4..8].copy_from_slice(&(new_len as u32).to_be_bytes());
    }

    // The object table records themselves are part of the metadata, which
    // precedes the data section; the file-size field above tracks the whole file.
    Ok(removals.len())
}

/// Endfield-only fields that vanilla Unity (and therefore stock AssetStudio in
/// Normal mode) does not serialize.
fn should_skip(class_id: i32, raw_field: &str) -> bool {
    // Type trees may prefix a node name with its serialized type, e.g.
    // "(int&)m_layerOptMode"; match against the plain field name.
    let field = match raw_field.strip_prefix('(') {
        Some(rest) => match rest.find(')') {
            Some(i) => &rest[i + 1..],
            None => raw_field,
        },
        None => raw_field,
    };
    match class_id {
        142 => field == "m_PathFlags",
        28 => field == "m_TextureGroup",
        21 => matches!(field, "m_UVSetIndex" | "m_SubsurfaceProfile"),
        // The three collision flags plus their alignment pad form a 4-byte unit
        // that stock readers do not consume at all; without removing it every
        // following field (index format, index buffer, vertex data) is offset.
        43 => matches!(
            field,
            "m_CollisionMeshOnly"
                | "m_CollisionMeshBaked"
                | "m_CollisionMeshConvex"
                | "m_MeshMetrics[2]"
                | "m_BonesPerVertex"
        ),
        48 => matches!(
            field,
            "m_DescriptorSetParams"
                | "m_DifferentMaterialCbKeywordNames"
                | "m_EnableShaderLODStreaming"
                | "subShaderBlobs"
                | "m_CompressionType"
        ),
        // Renderer extras are absorbed by the stock reader's 4-byte alignment
        // window, so they must remain in place.
        23 => false,
        // AnimatorController (91).
        91 => matches!(
            field,
            // NOTE: m_MeshSpace and m_UseThreePoseBlender must stay. The stock
            // reader ends LayerConstant with AlignStream after
            // m_SyncedLayerAffectsTiming, so those two bytes are consumed as
            // padding; deleting them shifts every later layer by 2 bytes.
            "m_curveMaskIndex"
                | "m_DisableRootMotionPass"
                | "m_ConvertSimpleBlend"
                | "m_PostProcessLayer"
                | "m_layerOptMode"
                | "m_LODThreshold"
                | "m_AbilityThreshold"
                | "m_UseIdentifyValues"
                | "m_BlendStyle"
                // NOTE: m_EnableBlendRootMotion must stay. After
                // m_CanTransitionToSelf the stock reader calls AlignStream,
                // consuming this bool (and its pad) as alignment.
                | "m_SyncGroupID"
                | "m_SyncGroupRole"
                | "m_CullingMode"
                | "m_StateParameterConstantArray"
                | "m_TransitionBlockParamConstantArray"
                | "m_BlendSequenceData"
                | "m_StateNameHash"
                | "m_ChildPoseTimeEventIDArray"
                | "m_ImmediateTransition"
                | "m_InterruptDynamicTransitions"
        ),
        // NOTE: m_TransferCompressed is a single-byte flag sitting in the same
        // 4-byte alignment unit as m_Legacy/m_Compressed/m_UseHighQualityCurve;
        // the stock reader absorbs it as padding, so it must stay.
        74 => matches!(
            field,
            "m_aclType"
                | "m_AclCompressedBuffer"
                | "m_ClipTag"
                | "m_TransitionRotateMode"
                | "m_TransitionRotateDirType"
                | "m_TotalSize"
                | "m_TransitionRotateCurveIndex"
                | "m_ACLType"
                | "m_ACLArray"
                | "m_PositionFactor"
                | "m_EulerFactor"
                | "m_ScaleFactor"
                | "m_FloatFactor"
                | "m_nPositionCurves"
                | "m_nRotationCurves"
                | "m_nEulerCurves"
                | "m_nScaleCurves"
                | "m_CompressedCurveCount"
                | "hashCodeType"
        ),
        _ => false,
    }
}

struct TreeWalker<'a> {
    data: &'a [u8],
    pos: usize,
    big_endian: bool,
    removals: Vec<(usize, usize)>,
}

/// Walk one object using its type tree and collect byte ranges that belong to
/// Endfield-only fields. Returns the end offset of the consumed object; callers
/// reject the result unless it matches the object's recorded size, so a
/// mis-modelled tree can never corrupt an object.
fn walk_object(
    data: &[u8],
    start: usize,
    root: &TypeTreeNode,
    class_id: i32,
    big_endian: bool,
) -> Option<(usize, Vec<(usize, usize)>)> {
    let mut walker = TreeWalker {
        data,
        pos: start,
        big_endian,
        removals: Vec::new(),
    };
    walker.walk(root, class_id, false)?;
    Some((walker.pos, walker.removals))
}

impl TreeWalker<'_> {
    fn walk(&mut self, node: &TypeTreeNode, class_id: i32, skipping: bool) -> Option<()> {
        if !skipping && should_skip(class_id, &node.name) {
            let start = self.pos;
            self.walk(node, class_id, true)?;
            let len = self.pos - start;
            if len > 0 {
                self.removals.push((start, len));
            }
            return Some(());
        }

        if node.type_name == "Array" || node.type_name == "TypelessData" {
            let count = self.read_i32()?;
            if !(0..=100_000_000).contains(&count) {
                return None;
            }
            let element = node.children.get(1)?;
            if element.children.is_empty() && element.byte_size > 0 {
                // Fixed-size element (the common case): jump over the payload
                // instead of walking it element by element.
                let bytes = count as usize * element.byte_size as usize;
                if self.pos + bytes > self.data.len() {
                    return None;
                }
                self.pos += bytes;
            } else {
                for _ in 0..count {
                    self.walk(element, class_id, skipping)?;
                }
            }
        } else if !node.children.is_empty() {
            for child in &node.children {
                self.walk(child, class_id, skipping)?;
            }
        } else if node.byte_size > 0 {
            if self.pos + node.byte_size as usize > self.data.len() {
                return None;
            }
            self.pos += node.byte_size as usize;
        } else {
            // Strings/trees should have children; a negative size leaf
            // (`TypelessData`) cannot be walked safely.
            return None;
        }

        if node.meta_flag & 0x4000 != 0 {
            self.pos = (self.pos + 3) & !3;
        }
        Some(())
    }

    fn read_i32(&mut self) -> Option<i32> {
        let bytes: [u8; 4] = self.data.get(self.pos..self.pos + 4)?.try_into().ok()?;
        self.pos += 4;
        Some(if self.big_endian {
            i32::from_be_bytes(bytes)
        } else {
            i32::from_le_bytes(bytes)
        })
    }
}

/// Serialize a UnityFS bundle with LZ4HC-compressed storage blocks and block
/// info at the start of the file.
fn write_bundle<W: Write>(
    out: &mut W,
    segments: &[&[u8]],
    entries: &[DirectoryEntry],
    format_version: u32,
    player_version: &str,
    engine_version: &str,
    source_flags: u32,
) -> io::Result<(usize, usize, bool)> {
    let total: usize = segments.iter().map(|segment| segment.len()).sum();
    let blocks = compress_blocks(segments, total);

    let block_info_raw = build_block_info(&blocks, entries);
    let block_info = compress_lz4hc(&block_info_raw);
    // blocksInfo is padded to 16 bytes so the data area itself always starts on
    // a 16-byte boundary; ArchiveFlags.BlockInfoNeedPaddingAtStart (0x200) tells
    // readers to skip that padding.
    let block_info_padded_len = (block_info.len() + 15) & !15;
    // flags: compression 3 (LZ4HC) for the blocksInfo blob, 0x40 keeps
    // blocks + directory info combined and in front of the data.
    let flags = (source_flags & !0x3F & !0x80 & !0x200) | 0x40 | 0x03 | 0x200;

    let mut header_len =
        8 + 4 + player_version.len() + 1 + engine_version.len() + 1 + 8 + 4 + 4 + 4;
    if format_version >= 7 {
        header_len = (header_len + 15) & !15;
    }

    let data_size: usize = blocks.iter().map(|block| block.payload.len()).sum();
    let bundle_size = header_len + block_info_padded_len + data_size;

    let mut header = Vec::with_capacity(header_len);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&format_version.to_be_bytes());
    header.extend_from_slice(player_version.as_bytes());
    header.push(0);
    header.extend_from_slice(engine_version.as_bytes());
    header.push(0);
    header.extend_from_slice(&(bundle_size as i64).to_be_bytes());
    header.extend_from_slice(&(block_info.len() as u32).to_be_bytes());
    header.extend_from_slice(&(block_info_raw.len() as u32).to_be_bytes());
    header.extend_from_slice(&flags.to_be_bytes());
    header.resize(header_len, 0);

    out.write_all(&header)?;
    out.write_all(&block_info)?;
    out.write_all(&PADDING[..block_info_padded_len - block_info.len()])?;
    for block in &blocks {
        out.write_all(&block.payload)?;
    }

    let verified = blocks.iter().all(|block| block.verified);
    Ok((bundle_size, blocks.len(), verified))
}

/// A compressed storage block and the result of its LZ4 roundtrip check.
struct CompressedBlock {
    uncompressed: usize,
    payload: Vec<u8>,
    verified: bool,
}

/// Compress the stream block by block, spreading the (slow) LZ4HC work across
/// the available cores.
fn compress_blocks(segments: &[&[u8]], total: usize) -> Vec<CompressedBlock> {
    let count = if total == 0 {
        1
    } else {
        total.div_ceil(BLOCK_SIZE)
    };
    let workers = std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .min(count)
        .max(1);
    let chunk = count.div_ceil(workers);

    if workers == 1 {
        return (0..count)
            .map(|index| compress_block_at(segments, total, index))
            .collect();
    }

    let ranges: Vec<(usize, usize)> = (0..count)
        .step_by(chunk)
        .map(|start| (start, (start + chunk).min(count)))
        .collect();
    std::thread::scope(|scope| {
        let handles: Vec<_> = ranges
            .iter()
            .map(|&(start, end)| {
                scope.spawn(move || {
                    (start..end)
                        .map(|index| compress_block_at(segments, total, index))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("compression worker panicked"))
            .collect()
    })
}

fn compress_block_at(segments: &[&[u8]], total: usize, index: usize) -> CompressedBlock {
    let start = index * BLOCK_SIZE;
    let len = (total - start).min(BLOCK_SIZE);
    let source = gather_block(segments, start, len);
    let payload = compress_lz4hc(&source);
    let verified = if len == 0 {
        payload.is_empty()
    } else {
        match lz4_flex::decompress(&payload, len) {
            Ok(decoded) => decoded == source,
            Err(_) => false,
        }
    };
    CompressedBlock {
        uncompressed: len,
        payload,
        verified,
    }
}

/// Copy a logical block out of the (possibly multi-segment) stream.
fn gather_block(segments: &[&[u8]], start: usize, len: usize) -> Vec<u8> {
    let mut block = Vec::with_capacity(len);
    let mut base = 0;
    for segment in segments {
        let end = base + segment.len();
        if start < end && block.len() < len {
            let from = start.saturating_sub(base);
            let take = (len - block.len()).min(segment.len() - from);
            block.extend_from_slice(&segment[from..from + take]);
        }
        base = end;
        if block.len() == len {
            break;
        }
    }
    block
}

/// Compress a single raw LZ4 block with the high-compression encoder.
fn compress_lz4hc(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }
    lz4::block::compress(
        data,
        Some(lz4::block::CompressionMode::HIGHCOMPRESSION(9)),
        false,
    )
    .expect("LZ4HC compress")
}

fn build_block_info(blocks: &[CompressedBlock], entries: &[DirectoryEntry]) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&(blocks.len() as u32).to_be_bytes());
    for block in blocks {
        body.extend_from_slice(&(block.uncompressed as u32).to_be_bytes());
        body.extend_from_slice(&(block.payload.len() as u32).to_be_bytes());
        body.extend_from_slice(&3u16.to_be_bytes()); // compression: LZ4HC
    }
    body.extend_from_slice(&(entries.len() as u32).to_be_bytes());
    for entry in entries {
        body.extend_from_slice(&entry.offset.to_be_bytes());
        body.extend_from_slice(&entry.size.to_be_bytes());
        body.extend_from_slice(&entry.flags.to_be_bytes());
        body.extend_from_slice(entry.path.as_bytes());
        body.push(0);
    }

    // v7 blocksInfo starts with a 16-byte `uncompressedDataHash`; Unity writes
    // zeros here, so mirror that instead of computing a real hash.
    let mut info = Vec::with_capacity(16 + body.len());
    info.extend_from_slice(&[0u8; 16]);
    info.extend_from_slice(&body);
    info
}
