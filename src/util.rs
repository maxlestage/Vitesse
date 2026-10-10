//! Small internal utilities: URL decoding, MIME types and HTTP dates.

use std::borrow::Cow;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[inline]
fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decodes the `%XX` sequences of a URL segment (`+` signs are left as is).
///
/// Allocates nothing when there is nothing to decode.
pub(crate) fn percent_decode(input: &str) -> Cow<'_, str> {
    let bytes = input.as_bytes();
    let Some(first) = bytes.iter().position(|&b| b == b'%') else {
        return Cow::Borrowed(input);
    };
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&bytes[..first]);
    let mut i = first;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    match String::from_utf8(out) {
        Ok(s) => Cow::Owned(s),
        Err(e) => Cow::Owned(String::from_utf8_lossy(e.as_bytes()).into_owned()),
    }
}

/// Guesses the MIME type of a file from its extension.
pub(crate) fn mime_for(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" | "cjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "txt" | "log" => "text/plain; charset=utf-8",
        "md" => "text/markdown; charset=utf-8",
        "csv" => "text/csv; charset=utf-8",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        "wasm" => "application/wasm",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "webmanifest" => "application/manifest+json",
        _ => "application/octet-stream",
    }
}

/// Formats a date in the HTTP format (RFC 9110), e.g.
/// `Sun, 06 Nov 1994 08:49:37 GMT`.
pub(crate) fn http_date(time: SystemTime) -> String {
    const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let secs = time
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        DAYS[((days + 4) % 7) as usize],
        day,
        MONTHS[(month - 1) as usize],
        year,
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Converts a number of days since 1970-01-01 into (year, month, day).
/// Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe as i64 + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn decode() {
        assert_eq!(percent_decode("abc"), "abc");
        assert!(matches!(percent_decode("abc"), Cow::Borrowed(_)));
        assert_eq!(percent_decode("caf%C3%A9"), "café");
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%4"), "%4");
        assert_eq!(percent_decode("%41"), "A");
    }

    #[test]
    fn dates() {
        assert_eq!(http_date(UNIX_EPOCH), "Thu, 01 Jan 1970 00:00:00 GMT");
        let t = UNIX_EPOCH + Duration::from_secs(784_111_777);
        assert_eq!(http_date(t), "Sun, 06 Nov 1994 08:49:37 GMT");
        let t = UNIX_EPOCH + Duration::from_secs(1_709_164_800); // 29 février 2024
        assert_eq!(http_date(t), "Thu, 29 Feb 2024 00:00:00 GMT");
    }

    #[test]
    fn mimes() {
        assert_eq!(mime_for(Path::new("a/b.HTML")), "text/html; charset=utf-8");
        assert_eq!(mime_for(Path::new("x.wasm")), "application/wasm");
        assert_eq!(mime_for(Path::new("noext")), "application/octet-stream");
    }
}
