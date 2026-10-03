//! Just enough of the PE format to find an import slot in a loaded module.
//!
//! Works on the image as mapped in memory (offsets are RVAs). PES6.exe calls
//! its imports through `jmp [slot]` stubs, so replacing the address in the
//! slot redirects every call.

/// Read access to a mapped image, by RVA.
pub trait Image {
    fn bytes(&self, rva: usize, len: usize) -> Option<&[u8]>;

    fn u16_at(&self, rva: usize) -> Option<u16> {
        self.bytes(rva, 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32_at(&self, rva: usize) -> Option<u32> {
        self.bytes(rva, 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    /// Null-terminated ASCII string, at most 256 bytes.
    fn c_str(&self, rva: usize) -> Option<&[u8]> {
        let mut len = 0;
        while len < 256 {
            if *self.bytes(rva + len, 1)?.first()? == 0 {
                return self.bytes(rva, len);
            }
            len += 1;
        }
        None
    }
}

impl Image for [u8] {
    fn bytes(&self, rva: usize, len: usize) -> Option<&[u8]> {
        self.get(rva..rva.checked_add(len)?)
    }
}

const PE32_MAGIC: u16 = 0x10b;
const IMPORT_DIRECTORY: usize = 1;
const ORDINAL_FLAG: u32 = 0x8000_0000;

/// RVA of the import address table slot of `function` imported from `dll`
/// (case-insensitive), in a 32-bit image. Imports by ordinal are skipped.
pub fn import_slot(image: &(impl Image + ?Sized), dll: &str, function: &str) -> Option<usize> {
    if image.bytes(0, 2)? != b"MZ" {
        return None;
    }
    let pe = image.u32_at(0x3c)? as usize;
    if image.bytes(pe, 4)? != b"PE\0\0" {
        return None;
    }
    let optional = pe + 24;
    if image.u16_at(optional)? != PE32_MAGIC {
        return None;
    }
    let imports = image.u32_at(optional + 96 + 8 * IMPORT_DIRECTORY)? as usize;
    if imports == 0 {
        return None;
    }

    for descriptor in (imports..).step_by(20) {
        let names = image.u32_at(descriptor)? as usize;
        let dll_name = image.u32_at(descriptor + 12)? as usize;
        let slots = image.u32_at(descriptor + 16)? as usize;
        if dll_name == 0 {
            return None;
        }
        if !image.c_str(dll_name)?.eq_ignore_ascii_case(dll.as_bytes()) {
            continue;
        }
        // Once loaded, the slots hold addresses: the names come from the
        // separate lookup table, which PES6.exe has for every DLL.
        if names == 0 {
            return None;
        }
        for i in 0.. {
            let entry = image.u32_at(names + 4 * i)?;
            if entry == 0 {
                break;
            }
            if entry & ORDINAL_FLAG != 0 {
                continue;
            }
            // Hint (2 bytes), then the name.
            if image.c_str(entry as usize + 2)? == function.as_bytes() {
                return Some(slots + 4 * i);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny mapped PE32 image importing two functions from TEST.dll.
    fn image() -> Vec<u8> {
        let mut img = vec![0u8; 0x400];
        img[0..2].copy_from_slice(b"MZ");
        img[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        img[0x80..0x84].copy_from_slice(b"PE\0\0");
        let optional = 0x80 + 24;
        img[optional..optional + 2].copy_from_slice(&PE32_MAGIC.to_le_bytes());
        let dir = optional + 96 + 8;
        img[dir..dir + 4].copy_from_slice(&0x200u32.to_le_bytes());

        // One descriptor at 0x200, then a null one.
        let put = |img: &mut Vec<u8>, at: usize, v: u32| {
            img[at..at + 4].copy_from_slice(&v.to_le_bytes())
        };
        put(&mut img, 0x200, 0x280); // names
        put(&mut img, 0x200 + 12, 0x300); // dll name
        put(&mut img, 0x200 + 16, 0x2c0); // slots
        // Name table: ordinal import, then two by name.
        put(&mut img, 0x280, ORDINAL_FLAG | 7);
        put(&mut img, 0x284, 0x320);
        put(&mut img, 0x288, 0x340);
        img[0x300..0x309].copy_from_slice(b"TEST.dll\0");
        img[0x322..0x32a].copy_from_slice(b"FirstFn\0");
        img[0x342..0x34b].copy_from_slice(b"SecondFn\0");
        img
    }

    #[test]
    fn finds_the_slot_of_a_named_import() {
        let img = image();
        assert_eq!(
            import_slot(img.as_slice(), "test.DLL", "FirstFn"),
            Some(0x2c4)
        );
        assert_eq!(
            import_slot(img.as_slice(), "TEST.dll", "SecondFn"),
            Some(0x2c8)
        );
    }

    #[test]
    fn missing_imports_are_none() {
        let img = image();
        assert_eq!(import_slot(img.as_slice(), "TEST.dll", "Nope"), None);
        assert_eq!(import_slot(img.as_slice(), "OTHER.dll", "FirstFn"), None);
    }

    #[test]
    fn rejects_non_pe_data() {
        assert_eq!(
            import_slot([0u8; 64].as_slice(), "TEST.dll", "FirstFn"),
            None
        );
        let mut img = image();
        img[0x80] = b'X';
        assert_eq!(import_slot(img.as_slice(), "TEST.dll", "FirstFn"), None);
    }
}
