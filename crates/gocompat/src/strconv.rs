// SPDX-License-Identifier: BSD-3-Clause
// Copyright 2009 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSES file.
//
// Ported from Go 1.27.2 (https://go.dev/src):
//   - src/internal/strconv/ftoa.go
//   - src/fmt/format.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Subset of Go `strconv` / `fmt` number formatting.

/// Shortest decimal digits and decimal-point position of a finite non-zero |f|:
/// value = 0.d1d2d3... × 10^dp
fn shortest_digits(f: f64) -> (String, i64) {
    let s = format!("{:e}", f.abs()); // e.g. "1.2345e-7", shortest round-trip digits
    let (mant, exp) = s.split_once('e').unwrap();
    let exp: i64 = exp.parse().unwrap();
    let digits: String = mant.chars().filter(|c| *c != '.').collect();
    (digits, exp + 1)
}

/// `strconv.FormatFloat(f, fmt, -1, 64)` for fmt in 'e', 'f', 'g' (shortest precision).
pub fn FormatFloatShortest(f: f64, fmt: u8) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "+Inf".into() } else { "-Inf".into() };
    }
    let neg = f.is_sign_negative();
    let (digits, dp) = if f == 0.0 { ("0".to_string(), 1) } else { shortest_digits(f) };
    let nd = digits.len() as i64;
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    let use_e = match fmt {
        b'e' => true,
        b'f' => false,
        _ => {
            // shortest: eprec = 6
            let exp = dp - 1;
            exp < -4 || exp >= 6
        }
    };
    if use_e {
        out.push_str(&digits[..1]);
        if nd > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        let exp = if f == 0.0 { 0 } else { dp - 1 };
        out.push('e');
        if exp < 0 {
            out.push('-');
        } else {
            out.push('+');
        }
        let a = exp.abs();
        if a < 10 {
            out.push('0');
        }
        out.push_str(&a.to_string());
    } else {
        if dp <= 0 {
            out.push_str("0.");
            for _ in 0..(-dp) {
                out.push('0');
            }
            out.push_str(&digits);
        } else if dp >= nd {
            out.push_str(&digits);
            for _ in 0..(dp - nd) {
                out.push('0');
            }
        } else {
            out.push_str(&digits[..dp as usize]);
            out.push('.');
            out.push_str(&digits[dp as usize..]);
        }
    }
    out
}

/// Go `fmt.Sprintf("%v", f)` for a float64 (= `strconv.FormatFloat(f, 'g', -1, 64)`).
pub fn FormatFloatV(f: f64) -> String {
    FormatFloatShortest(f, b'g')
}

/// `strconv.Itoa`
pub fn Itoa(i: i64) -> String {
    i.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn v_format() {
        assert_eq!(FormatFloatV(1.0), "1");
        assert_eq!(FormatFloatV(-2.5), "-2.5");
        assert_eq!(FormatFloatV(1e6), "1e+06");
        assert_eq!(FormatFloatV(123456.0), "123456");
        assert_eq!(FormatFloatV(123456789.0), "1.23456789e+08");
        assert_eq!(FormatFloatV(0.0001), "0.0001");
        assert_eq!(FormatFloatV(0.00001), "1e-05");
        assert_eq!(FormatFloatV(0.0), "0");
        assert_eq!(FormatFloatV(-0.0), "-0");
        assert_eq!(FormatFloatV(1e21), "1e+21");
        assert_eq!(FormatFloatV(f64::INFINITY), "+Inf");
    }
}
