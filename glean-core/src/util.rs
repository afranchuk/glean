// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use chrono::{DateTime, FixedOffset, Local};

use crate::error_recording::{record_error, ErrorType};
use crate::metrics::TimeUnit;
use crate::CommonMetricData;
use crate::Glean;

/// Sanitizes the application id, generating a pipeline-friendly string that replaces
/// non alphanumeric characters with dashes.
pub fn sanitize_application_id(application_id: &str) -> String {
    let mut last_dash = false;
    application_id
        .chars()
        .filter_map(|x| match x {
            'A'..='Z' | 'a'..='z' | '0'..='9' => {
                last_dash = false;
                Some(x.to_ascii_lowercase())
            }
            _ => {
                let result = if last_dash { None } else { Some('-') };
                last_dash = true;
                result
            }
        })
        .collect()
}

/// Generate an ISO8601 compliant date/time string for the given time, truncating
/// it to the provided TimeUnit.
///
/// ## Arguments:
///
/// * `datetime`: the `DateTime` object that holds the date, time and timezone information.
/// * `truncate_to`: the desired resolution to use for the output string.
///
/// ## Return value:
///
/// Returns a string representing the provided date/time truncated to the requested time unit.
pub fn get_iso_time_string(datetime: DateTime<FixedOffset>, truncate_to: TimeUnit) -> String {
    datetime.format(truncate_to.format_pattern()).to_string()
}

/// Get the current date & time with a fixed-offset timezone.
///
/// This converts from the `Local` timezone into its fixed-offset equivalent.
pub(crate) fn local_now_with_offset() -> DateTime<FixedOffset> {
    let now: DateTime<Local> = Local::now();
    now.with_timezone(now.offset())
}

/// Truncates a string, ensuring that it doesn't end in the middle of a codepoint.
///
/// ## Arguments:
///
/// * `value` - The `String` to truncate.
/// * `length` - The length, in bytes, to truncate to.  The resulting string will
///   be at most this many bytes, but may be shorter to prevent ending in the middle
///   of a codepoint.
///
/// ## Return value:
///
/// Returns a string, with at most `length` bytes.
pub(crate) fn truncate_string_at_boundary<S: Into<String>>(value: S, length: usize) -> String {
    let s = value.into();
    if s.len() > length {
        for i in (0..=length).rev() {
            if s.is_char_boundary(i) {
                return s[0..i].to_string();
            }
        }
        // If we never saw a character boundary, the safest thing we can do is
        // return the empty string, though this should never happen in practice.
        return "".to_string();
    }
    s
}

/// Truncates a string, ensuring that it doesn't end in the middle of a codepoint.
/// If the string required truncation, records an error through the error
/// reporting mechanism.
///
/// ## Arguments:
///
/// * `glean` - The Glean instance the metric doing the truncation belongs to.
/// * `meta` - The metadata for the metric. Used for recording the error.
/// * `value` - The `String` to truncate.
/// * `length` - The length, in bytes, to truncate to.  The resulting string will
///   be at most this many bytes, but may be shorter to prevent ending in the middle
///   of a codepoint.
///
/// ## Return value:
///
/// Returns a string, with at most `length` bytes.
pub(crate) fn truncate_string_at_boundary_with_error<S: Into<String>>(
    glean: &Glean,
    meta: &CommonMetricData,
    value: S,
    length: usize,
) -> String {
    let s = value.into();
    if s.len() > length {
        let msg = format!("Value length {} exceeds maximum of {}", s.len(), length);
        record_error(glean, meta, ErrorType::InvalidValue, msg, None);
        truncate_string_at_boundary(s, length)
    } else {
        s
    }
}

// On i686 on Windows, the CPython interpreter sets the FPU precision control
// flag to 53 bits of precision, rather than the 64 bit default. On x86_64 on
// Windows, the CPython interpreter changes the rounding control settings. This
// causes different floating point results than on other architectures. This
// context manager makes it easy to set the correct precision and rounding control
// to match our other targets and platforms.
//
// See https://bugzilla.mozilla.org/show_bug.cgi?id=1623335 for additional context.
#[cfg(all(target_os = "windows", target_env = "gnu"))]
pub mod floating_point_context {
    // `size_t` is "pointer size", which is equivalent to Rust's `usize`.
    // It's defined as such in libc:
    // * https://github.com/rust-lang/libc/blob/bcbfeb5516cd5bb055198dbfbddf8d626fa2be07/src/unix/mod.rs#L19
    // * https://github.com/rust-lang/libc/blob/bcbfeb5516cd5bb055198dbfbddf8d626fa2be07/src/windows/mod.rs#L16
    #[allow(non_camel_case_types)]
    type size_t = usize;

    #[link(name = "m")]
    extern "C" {
        // Gets and sets the floating point control word.
        // See documentation here:
        // https://docs.microsoft.com/en-us/cpp/c-runtime-library/reference/controlfp-s
        fn _controlfp_s(current: *mut size_t, new: size_t, mask: size_t) -> size_t;
    }

    // Rounding control mask
    const MCW_RC: size_t = 0x00000300;
    // Round by truncation
    const RC_CHOP: size_t = 0x00000300;
    // Precision control mask
    const MCW_PC: size_t = 0x00030000;
    // Values for 64-bit precision
    const PC_64: size_t = 0x00000000;

    pub struct FloatingPointContext {
        original_value: size_t,
    }

    impl FloatingPointContext {
        pub fn new() -> Self {
            let mut current: size_t = 0;
            let _err = unsafe { _controlfp_s(&mut current, PC_64 | RC_CHOP, MCW_PC | MCW_RC) };

            FloatingPointContext {
                original_value: current,
            }
        }
    }

    impl Drop for FloatingPointContext {
        fn drop(&mut self) {
            let mut current: size_t = 0;
            let _err = unsafe { _controlfp_s(&mut current, self.original_value, MCW_PC | MCW_RC) };
        }
    }
}

#[cfg(not(all(target_os = "windows", target_env = "gnu")))]
pub mod floating_point_context {
    pub struct FloatingPointContext {}

    impl FloatingPointContext {
        pub fn new() -> Self {
            FloatingPointContext {}
        }
    }
}

/// The debug view tag is the value for the `X-Debug-Id` header of tagged ping requests,
/// thus is it must be a valid header value.
///
/// In other words, it must match the regex: "[a-zA-Z0-9-]{1,20}"
///
/// The regex crate isn't used here because it adds to the binary size,
/// and the Glean SDK doesn't use regular expressions anywhere else.
pub fn validate_debug_view_tag(value: &str) -> Option<String> {
    if value.is_empty() {
        log::error!("Debug view tag must have at least one character.");
        return None;
    }

    let mut iter = value.chars();
    let mut count = 0;

    loop {
        match iter.next() {
            // We are done, so the whole expression is valid.
            None => return Some(value.into()),
            // Valid characters.
            Some('-') | Some('a'..='z') | Some('A'..='Z') | Some('0'..='9') => (),
            // An invalid character
            Some(c) => {
                log::error!("Invalid character '{}' in debug view tag.", c);
                return None;
            }
        }
        count += 1;
        if count == 20 {
            log::error!("Debug view tag cannot exceed 20 characters");
            return None;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::offset::TimeZone;

    #[test]
    fn test_sanitize_application_id() {
        assert_eq!(
            "org-mozilla-test-app",
            sanitize_application_id("org.mozilla.test-app")
        );
        assert_eq!(
            "org-mozilla-test-app",
            sanitize_application_id("org.mozilla..test---app")
        );
        assert_eq!(
            "org-mozilla-test-app",
            sanitize_application_id("org-mozilla-test-app")
        );
        assert_eq!(
            "org-mozilla-test-app",
            sanitize_application_id("org.mozilla.Test.App")
        );
    }

    #[test]
    fn test_get_iso_time_string() {
        // `1985-07-03T12:09:14.000560274+01:00`
        let dt = FixedOffset::east(3600)
            .ymd(1985, 7, 3)
            .and_hms_nano(12, 9, 14, 1_560_274);
        assert_eq!(
            "1985-07-03T12:09:14.001560274+01:00",
            get_iso_time_string(dt, TimeUnit::Nanosecond)
        );
        assert_eq!(
            "1985-07-03T12:09:14.001560+01:00",
            get_iso_time_string(dt, TimeUnit::Microsecond)
        );
        assert_eq!(
            "1985-07-03T12:09:14.001+01:00",
            get_iso_time_string(dt, TimeUnit::Millisecond)
        );
        assert_eq!(
            "1985-07-03T12:09:14+01:00",
            get_iso_time_string(dt, TimeUnit::Second)
        );
        assert_eq!(
            "1985-07-03T12:09+01:00",
            get_iso_time_string(dt, TimeUnit::Minute)
        );
        assert_eq!(
            "1985-07-03T12+01:00",
            get_iso_time_string(dt, TimeUnit::Hour)
        );
        assert_eq!("1985-07-03+01:00", get_iso_time_string(dt, TimeUnit::Day));
    }

    #[test]
    fn local_now_gets_the_time() {
        let now = Local::now();
        let fixed_now = local_now_with_offset();

        // We can't compare across differing timezones, so we just compare the UTC timestamps.
        // The second timestamp should be just a few nanoseconds later.
        assert!(
            fixed_now.naive_utc() >= now.naive_utc(),
            "Time mismatch. Local now: {:?}, Fixed now: {:?}",
            now,
            fixed_now
        );
    }

    #[test]
    fn truncate_safely_test() {
        let value = "电脑坏了".to_string();
        let truncated = truncate_string_at_boundary(value, 10);
        assert_eq!("电脑坏", truncated);

        let value = "0123456789abcdef".to_string();
        let truncated = truncate_string_at_boundary(value, 10);
        assert_eq!("0123456789", truncated);
    }

    #[test]
    #[should_panic]
    fn truncate_naive() {
        // Ensure that truncating the naïve way on this string would panic
        let value = "电脑坏了".to_string();
        value[0..10].to_string();
    }

    #[test]
    fn validates_debug_view_tag_correctly() {
        assert!(validate_debug_view_tag("valid-value").is_some());
        assert!(validate_debug_view_tag("-also-valid-value").is_some());
        assert!(validate_debug_view_tag("invalid_value").is_none());
        assert!(validate_debug_view_tag("invalid value").is_none());
        assert!(validate_debug_view_tag("!nv@lid-val*e").is_none());
        assert!(validate_debug_view_tag("invalid-value-because-way-too-long").is_none());
        assert!(validate_debug_view_tag("").is_none());
    }
}