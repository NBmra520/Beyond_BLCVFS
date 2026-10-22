use crate::VFS::BLC::{BlcMainInfo, FileInfo, BlcParser, Decryptor, BlcError, Result};
use crate::VFS::FileType::{LuaDecipher, PckExtractor};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use memmap2::Mmap;

pub struct Package {
    pub name: String,
    pub main_info: BlcMainInfo,
    chunk_files: HashMap<String, PathBuf>,
    file_index: HashMap<String, FileInfo>,
    decryptor: Decryptor,
    #[allow(dead_code)]
    base_path: PathBuf,
}

impl Package {
    pub fn mount<P: AsRef<Path>>(package_folder: P) -> Result<Self> {
        let package_folder = package_folder.as_ref();
        let package_name = package_folder
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        
        let blc_files: Vec<PathBuf> = std::fs::read_dir(package_folder)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "blc"))
            .map(|entry| entry.path())
            .collect();
        
        if blc_files.is_empty() {
            return Err(BlcError::InvalidFormat(format!(
                "No BLC file found in package folder: {}",
                package_folder.display()
            )));
        }
        
        let blc_path = &blc_files[0];
        let blc_data = std::fs::read(blc_path)?;
        
        let decryptor = Decryptor::new()?;
        let decrypted_data = decryptor.decrypt_blc(&blc_data)?;
        
        let main_info = BlcParser::parse(&decrypted_data)?;
        
        let mut chunk_files = HashMap::new();
        for chunk in &main_info.all_chunks {
            let chunk_md5 = chunk.md5_name.to_hex().to_uppercase();
            let chk_file = package_folder.join(format!("{}.chk", chunk_md5));
            if chk_file.exists() {
                chunk_files.insert(chunk_md5, chk_file);
            }
        }
        
        let mut file_index = HashMap::new();
        for chunk in &main_info.all_chunks {
            for file in &chunk.files {
                file_index.insert(file.file_name.clone(), file.clone());
            }
        }
        
        Ok(Self {
            name: package_name,
            main_info,
            chunk_files,
            file_index,
            decryptor,
            base_path: package_folder.to_path_buf(),
        })
    }
    
    pub fn list_files(&self) -> Vec<&str> {
        self.file_index.keys().map(|s| s.as_str()).collect()
    }
    
    #[allow(dead_code)]
    pub fn file_exists(&self, path: &str) -> bool {
        self.file_index.contains_key(path)
    }
    
    #[allow(dead_code)]
    pub fn get_file_info(&self, path: &str) -> Option<&FileInfo> {
        self.file_index.get(path)
    }
    
    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let file_info = self.file_index
            .get(path)
            .ok_or_else(|| BlcError::FileNotFound(PathBuf::from(path)))?;
        
        let chunk_md5 = file_info.file_chunk_md5_name.to_hex().to_uppercase();
        let chk_path = self.chunk_files
            .get(&chunk_md5)
            .ok_or_else(|| BlcError::ChunkNotFound(chunk_md5.clone()))?;
        
        let file = std::fs::File::open(chk_path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        
        let offset = file_info.offset as usize;
        let length = file_info.len as usize;
        
        if offset + length > mmap.len() {
            return Err(BlcError::InvalidOffset {
                expected: offset + length,
                actual: mmap.len(),
            });
        }
        
        let data = &mmap[offset..offset + length];
        
        let mut result = if file_info.b_use_encrypt {
            let iv_seed = file_info.iv_seed
                .ok_or_else(|| BlcError::DecryptionFailed(
                    "File marked as encrypted but missing iv_seed".to_string()
                ))?;
            
            self.decryptor.decrypt_file(iv_seed, data)?
        } else {
            data.to_vec()
        };
        
        if path.to_lowercase().ends_with(".lua") {
            if let Some(decrypted) = LuaDecipher::decrypt(&result) {
                if LuaDecipher::is_valid_lua_bytecode(&decrypted) {
                    result = decrypted;
                }
            }
        }
        
        Ok(result)
    }
    
    #[allow(dead_code)]
    pub fn get_chunk_count(&self) -> usize {
        self.chunk_files.len()
    }
    
    pub fn get_file_count(&self) -> usize {
        self.file_index.len()
    }

    pub fn get_base_path(&self) -> &Path {
        &self.base_path
    }

    pub fn read_pck_file(&self, path: &str) -> Result<Vec<u8>> {
        let data = self.read_file(path)?;
        PckExtractor::get_decrypted_pck(&data)
    }

    pub fn list_pck_files(&self) -> Vec<&str> {
        self.file_index
            .keys()
            .filter(|k| k.to_lowercase().ends_with(".pck"))
            .map(|s| s.as_str())
            .collect()
    }

    pub fn extract_pck_to_dir(&self, path: &str, output_dir: &Path) -> Result<Vec<PathBuf>> {
        let data = self.read_file(path)?;
        let file_name = Path::new(path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("output.pck");
        PckExtractor::extract_to_dir(&data, output_dir, Some(file_name))
    }
}

pub mod multi_vfs;
pub use multi_vfs::MultiVFS;
