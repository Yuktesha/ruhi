//! # `http_client` — 如喜原生程序內 HTTP/HTTPS 客戶端與系統 Shell 輔助模組
//!
//! 在 Windows 平台直接透過作業系統原生 `winhttp.dll` (`WinHTTP` C API) 與 `shell32.dll` (`ShellExecuteW`)
//! 執行所有網路請求與網址／檔案開啟操作，100% 運行於程序記憶體空間內，
//! 徹底杜絕衍生外部 `curl.exe` 或 `cmd.exe /c start` 子程序，確保零主控台閃爍與零焦點干擾。

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub etag: String,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

/// 解析 URL 為 `(is_https, host, port, path_and_query)`
pub fn parse_url(url: &str) -> Option<(bool, String, u16, String)> {
    let trimmed = url.trim();
    let (is_https, default_port, rest) = if let Some(r) = trimmed.strip_prefix("https://") {
        (true, 443u16, r)
    } else if let Some(r) = trimmed.strip_prefix("http://") {
        (false, 80u16, r)
    } else {
        return None;
    };

    let (authority, path_and_query) = match rest.find('/') {
        Some(idx) => (&rest[..idx], format!("/{}", &rest[idx + 1..])),
        None => (rest, "/".to_string()),
    };

    if authority.is_empty() {
        return None;
    }

    let (host, port) = match authority.rfind(':') {
        Some(colon_idx) if !authority[colon_idx + 1..].contains(']') => {
            let h = &authority[..colon_idx];
            let p = authority[colon_idx + 1..]
                .parse::<u16>()
                .unwrap_or(default_port);
            (h.to_string(), p)
        }
        _ => (authority.to_string(), default_port),
    };

    if host.is_empty() {
        return None;
    }

    Some((is_https, host, port, path_and_query))
}

#[cfg(target_os = "windows")]
mod winhttp_imp {
    use super::{parse_url, HttpResponse};
    use std::ffi::c_void;
    use std::ptr;

    const WINHTTP_ACCESS_TYPE_DEFAULT_PROXY: u32 = 0;
    const WINHTTP_FLAG_SECURE: u32 = 0x00800000;
    const WINHTTP_ADDREQ_FLAG_ADD: u32 = 0x20000000;
    const WINHTTP_ADDREQ_FLAG_REPLACE: u32 = 0x80000000;
    const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
    const WINHTTP_QUERY_LOCATION: u32 = 33;
    const WINHTTP_QUERY_ETAG: u32 = 54;
    const WINHTTP_QUERY_FLAG_NUMBER: u32 = 0x20000000;
    const WINHTTP_OPTION_DISABLE_FEATURE: u32 = 63;
    const WINHTTP_DISABLE_REDIRECTS: u32 = 0x00000002;

    #[link(name = "winhttp")]
    extern "system" {
        fn WinHttpOpen(
            psz_agent_w: *const u16,
            dw_access_type: u32,
            psz_proxy_w: *const u16,
            psz_proxy_bypass_w: *const u16,
            dw_flags: u32,
        ) -> *mut c_void;

        fn WinHttpSetTimeouts(
            h_internet: *mut c_void,
            n_resolve_timeout: i32,
            n_connect_timeout: i32,
            n_send_timeout: i32,
            n_receive_timeout: i32,
        ) -> i32;

        fn WinHttpConnect(
            h_session: *mut c_void,
            pswz_server_name: *const u16,
            n_server_port: u16,
            dw_reserved: u32,
        ) -> *mut c_void;

        fn WinHttpOpenRequest(
            h_connect: *mut c_void,
            pwsz_verb: *const u16,
            pwsz_object_name: *const u16,
            pwsz_version: *const u16,
            pwsz_referrer: *const u16,
            ppwsz_accept_types: *const *const u16,
            dw_flags: u32,
        ) -> *mut c_void;

        fn WinHttpSetOption(
            h_internet: *mut c_void,
            dw_option: u32,
            lp_buffer: *const c_void,
            dw_buffer_length: u32,
        ) -> i32;

        fn WinHttpAddRequestHeaders(
            h_request: *mut c_void,
            lpsz_headers: *const u16,
            dw_headers_length: u32,
            dw_modifiers: u32,
        ) -> i32;

        fn WinHttpSendRequest(
            h_request: *mut c_void,
            lpsz_headers: *const u16,
            dw_headers_length: u32,
            lp_optional: *const c_void,
            dw_optional_length: u32,
            dw_total_length: u32,
            dw_context: usize,
        ) -> i32;

        fn WinHttpReceiveResponse(h_request: *mut c_void, lp_reserved: *mut c_void) -> i32;

        fn WinHttpQueryHeaders(
            h_request: *mut c_void,
            dw_info_level: u32,
            pwsz_name: *const u16,
            lp_buffer: *mut c_void,
            lpdw_buffer_length: *mut u32,
            lpdw_index: *mut u32,
        ) -> i32;

        fn WinHttpQueryDataAvailable(
            h_request: *mut c_void,
            lpdw_number_of_bytes_available: *mut u32,
        ) -> i32;

        fn WinHttpReadData(
            h_request: *mut c_void,
            lp_buffer: *mut c_void,
            dw_number_of_bytes_to_read: u32,
            lpdw_number_of_bytes_read: *mut u32,
        ) -> i32;

        fn WinHttpCloseHandle(h_internet: *mut c_void) -> i32;
    }

    struct WinHttpHandle(*mut c_void);

    impl WinHttpHandle {
        fn new(ptr: *mut c_void) -> Option<Self> {
            if ptr.is_null() {
                None
            } else {
                Some(Self(ptr))
            }
        }
        fn as_ptr(&self) -> *mut c_void {
            self.0
        }
    }

    impl Drop for WinHttpHandle {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    WinHttpCloseHandle(self.0);
                }
            }
        }
    }

    fn to_wide_null(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    unsafe fn query_header_string(h_req: *mut c_void, info_level: u32) -> Option<String> {
        let mut byte_len: u32 = 0;
        WinHttpQueryHeaders(
            h_req,
            info_level,
            ptr::null(),
            ptr::null_mut(),
            &mut byte_len,
            ptr::null_mut(),
        );
        if byte_len == 0 {
            return None;
        }
        let u16_len = (byte_len as usize / 2) + 1;
        let mut buf = vec![0u16; u16_len];
        let ok = WinHttpQueryHeaders(
            h_req,
            info_level,
            ptr::null(),
            buf.as_mut_ptr() as *mut c_void,
            &mut byte_len,
            ptr::null_mut(),
        );
        if ok == 0 {
            return None;
        }
        let char_count = (byte_len as usize / 2).min(buf.len());
        let slice = &buf[..char_count];
        let trimmed_slice = match slice.iter().position(|&c| c == 0) {
            Some(pos) => &slice[..pos],
            None => slice,
        };
        Some(String::from_utf16_lossy(trimmed_slice).trim().to_string())
    }

    pub fn execute_request(
        initial_method: &str,
        initial_url: &str,
        user_agent: &str,
        headers: &[(&str, &str)],
        initial_body: Option<&[u8]>,
        timeout_secs: u64,
    ) -> Result<HttpResponse, String> {
        let timeout_ms = (timeout_secs.clamp(2, 300) * 1000) as i32;
        let agent_w = to_wide_null(if user_agent.is_empty() {
            concat!("RuhI-Studio/", env!("CARGO_PKG_VERSION"))
        } else {
            user_agent
        });

        let session = unsafe {
            WinHttpHandle::new(WinHttpOpen(
                agent_w.as_ptr(),
                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                ptr::null(),
                ptr::null(),
                0,
            ))
        }
        .ok_or_else(|| "無法初始化 WinHTTP 工作階段 (WinHttpOpen failed)".to_string())?;

        unsafe {
            let _ = WinHttpSetTimeouts(
                session.as_ptr(),
                timeout_ms.min(10_000),
                timeout_ms.min(10_000),
                timeout_ms,
                timeout_ms,
            );
        }

        let mut current_method = initial_method.to_ascii_uppercase();
        let mut current_url = initial_url.trim().to_string();
        let mut include_body = true;

        for _hop in 0..6 {
            let (is_https, host, port, path_and_query) =
                parse_url(&current_url).ok_or_else(|| format!("無效的 URL 格式: {current_url}"))?;

            let host_w = to_wide_null(&host);
            let connect = unsafe {
                WinHttpHandle::new(WinHttpConnect(session.as_ptr(), host_w.as_ptr(), port, 0))
            }
            .ok_or_else(|| format!("無法連線至主機 {host}:{port}"))?;

            let verb_w = to_wide_null(&current_method);
            let path_w = to_wide_null(&path_and_query);
            let flags = if is_https { WINHTTP_FLAG_SECURE } else { 0 };

            let request = unsafe {
                WinHttpHandle::new(WinHttpOpenRequest(
                    connect.as_ptr(),
                    verb_w.as_ptr(),
                    path_w.as_ptr(),
                    ptr::null(),
                    ptr::null(),
                    ptr::null(),
                    flags,
                ))
            }
            .ok_or_else(|| "無法建立 WinHTTP 請求控制代碼".to_string())?;

            unsafe {
                // 停用內建自動重新導向，由本迴圈精確處理 HTTP 301/302/303 (含 Google Apps Script POST -> GET 轉換)
                let disable_flags: u32 = WINHTTP_DISABLE_REDIRECTS;
                let _ = WinHttpSetOption(
                    request.as_ptr(),
                    WINHTTP_OPTION_DISABLE_FEATURE,
                    &disable_flags as *const u32 as *const c_void,
                    std::mem::size_of::<u32>() as u32,
                );
            }

            for (k, v) in headers {
                if !include_body && k.eq_ignore_ascii_case("content-type") {
                    continue;
                }
                let hdr_line = format!("{k}: {v}\r\n");
                let hdr_w = to_wide_null(&hdr_line);
                unsafe {
                    let _ = WinHttpAddRequestHeaders(
                        request.as_ptr(),
                        hdr_w.as_ptr(),
                        u32::MAX,
                        WINHTTP_ADDREQ_FLAG_ADD | WINHTTP_ADDREQ_FLAG_REPLACE,
                    );
                }
            }

            let body_slice: &[u8] = if include_body {
                initial_body.unwrap_or(&[])
            } else {
                &[]
            };
            let body_ptr = if body_slice.is_empty() {
                ptr::null()
            } else {
                body_slice.as_ptr() as *const c_void
            };
            let body_len = body_slice.len() as u32;

            let send_ok = unsafe {
                WinHttpSendRequest(
                    request.as_ptr(),
                    ptr::null(),
                    0,
                    body_ptr,
                    body_len,
                    body_len,
                    0,
                )
            };
            if send_ok == 0 {
                return Err(format!("傳送 HTTP 請求至 {host}:{port} 失敗"));
            }

            let recv_ok = unsafe { WinHttpReceiveResponse(request.as_ptr(), ptr::null_mut()) };
            if recv_ok == 0 {
                return Err(format!("等待 {host}:{port} 回應逾時或中斷"));
            }

            let mut status_code: u32 = 0;
            let mut status_size = std::mem::size_of::<u32>() as u32;
            let status_ok = unsafe {
                WinHttpQueryHeaders(
                    request.as_ptr(),
                    WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                    ptr::null(),
                    &mut status_code as *mut u32 as *mut c_void,
                    &mut status_size,
                    ptr::null_mut(),
                )
            };
            if status_ok == 0 {
                return Err("無法讀取 HTTP 狀態碼".to_string());
            }

            if matches!(status_code, 301 | 302 | 303 | 307 | 308) {
                if let Some(loc) =
                    unsafe { query_header_string(request.as_ptr(), WINHTTP_QUERY_LOCATION) }
                {
                    if !loc.is_empty() {
                        current_url = if loc.starts_with("https://") || loc.starts_with("http://") {
                            loc
                        } else {
                            let scheme = if is_https { "https" } else { "http" };
                            let prefix = if loc.starts_with('/') { "" } else { "/" };
                            format!("{scheme}://{host}:{port}{prefix}{loc}")
                        };
                        if matches!(status_code, 301 | 302 | 303) {
                            current_method = "GET".to_string();
                            include_body = false;
                        }
                        continue;
                    }
                }
            }

            let etag = unsafe { query_header_string(request.as_ptr(), WINHTTP_QUERY_ETAG) }
                .unwrap_or_default();

            let mut resp_body = Vec::new();
            loop {
                let mut avail: u32 = 0;
                let ok_avail = unsafe { WinHttpQueryDataAvailable(request.as_ptr(), &mut avail) };
                if ok_avail == 0 || avail == 0 {
                    break;
                }
                let mut chunk = vec![0u8; avail as usize];
                let mut read_bytes: u32 = 0;
                let ok_read = unsafe {
                    WinHttpReadData(
                        request.as_ptr(),
                        chunk.as_mut_ptr() as *mut c_void,
                        avail,
                        &mut read_bytes,
                    )
                };
                if ok_read == 0 || read_bytes == 0 {
                    break;
                }
                resp_body.extend_from_slice(&chunk[..read_bytes as usize]);
            }

            return Ok(HttpResponse {
                status: status_code as u16,
                etag,
                body: resp_body,
            });
        }

        Err("HTTP 重新導向次數超過上限 (Too many redirects)".to_string())
    }
}

/// 執行 HTTP GET 請求（支援自訂 `User-Agent` 與 `If-None-Match` ETag 條件標頭）
pub fn http_get_with_headers(
    url: &str,
    user_agent: &str,
    headers: &[(&str, &str)],
    timeout_secs: u64,
) -> Result<HttpResponse, String> {
    #[cfg(target_os = "windows")]
    {
        winhttp_imp::execute_request("GET", url, user_agent, headers, None, timeout_secs)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (url, user_agent, headers, timeout_secs);
        Err("Non-Windows HTTP client not configured".to_string())
    }
}

/// 執行一般 HTTP GET 請求
pub fn http_get(url: &str, timeout_secs: u64) -> Result<HttpResponse, String> {
    http_get_with_headers(url, "", &[], timeout_secs)
}

/// 執行 HTTP POST JSON 請求（支援自訂標頭與 302 自動跟隨）
pub fn http_post_json(
    url: &str,
    payload: &serde_json::Value,
    extra_headers: &[(&str, &str)],
    timeout_secs: u64,
) -> Result<HttpResponse, String> {
    let body_bytes =
        serde_json::to_vec(payload).map_err(|e| format!("JSON 序列化失敗: {e}"))?;
    let mut headers = Vec::with_capacity(extra_headers.len() + 1);
    headers.push(("Content-Type", "application/json; charset=utf-8"));
    for &(k, v) in extra_headers {
        if !k.eq_ignore_ascii_case("content-type") {
            headers.push((k, v));
        }
    }
    #[cfg(target_os = "windows")]
    {
        winhttp_imp::execute_request(
            "POST",
            url,
            "",
            &headers,
            Some(&body_bytes),
            timeout_secs,
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (url, headers, body_bytes, timeout_secs);
        Err("Non-Windows HTTP client not configured".to_string())
    }
}

/// 以作業系統原生 Shell API (`ShellExecuteW`) 開啟網址、HTML 檔案或目錄（零 `cmd.exe` 子程序）
pub fn open_url_or_path(target: &str) -> bool {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return false;
    }
    #[cfg(target_os = "windows")]
    unsafe {
        #[link(name = "shell32")]
        extern "system" {
            fn ShellExecuteW(
                hwnd: *mut std::ffi::c_void,
                lp_operation: *const u16,
                lp_file: *const u16,
                lp_parameters: *const u16,
                lp_directory: *const u16,
                n_show_cmd: i32,
            ) -> isize;
        }
        const SW_SHOWNORMAL: i32 = 1;
        let op: Vec<u16> = "open\0".encode_utf16().collect();
        let file: Vec<u16> = trimmed.encode_utf16().chain(std::iter::once(0)).collect();
        let res = ShellExecuteW(
            std::ptr::null_mut(),
            op.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        );
        res > 32
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(trimmed)
            .spawn()
            .is_ok()
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(trimmed)
            .spawn()
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_url() {
        let (https, host, port, path) =
            parse_url("https://raw.githubusercontent.com/user/repo/main/feed.json").unwrap();
        assert!(https);
        assert_eq!(host, "raw.githubusercontent.com");
        assert_eq!(port, 443);
        assert_eq!(path, "/user/repo/main/feed.json");

        let (https2, host2, port2, path2) =
            parse_url("http://127.0.0.1:11434/api/chat").unwrap();
        assert!(!https2);
        assert_eq!(host2, "127.0.0.1");
        assert_eq!(port2, 11434);
        assert_eq!(path2, "/api/chat");
    }
}
