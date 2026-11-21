// SPDX-License-Identifier: Apache-2.0
// ruhi-core/src/mmap_io.rs — 跨模組共用之零依賴唯讀記憶體映射緩衝區與安全位元組讀取輔助函式

use std::fs::File;
use std::io;
use std::path::Path;

/// 作業系統原生唯讀記憶體映射緩衝區（Windows `CreateFileMappingW` / `MapViewOfFile`；非 Windows 採用唯讀緩衝區）
pub struct ReadOnlyMmap {
    ptr: *const u8,
    len: usize,
    #[cfg(target_os = "windows")]
    mapping_handle: isize,
    #[cfg(not(target_os = "windows"))]
    _owned_fallback: Vec<u8>,
}

unsafe impl Send for ReadOnlyMmap {}
unsafe impl Sync for ReadOnlyMmap {}

#[cfg(target_os = "windows")]
mod win_mmap {
    use std::fs::File;
    use std::io;
    use std::os::windows::io::AsRawHandle;

    const PAGE_READONLY: u32 = 0x02;
    const FILE_MAP_READ: u32 = 0x0004;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateFileMappingW(
            hFile: isize,
            lpFileMappingAttributes: *const core::ffi::c_void,
            flProtect: u32,
            dwMaximumSizeHigh: u32,
            dwMaximumSizeLow: u32,
            lpName: *const u16,
        ) -> isize;
        fn MapViewOfFile(
            hFileMappingObject: isize,
            dwDesiredAccess: u32,
            dwFileOffsetHigh: u32,
            dwFileOffsetLow: u32,
            dwNumberOfBytesToMap: usize,
        ) -> *const u8;
        fn UnmapViewOfFile(lpBaseAddress: *const u8) -> i32;
        fn CloseHandle(hObject: isize) -> i32;
    }

    pub fn map_file(file: &File, len: usize) -> io::Result<(*const u8, isize)> {
        if len == 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "empty file"));
        }
        unsafe {
            let h_file = file.as_raw_handle() as isize;
            let h_map = CreateFileMappingW(
                h_file,
                core::ptr::null(),
                PAGE_READONLY,
                0,
                0,
                core::ptr::null(),
            );
            if h_map == 0 {
                return Err(io::Error::last_os_error());
            }
            let ptr = MapViewOfFile(h_map, FILE_MAP_READ, 0, 0, len);
            if ptr.is_null() {
                let err = io::Error::last_os_error();
                CloseHandle(h_map);
                return Err(err);
            }
            Ok((ptr, h_map))
        }
    }

    pub fn unmap(ptr: *const u8, h_map: isize) {
        unsafe {
            if !ptr.is_null() {
                UnmapViewOfFile(ptr);
            }
            if h_map != 0 {
                CloseHandle(h_map);
            }
        }
    }
}

impl ReadOnlyMmap {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let len_u64 = file.metadata()?.len();
        let len = usize::try_from(len_u64).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "mapped file exceeds address space")
        })?;
        #[cfg(target_os = "windows")]
        {
            let (ptr, mapping_handle) = win_mmap::map_file(&file, len)?;
            Ok(Self {
                ptr,
                len,
                mapping_handle,
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            drop(file);
            if len == 0 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "empty file"));
            }
            let bytes = std::fs::read(path)?;
            let ptr = bytes.as_ptr();
            let len = bytes.len();
            Ok(Self {
                ptr,
                len,
                _owned_fallback: bytes,
            })
        }
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Drop for ReadOnlyMmap {
    fn drop(&mut self) {
        #[cfg(target_os = "windows")]
        {
            win_mmap::unmap(self.ptr, self.mapping_handle);
        }
    }
}

/// 邊界檢查切片讀取輔助函式
#[inline]
pub fn slice_checked(buf: &[u8], off: usize, len: usize) -> Option<&[u8]> {
    let end = off.checked_add(len)?;
    buf.get(off..end)
}

/// 安全讀取小端序 `u16`
#[inline]
pub fn read_u16_le(buf: &[u8], off: usize) -> Option<u16> {
    let s = slice_checked(buf, off, 2)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

/// 安全讀取小端序 `u32`
#[inline]
pub fn read_u32_le(buf: &[u8], off: usize) -> Option<u32> {
    let s = slice_checked(buf, off, 4)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// 安全讀取小端序 `u64`
#[inline]
pub fn read_u64_le(buf: &[u8], off: usize) -> Option<u64> {
    let s = slice_checked(buf, off, 8)?;
    Some(u64::from_le_bytes([
        s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7],
    ]))
}

/// 安全讀取小端序 IEEE-754 `f32`
#[inline]
pub fn read_f32_le(buf: &[u8], off: usize) -> Option<f32> {
    let s = slice_checked(buf, off, 4)?;
    Some(f32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_only_mmap_and_le_helpers() {
        let tmp_dir = std::env::temp_dir().join(format!("ruhi_mmap_io_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp_dir);
        std::fs::create_dir_all(&tmp_dir).unwrap();
        let file_path = tmp_dir.join("probe.bin");

        let mut data = Vec::new();
        data.extend_from_slice(&0x1234u16.to_le_bytes());
        data.extend_from_slice(&0x89AB_CDEFu32.to_le_bytes());
        data.extend_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
        data.extend_from_slice(&(-3.5f32).to_le_bytes());
        std::fs::write(&file_path, &data).unwrap();

        let mmap = ReadOnlyMmap::open(&file_path).expect("ReadOnlyMmap::open should succeed");
        assert_eq!(mmap.len(), data.len());
        assert!(!mmap.is_empty());
        let buf = mmap.as_slice();
        assert_eq!(read_u16_le(buf, 0), Some(0x1234));
        assert_eq!(read_u32_le(buf, 2), Some(0x89AB_CDEF));
        assert_eq!(read_u64_le(buf, 6), Some(0x0102_0304_0506_0708));
        assert_eq!(read_f32_le(buf, 14), Some(-3.5f32));

        // 邊界檢查與溢位保護
        assert_eq!(slice_checked(buf, 14, 4).map(|s| s.len()), Some(4));
        assert_eq!(slice_checked(buf, 15, 4), None);
        assert_eq!(slice_checked(buf, usize::MAX - 1, 4), None);
        assert_eq!(read_u32_le(buf, buf.len() - 3), None);

        // 解除映射後可安全刪除暫存檔
        drop(mmap);
        std::fs::remove_file(&file_path).expect("unmapped file should be removable");
        let _ = std::fs::remove_dir_all(&tmp_dir);
    }
}
