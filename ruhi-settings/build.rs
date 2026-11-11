use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn write_res_header(
    out: &mut Vec<u8>,
    data_size: u32,
    type_id: u16,
    name_id: u16,
    mem_flags: u16,
    lang_id: u16,
) {
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend_from_slice(&32u32.to_le_bytes()); // HeaderSize = 32 bytes
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&type_id.to_le_bytes());
    out.extend_from_slice(&0xFFFFu16.to_le_bytes());
    out.extend_from_slice(&name_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&mem_flags.to_le_bytes());
    out.extend_from_slice(&lang_id.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
}

fn pad4(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

fn compile_ico_to_res(ico_bytes: &[u8]) -> Vec<u8> {
    let count = u16::from_le_bytes([ico_bytes[4], ico_bytes[5]]) as usize;
    let mut res = Vec::new();

    // 1. Null 32-byte resource header
    write_res_header(&mut res, 0, 0, 0, 0, 0);

    // 2. RT_ICON (3) records for each image entry
    let mut grp_data = Vec::with_capacity(6 + count * 14);
    grp_data.extend_from_slice(&ico_bytes[0..6]); // Reserved (0), Type (1), Count

    for i in 0..count {
        let entry_offset = 6 + i * 16;
        let entry = &ico_bytes[entry_offset..entry_offset + 16];
        let bytes_in_res = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let image_offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        let img_data = &ico_bytes[image_offset..image_offset + bytes_in_res];
        let icon_id = (i + 1) as u16;

        write_res_header(&mut res, bytes_in_res as u32, 3, icon_id, 0x1010, 0x0409);
        res.extend_from_slice(img_data);
        pad4(&mut res);

        // GRPICONDIRENTRY: first 12 bytes of ICONDIRENTRY + u16 icon_id
        grp_data.extend_from_slice(&entry[0..12]);
        grp_data.extend_from_slice(&icon_id.to_le_bytes());
    }

    // 3. RT_GROUP_ICON (14) record with ID = 1
    write_res_header(&mut res, grp_data.len() as u32, 14, 1, 0x1030, 0x0409);
    res.extend_from_slice(&grp_data);
    pad4(&mut res);

    res
}

fn main() {
    println!("cargo:rerun-if-changed=../assets/ruhi.ico");
    if env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        let ico_path = manifest_dir.join("..").join("assets").join("ruhi.ico");
        if let Ok(ico_bytes) = fs::read(&ico_path) {
            let res_bytes = compile_ico_to_res(&ico_bytes);
            let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
            let res_path = out_dir.join("ruhi_icon.res");
            fs::write(&res_path, res_bytes).expect("Failed to write ruhi_icon.res");

            let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
            if target_env == "gnu" {
                let obj_path = out_dir.join("ruhi_icon.o");
                let status = Command::new("windres")
                    .args(["-i", &res_path.to_string_lossy(), "-o", &obj_path.to_string_lossy()])
                    .status();
                if matches!(status, Ok(s) if s.success()) {
                    println!("cargo:rustc-link-arg-bins={}", obj_path.display());
                }
            } else {
                println!("cargo:rustc-link-arg-bins={}", res_path.display());
            }
        }
    }
}
