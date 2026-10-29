# BLC-VFS
**Remember: Delete this within 24 hours.**

A high-performance tool for mounting and extracting game resource packages from Arknights: Endfield (BLC format).
Mounting 260,000 files of 50GB only takes 1 second

## ⚠️ DISCLAIMER

**THIS SOFTWARE IS PROVIDED FOR EDUCATIONAL AND RESEARCH PURPOSES ONLY.**

This project is a tool designed for studying file formats and encryption methods. The author does not condone or encourage:

- Piracy or copyright infringement
- Unauthorized distribution of game assets
- Commercial use of extracted content
- Any illegal activities

**USE THIS SOFTWARE AT YOUR OWN RISK. THE AUTHOR IS NOT RESPONSIBLE FOR ANY MISUSE OR DAMAGES RESULTING FROM THE USE OF THIS TOOL.**

## 🗑️ DELETE NOTICE

**Please delete this repository within 24 hours of downloading/cloning.**

This repository contains tools and information that may be used to extract copyrighted material. By accessing this repository, you agree to:

1. Use this software solely for educational purposes
2. Not distribute any extracted copyrighted content
3. Delete all copies of this repository within 24 hours
4. Comply with all applicable laws and regulations in your jurisdiction

## 📋 License

This project is licensed under the **Attribution-NonCommercial-ShareAlike 4.0 International (CC BY-NC-SA 4.0)** License.

```
Copyright (c) 2026 blc-vfs contributors

Licensed under the Attribution-NonCommercial-ShareAlike 4.0 International License.
You may not use this file except in compliance with the License.
You may obtain a copy of the License at:
    https://creativecommons.org/licenses/by-nc-sa/4.0/

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```

## 🙏 Acknowledgments & Code References

This project incorporates code and concepts from the following sources:

### CC BY-NC-SA 4.0 Licensed Sources:

- **[BydTools](https://github.com/SherkeyXD/BydTools)** by SherkeyXD
  - BLC/VFS parsing and decryption implementation reference
  - Licensed under: **Attribution-NonCommercial-ShareAlike 4.0**

- **[Endfield CBT3 Reverse Engineering](https://blog.harryh.cn/Reverse-Engineering/Endfield-CBT3-Reverse-Engineering-1-VFS-Storage-Decryption/)** by Harryh
  - VFS storage and decryption analysis documentation
  - Licensed under: **Attribution-NonCommercial-ShareAlike 4.0**

- **[AnimeWwise](https://github.com/Escartem/AnimeWwise)** by Escartem
  - Wwise audio processing utilities
  - Licensed under: **Attribution-NonCommercial-ShareAlike 4.0**

### MIT Licensed Sources:

- **[AnimeStudio](https://github.com/Escartem/AnimeStudio)** by Escartem
  - Studio tools and utilities reference
  - Licensed under: **MIT License**

- **[VGMToolbox](https://github.com/Manicsteiner/VGMToolbox/)** by Manicsteiner
  - Criware USM video stream demuxing implementation
  - Licensed under: **MIT License**

### Unlicensed Sources:

- **[fluorite.moe](https://github.com/Eltik/fluorite.moe)** by Eltik
  - Endfield AB (UnityFS) decryption / VFS container extraction implementation reference
  - **Note:** This repository has no explicit license; code used with attribution

- **[FkArkEnd](https://github.com/shiikwi/FkArkEnd)** by shiikwi
  - Binary parsing reference
  - **Note:** This repository has no explicit license; code used with attribution

---

## 🚀 Features

### Core Functionality
- **BLC File Parsing**: Complete support for BLC virtual file system format
- **Decryption**: ChaCha20-based automatic decryption for encrypted files
- **AB Decryption (optional)**: rebuild Endfield-encrypted `.ab` payloads into standard UnityFS bundles readable by stock AssetStudio (CLI `extract-ab --decrypt`, GUI `Decrypt AB` checkbox). The rebuild drops Endfield-only fields, so some information is lost — for the complete bundle export the raw bytes and load them in AnimeStudio (game type `ArknightsEndfield`). See [fluorite.moe](https://github.com/Eltik/fluorite.moe) for the AB decryption reference.
- **Multiple File Types**: Support for BLC, PCK, and xLua formats
- **High Performance**: Memory-mapped I/O for efficient large file handling
- **Zero-Copy Operations**: Optimized data handling to minimize memory usage

### User Interfaces
- **CLI (Command Line Interface)**: Full-featured command-line tool
- **GUI (Graphical User Interface)**: Modern interface built with egui/eframe

### Supported Formats

#### Core Formats
| Format | Extension | Description | Status |
|--------|-----------|-------------|--------|
| BLC | `.blc` | Main virtual file system container | ✅ Working |
| CHK | `.chk` | Chunk integrity verification files | ✅ Working |

#### Audio Formats
| Format | Extension | Description | Status |
|--------|-----------|-------------|--------|
| PCK | `.pck` | Wwise audio package files | ✅ Working |
| WEM | `.wem` | Wwise encoded media (embedded in PCK) | ✅ Working |

#### Script Formats
| Format | Extension | Description | Status |
|--------|-----------|-------------|--------|
| xLua | `.lua` | Encrypted Lua scripts (XXTEA) | ✅ Working |
| Lua | `.lua` | Decrypted Lua source code | ✅ Working |

#### Video Formats
| Format | Extension | Description | Status |
|--------|-----------|-------------|--------|
| USM | `.usm` | Cri Stream Media (video extraction) | ✅ Working |
| M2V | `.m2v` | MPEG-2 video stream (extracted from USM) | ✅ Working |

#### Data/Config Formats
| Format | Extension | Description | Status |
|--------|-----------|-------------|--------|
| SparkBytes | `.bytes` | TableCfg package files | ✅ Working |
| JSON | `.json` | Exported configuration data | ✅ Working |
| PathBytes | `.bytes` | Path-based compressed data | ✅ Partial support |
| HGMMap | `.hgmmap` | Map data files | ✅ Working |

#### Binary Formats
| Format | Extension | Description | Status |
|--------|-----------|-------------|--------|
| CompressData | `.bin` | Compressed data archives | ✅ Working |

## 🏗️ Architecture

### Module Structure

```
src/
├── Binaries/
│   ├── cli.rs              # Command-line interface
│   ├── gui_main.rs         # GUI application entry point
│   └── gui/
│       ├── mod.rs          # GUI module root
│       ├── packages_window.rs  # Package browser window
│       ├── file_browser.rs     # File navigation panel
│       ├── extraction.rs       # Extraction logic UI
│       ├── pck_view.rs         # PCK viewer component
│       ├── search_panel.rs     # Search functionality
│       ├── progress_bar.rs     # Progress indicator
│       ├── status_bar.rs       # Status display
│       ├── header.rs           # Window header
│       └── types.rs            # GUI-specific types
├── VFS/
│   ├── mod.rs              # Virtual File System root
│   ├── Package/
│   │   ├── mod.rs          # Package management
│   │   └── multi_vfs.rs    # Multi-VFS support
│   ├── FileType/
│   │   ├── mod.rs          # File type dispatcher
│   │   ├── PCK/
│   │   │   ├── mod.rs      # PCK module root
│   │   │   ├── pck_parser.rs    # PCK file parser
│   │   │   ├── pck_decipher.rs  # PCK decryption
│   │   │   └── pck_extractor.rs # PCK extraction
│   │   └── xLua/
│   │       ├── mod.rs      # xLua module root
│   │       ├── lua_decipher.rs  # Lua decryption
│   │       └── xxtea.rs        # XXTEA implementation
│   ├── sparkbytes/
│   │   ├── mod.rs          # SparkBytes module root
│   │   ├── bytes_parser.rs # .bytes file parser (TableCfg)
│   │   └── types.rs        # SparkBytes type definitions
│   ├── pathbytes/
│   │   ├── mod.rs          # PathBytes module root
│   │   ├── parser.rs       # PathBytes parser
│   │   ├── compress_parser.rs  # Compressed data parser
│   │   ├── types.rs        # PathBytes type definitions
│   │   └── json.rs         # JSON export utilities
│   ├── hgmmap/
│   │   ├── mod.rs          # HGMMap module root
│   │   ├── hgmmap_parser.rs # HGMMap file parser
│   │   ├── types.rs        # HGMMap type definitions
│   │   └── json.rs         # JSON export utilities
│   └── usm/
│       ├── mod.rs          # USM module root
│       └── parser.rs       # USM video stream extractor
│   └── BLC/
│       ├── mod.rs          # BLC module root
│       ├── types.rs        # Data structures
│       ├── parser.rs       # Binary format parser
│       ├── crypto.rs       # Encryption/decryption
│       └── error.rs        # Error definitions
└── lib.rs                  # Library entry point
```

### Key Components

#### 1. **BLC Types & Parser** ([types.rs](src/VFS/BLC/types.rs), [parser.rs](src/VFS/BLC/parser.rs))
- `BlcMainInfo`: Main information block structure
- `ChunkInfo`: Chunk metadata
- `FileInfo`: Individual file information
- `UInt128`: 128-bit MD5 hash representation

#### 2. **Cryptographic Module** ([crypto.rs](src/VFS/BLC/crypto.rs))
- ChaCha20 stream cipher implementation
- Automatic BLC file decryption
- Per-file decryption support

#### 3. **Virtual File System** ([VFS/mod.rs](src/VFS/mod.rs))
- BLC file mounting and management
- CHK file mapping for integrity verification
- Unified file reading interface
- Transparent decryption layer

#### 4. **Error Handling** ([error.rs](src/VFS/BLC/error.rs))
- Comprehensive error type definitions
- Detailed error messages for debugging

## 🛠️ Installation & Usage

### Prerequisites

- **Rust Toolchain**: Install from [rustup.rs](https://rustup.rs/)
  - Minimum Rust Edition: 2021
  - Recommended: Latest stable release

### Building

```bash
# Clone the repository
git clone <repository-url>
cd blc-vfs

# Build release version
cargo build --release
```

The build process will produce two binaries:
- `blc-vfs-cli.exe` - Command-line interface
- `blc-vfs-gui.exe` - Graphical user interface

### CLI Usage

```bash
# Start interactive CLI
./target/release/blc-vfs-cli.exe

# Available commands:
mount <blc_file> [chk_folder]    # Mount a BLC file (optionally specify CHK folder)
list                               # List all mounted files
extract <file_path>                # Extract a single file
extract-all [output_dir]           # Extract all files to directory
info                               # Display VFS information
help                               # Show available commands
quit / exit                        # Exit the program
```

### GUI Usage

```bash
# Launch graphical interface
./target/release/blc-vfs-gui.exe
```

The GUI provides:
- 📁 Package browser with drag-and-drop support
- 🔍 Real-time file search functionality
- 📊 Progress tracking during extraction
- 🎨 Modern, responsive interface


## ✅ Verified Capabilities

- ✅ Successfully mount BLC files
- ✅ Parse complete file listings
- ✅ Extract individual and batch files
- ✅ Automatic decryption of encrypted content
- ✅ Support for multiple platforms (Windows, Android bundles)
- ✅ Memory-efficient large file handling
- ✅ **Partial .bytes file support**: TableCfg package (42A8FCA6) .bytes files → JSON
- ✅ ExtendData .bin files
- ❌ Streaming .bytes files (not yet supported)

## 🔬 Technical Highlights

### Performance Optimizations
1. **Memory Mapping (mmap)**: Zero-copy reads for large files using `memmap2`
2. **Parallel Processing**: Multi-threaded extraction via `rayon`
3. **Efficient Cryptography**: Stream cipher operations minimize memory overhead
4. **Type Safety**: Rust's ownership system ensures memory safety without garbage collection

### Comparison with Python Implementation

| Feature | Python Version | This Implementation |
|---------|---------------|-------------------|
| Performance | Slower | ~10-50x faster |
| Memory Usage | Higher | Significantly lower |
| Type Safety | Dynamic typing | Static typing |
| Error Handling | Exceptions | Result<T, E> pattern |
| Concurrency | GIL limited | Native multi-threading |
| Binary Size | Requires interpreter | Single executable |

## 📊 Dependencies

### Runtime Dependencies
- `chacha20` v0.9 - ChaCha20 stream cipher
- `base64` v0.22 - Base64 encoding/decoding
- `serde` v1.0 - Serialization framework
- `thiserror` v2.0 - Error derivation macros
- `memmap2` v0.9 - Memory-mapped file I/O
- `walkdir` v2.5 - Directory traversal
- `md-5` v0.10 - MD5 hashing
- `byteorder` v1.5 - Byte order utilities
- `serde_json` v1.0 - JSON serialization for .bytes parsing

### GUI Dependencies
- `eframe` v0.31 - egui framework integration
- `egui` v0.31 - Immediate mode GUI library
- `rfd` v0.15 - Native file dialogs
- `rayon` v1.10 - Parallel processing



## 🤝 Contributing

Contributions are welcome! Please follow these guidelines:

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

**Note:** All contributions must comply with the CC BY-NC-SA 4.0 license terms.

## 📄 Code of Conduct

- Respect copyright and intellectual property
- Use only for educational/research purposes
- Do not redistribute extracted copyrighted materials
- Credit original authors when referencing their work
- Comply with all applicable laws



---


