use std::ffi::c_void;
use std::ptr::null_mut;

use windows::Win32::Networking::WinHttp::{
    INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_ACCESS_TYPE_NAMED_PROXY,
    WINHTTP_CURRENT_USER_IE_PROXY_CONFIG, WINHTTP_FLAG_SECURE, WinHttpGetIEProxyConfigForCurrentUser,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect, WinHttpOpen,
    WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse, WinHttpSendRequest,
    WinHttpSetTimeouts,
};
use windows::core::{PCWSTR, w};

pub enum Fetched {
    Body(String),
    NotFound,
}

/// Asks GitHub for the latest release of this project. Returns `None` on any failure.
pub fn fetch_latest_release() -> Option<Fetched> {
    unsafe {
        // WinHTTP ignores the proxy set in Windows "Internet options" unless it is passed explicitly.
        let mut ie = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
        let named = WinHttpGetIEProxyConfigForCurrentUser(&mut ie).is_ok() && !ie.lpszProxy.is_null();
        let session = if named {
            WinHttpOpen(
                w!("ow-cache-cleaner"),
                WINHTTP_ACCESS_TYPE_NAMED_PROXY,
                PCWSTR(ie.lpszProxy.0),
                PCWSTR(ie.lpszProxyBypass.0),
                0,
            )
        } else {
            WinHttpOpen(
                w!("ow-cache-cleaner"),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                PCWSTR::null(),
                PCWSTR::null(),
                0,
            )
        };
        if session.is_null() {
            return None;
        }
        let _ = WinHttpSetTimeouts(session, 5000, 5000, 5000, 10000);
        let result = request(session);
        let _ = WinHttpCloseHandle(session);
        result
    }
}

unsafe fn request(session: *mut c_void) -> Option<Fetched> {
    unsafe {
        let connect = WinHttpConnect(session, w!("api.github.com"), INTERNET_DEFAULT_HTTPS_PORT, 0);
        if connect.is_null() {
            return None;
        }
        let result = get(connect);
        let _ = WinHttpCloseHandle(connect);
        result
    }
}

unsafe fn get(connect: *mut c_void) -> Option<Fetched> {
    unsafe {
        let request = WinHttpOpenRequest(
            connect,
            w!("GET"),
            w!("/repos/Teakowa/ow-cache-cleaner/releases/latest"),
            PCWSTR::null(),
            PCWSTR::null(),
            null_mut(),
            WINHTTP_FLAG_SECURE,
        );
        if request.is_null() {
            return None;
        }
        let result = read_response(request);
        let _ = WinHttpCloseHandle(request);
        result
    }
}

unsafe fn read_response(request: *mut c_void) -> Option<Fetched> {
    unsafe {
        WinHttpSendRequest(request, None, None, 0, 0, 0).ok()?;
        WinHttpReceiveResponse(request, null_mut()).ok()?;

        let mut status = 0u32;
        let mut size = size_of::<u32>() as u32;
        WinHttpQueryHeaders(
            request,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut c_void),
            &mut size,
            null_mut(),
        )
        .ok()?;
        match status {
            200 => {}
            404 => return Some(Fetched::NotFound),
            _ => return None,
        }

        let mut body = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let mut read = 0u32;
            WinHttpReadData(request, buffer.as_mut_ptr() as *mut c_void, buffer.len() as u32, &mut read).ok()?;
            if read == 0 {
                break;
            }
            body.extend_from_slice(&buffer[..read as usize]);
        }
        Some(Fetched::Body(String::from_utf8_lossy(&body).into_owned()))
    }
}
