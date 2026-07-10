use crate::dram::Dram;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::Path;

const ELF_MAGIC: &[u8; 4] = b"\x7fELF";
const ELF64_HEADER_SIZE: usize = 64;
const ELF64_PROGRAM_HEADER_SIZE: usize = 56;
const ELFCLASS64: u8 = 2;
const ELFDATA2LSB: u8 = 1;
const EM_RISCV: u16 = 243;
const PT_LOAD: u32 = 1;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ImageFormat {
    Auto,
    Flat,
    Elf,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct LoadedImage {
    pub format: ImageFormat,
    pub entry: u64,
    pub loaded_bytes: u64,
}

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    InvalidImage(String),
}

impl Display for LoadError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => Display::fmt(error, f),
            Self::InvalidImage(message) => f.write_str(message),
        }
    }
}

impl Error for LoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::InvalidImage(_) => None,
        }
    }
}

impl From<std::io::Error> for LoadError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn load_image<P: AsRef<Path>>(
    dram: &mut Dram,
    path: P,
    format: ImageFormat,
) -> Result<LoadedImage, LoadError> {
    let bytes = fs::read(path)?;
    load_image_bytes(dram, &bytes, format)
}

pub fn load_image_bytes(
    dram: &mut Dram,
    bytes: &[u8],
    format: ImageFormat,
) -> Result<LoadedImage, LoadError> {
    let detected = match format {
        ImageFormat::Auto if bytes.starts_with(ELF_MAGIC) => ImageFormat::Elf,
        ImageFormat::Auto => ImageFormat::Flat,
        explicit => explicit,
    };

    match detected {
        ImageFormat::Flat => {
            dram.load_bytes(dram.base, bytes)?;
            Ok(LoadedImage {
                format: detected,
                entry: dram.base,
                loaded_bytes: bytes.len() as u64,
            })
        }
        ImageFormat::Elf => load_elf64(dram, bytes),
        ImageFormat::Auto => unreachable!(),
    }
}

fn load_elf64(dram: &mut Dram, bytes: &[u8]) -> Result<LoadedImage, LoadError> {
    if bytes.len() < ELF64_HEADER_SIZE || !bytes.starts_with(ELF_MAGIC) {
        return invalid("not an ELF image");
    }
    if bytes[4] != ELFCLASS64 {
        return invalid("only ELF64 images are supported");
    }
    if bytes[5] != ELFDATA2LSB {
        return invalid("only little-endian ELF images are supported");
    }
    if bytes[6] != 1 {
        return invalid("unsupported ELF identification version");
    }
    if read_u16(bytes, 18)? != EM_RISCV {
        return invalid("ELF machine is not RISC-V");
    }

    let entry = read_u64(bytes, 24)?;
    let phoff = to_usize(read_u64(bytes, 32)?, "program header offset")?;
    let phentsize = read_u16(bytes, 54)? as usize;
    let phnum = read_u16(bytes, 56)? as usize;
    if phnum == 0 {
        return invalid("ELF image has no program headers");
    }
    if phentsize < ELF64_PROGRAM_HEADER_SIZE {
        return invalid("ELF program header is too small");
    }

    let table_len = phentsize
        .checked_mul(phnum)
        .and_then(|len| phoff.checked_add(len))
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| LoadError::InvalidImage("ELF program header table is truncated".into()))?;
    let _ = table_len;

    let mut loaded_bytes = 0u64;
    let mut load_segments = 0usize;
    for index in 0..phnum {
        let header = phoff + index * phentsize;
        if read_u32(bytes, header)? != PT_LOAD {
            continue;
        }

        let file_offset = to_usize(read_u64(bytes, header + 8)?, "segment file offset")?;
        let virtual_address = read_u64(bytes, header + 16)?;
        let physical_address = read_u64(bytes, header + 24)?;
        let file_size = to_usize(read_u64(bytes, header + 32)?, "segment file size")?;
        let memory_size = to_usize(read_u64(bytes, header + 40)?, "segment memory size")?;
        if file_size > memory_size {
            return invalid("ELF segment file size exceeds memory size");
        }
        let file_end = file_offset
            .checked_add(file_size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| LoadError::InvalidImage("ELF load segment is truncated".into()))?;
        let address = if physical_address == 0 {
            virtual_address
        } else {
            physical_address
        };

        dram.load_bytes(address, &bytes[file_offset..file_end])?;
        if memory_size > file_size {
            let bss_address = address
                .checked_add(file_size as u64)
                .ok_or_else(|| LoadError::InvalidImage("ELF segment address overflow".into()))?;
            dram.zero_range(bss_address, memory_size - file_size)?;
        }
        loaded_bytes = loaded_bytes
            .checked_add(memory_size as u64)
            .ok_or_else(|| LoadError::InvalidImage("loaded byte count overflow".into()))?;
        load_segments += 1;
    }

    if load_segments == 0 {
        return invalid("ELF image has no loadable segments");
    }

    let dram_end = dram
        .end()
        .ok_or_else(|| LoadError::InvalidImage("DRAM address range overflows".into()))?;
    if !(dram.base..dram_end).contains(&entry) {
        return invalid("ELF entry point is outside DRAM");
    }

    Ok(LoadedImage {
        format: ImageFormat::Elf,
        entry,
        loaded_bytes,
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, LoadError> {
    Ok(u16::from_le_bytes(read_array(bytes, offset)?))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, LoadError> {
    Ok(u32::from_le_bytes(read_array(bytes, offset)?))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, LoadError> {
    Ok(u64::from_le_bytes(read_array(bytes, offset)?))
}

fn read_array<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N], LoadError> {
    let end = offset
        .checked_add(N)
        .ok_or_else(|| LoadError::InvalidImage("ELF structure offset overflows".into()))?;
    bytes
        .get(offset..end)
        .and_then(|slice| slice.try_into().ok())
        .ok_or_else(|| LoadError::InvalidImage("ELF structure is truncated".into()))
}

fn to_usize(value: u64, label: &str) -> Result<usize, LoadError> {
    usize::try_from(value)
        .map_err(|_| LoadError::InvalidImage(format!("{label} does not fit host address space")))
}

fn invalid<T>(message: &str) -> Result<T, LoadError> {
    Err(LoadError::InvalidImage(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elf_with_segment() -> Vec<u8> {
        let mut elf = vec![0u8; 0x104];
        elf[..4].copy_from_slice(ELF_MAGIC);
        elf[4] = ELFCLASS64;
        elf[5] = ELFDATA2LSB;
        elf[6] = 1;
        elf[16..18].copy_from_slice(&2u16.to_le_bytes());
        elf[18..20].copy_from_slice(&EM_RISCV.to_le_bytes());
        elf[20..24].copy_from_slice(&1u32.to_le_bytes());
        elf[24..32].copy_from_slice(&0x8000_0000u64.to_le_bytes());
        elf[32..40].copy_from_slice(&64u64.to_le_bytes());
        elf[52..54].copy_from_slice(&(ELF64_HEADER_SIZE as u16).to_le_bytes());
        elf[54..56].copy_from_slice(&(ELF64_PROGRAM_HEADER_SIZE as u16).to_le_bytes());
        elf[56..58].copy_from_slice(&1u16.to_le_bytes());

        let ph = 64;
        elf[ph..ph + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        elf[ph + 8..ph + 16].copy_from_slice(&0x100u64.to_le_bytes());
        elf[ph + 16..ph + 24].copy_from_slice(&0x8000_0000u64.to_le_bytes());
        elf[ph + 24..ph + 32].copy_from_slice(&0x8000_0000u64.to_le_bytes());
        elf[ph + 32..ph + 40].copy_from_slice(&4u64.to_le_bytes());
        elf[ph + 40..ph + 48].copy_from_slice(&8u64.to_le_bytes());
        elf[0x100..0x104].copy_from_slice(&[0x93, 0x0f, 0xa0, 0x02]);
        elf
    }

    #[test]
    fn auto_detects_and_loads_flat_images() {
        let mut dram = Dram::with_layout(0x8000_0000, 16);
        let loaded = load_image_bytes(&mut dram, &[1, 2, 3, 4], ImageFormat::Auto).unwrap();

        assert_eq!(loaded.format, ImageFormat::Flat);
        assert_eq!(loaded.entry, 0x8000_0000);
        assert_eq!(&dram.dram[..4], &[1, 2, 3, 4]);
    }

    #[test]
    fn loads_elf_segment_and_zeros_bss() {
        let mut dram = Dram::with_layout(0x8000_0000, 16);
        dram.dram.fill(0xff);
        let loaded = load_image_bytes(&mut dram, &elf_with_segment(), ImageFormat::Auto).unwrap();

        assert_eq!(loaded.format, ImageFormat::Elf);
        assert_eq!(loaded.entry, 0x8000_0000);
        assert_eq!(&dram.dram[..8], &[0x93, 0x0f, 0xa0, 0x02, 0, 0, 0, 0]);
    }

    #[test]
    fn rejects_non_riscv_elf_images() {
        let mut elf = elf_with_segment();
        elf[18..20].copy_from_slice(&62u16.to_le_bytes());
        let mut dram = Dram::with_layout(0x8000_0000, 16);

        assert!(matches!(
            load_image_bytes(&mut dram, &elf, ImageFormat::Elf),
            Err(LoadError::InvalidImage(_))
        ));
    }
}
