// SPDX-License-Identifier: BSD-3-Clause
// Copyright 2009 The Go Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSES file.
//
// Ported from Go 1.27.2 (https://go.dev/src):
//   - src/math/const.go
//   - src/math/unsafe.go
//   - src/math/bits.go
//   - src/math/abs.go
//   - src/math/signbit.go
//   - src/math/copysign.go
//   - src/math/floor.go
//   - src/math/dim.go
//   - src/math/dim_amd64.s
//   - src/runtime/minmax.go
//   - src/math/frexp.go
//   - src/math/ldexp.go
//   - src/math/modf.go
//   - src/math/mod.go
//   - src/math/exp.go
//   - src/math/exp_amd64.s
//   - src/math/log.go
//   - src/math/log_amd64.s
//   - src/math/hypot.go
//   - src/math/hypot_amd64.s
//   - src/math/pow.go
//   - src/math/atan.go
//   - src/math/atan2.go
//   - src/math/asin.go
//   - src/math/sin.go
//   - src/math/trig_reduce.go
//   - src/math/nextafter.go
// Rust port: Copyright 2026 Niklas Krieger.
//
// The original C code, the long comment, and the constants
// below are from FreeBSD's /usr/src/lib/msun/src/e_exp.c
// and came with this notice. The go code is a simplified
// version of the original C.
//
// ====================================================
// Copyright (C) 2004 by Sun Microsystems, Inc. All rights reserved.
//
// Permission to use, copy, modify, and distribute this
// software is freely granted, provided that this notice
// is preserved.
// ====================================================
//
// The original C code, the long comment, and the constants
// below are from FreeBSD's /usr/src/lib/msun/src/e_log.c
// and came with this notice. The go code is a simpler
// version of the original C.
//
// ====================================================
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
//
// Developed at SunPro, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this
// software is freely granted, provided that this notice
// is preserved.
// ====================================================
//
// Cephes Math Library Release 2.8:  June, 2000
// Copyright 1984, 1987, 1989, 1992, 2000 by Stephen L. Moshier
//
// The readme file at http://netlib.sandia.gov/cephes/ says:
//    Some software in this archive may be from the book _Methods and
// Programs for Mathematical Functions_ (Prentice-Hall or Simon & Schuster
// International, 1989) or from the Cephes Mathematical Library, a
// commercial product. In either event, it is copyrighted by the author.
// What you see here may be used freely but it comes with no support or
// guarantee.
//
//   The two known misprints in the book are repaired here in the
// source listings for the gamma function and the incomplete beta
// integral.
//
//   Stephen L. Moshier
//   moshier@na-net.ornl.gov

//! Go `math` exactly as executed natively by Go on linux/amd64 with AVX+FMA
//! (GOAMD64=v1): `Exp` = `exp_amd64.s` (useFMA path), `Log` = `log_amd64.s`,
//! `Hypot` = `hypot_amd64.s`, `Max`/`Min` = `dim_amd64.s`, Floor/Ceil/Trunc/Sqrt
//! are exact hardware ops, everything else is the pure-Go implementation.
//!
//! NOTE for porters: Go evaluates *constant* expressions (e.g. `math.Pi / 180`)
//! with exact arithmetic before rounding to float64. Where the Go operands are all
//! constants, verify the folded value (print `math.Float64bits` from Go) instead of
//! evaluating the expression in f64 at runtime.

pub const E: f64 = f64::from_bits(0x4005bf0a8b145769);
pub const Pi: f64 = f64::from_bits(0x400921fb54442d18);
pub const Phi: f64 = 1.61803398874989484820458683436563811772030917980576286213544862;
pub const Sqrt2: f64 = f64::from_bits(0x3ff6a09e667f3bcd);
pub const SqrtE: f64 = 1.64872127070012814684865078831408559368683596637233004219337478;
pub const SqrtPi: f64 = 1.77245385090551602729816748334114518279754945612238712821380779;
pub const SqrtPhi: f64 = 1.27201964951406896425242246173749149171560804184009624861664038;
pub const Ln2: f64 = f64::from_bits(0x3fe62e42fefa39ef);
pub const Log2E: f64 = f64::from_bits(0x3ff71547652b82fe);
pub const Ln10: f64 = 2.30258509299404568401799145468436420760110148862877297603332790;
pub const Log10E: f64 = 1.0 / Ln10;

pub const MaxFloat64: f64 = f64::MAX;
pub const SmallestNonzeroFloat64: f64 = f64::from_bits(1);
pub const MaxFloat32: f64 = f32::MAX as f64;
pub const SmallestNonzeroFloat32: f64 = 1.401298464324817070923729583289916131280e-45;

pub const MaxInt: i64 = i64::MAX;
pub const MinInt: i64 = i64::MIN;
pub const MaxInt8: i64 = i8::MAX as i64;
pub const MinInt8: i64 = i8::MIN as i64;
pub const MaxInt16: i64 = i16::MAX as i64;
pub const MinInt16: i64 = i16::MIN as i64;
pub const MaxInt32: i64 = i32::MAX as i64;
pub const MinInt32: i64 = i32::MIN as i64;
pub const MaxInt64: i64 = i64::MAX;
pub const MinInt64: i64 = i64::MIN;
pub const MaxUint8: u64 = u8::MAX as u64;
pub const MaxUint16: u64 = u16::MAX as u64;
pub const MaxUint32: u64 = u32::MAX as u64;
pub const MaxUint64: u64 = u64::MAX;
pub const MaxUint: u64 = u64::MAX;

const uvnan: u64 = 0x7FF8000000000001;
const uvinf: u64 = 0x7FF0000000000000;
const uvneginf: u64 = 0xFFF0000000000000;
const uvone: u64 = 0x3FF0000000000000;
const mask: u64 = 0x7FF;
const shift: u64 = 64 - 11 - 1;
const bias: i64 = 1023;
const signMask: u64 = 1 << 63;
const fracMask: u64 = (1 << shift) - 1;

#[inline]
pub fn Float64bits(f: f64) -> u64 {
    f.to_bits()
}
#[inline]
pub fn Float64frombits(b: u64) -> f64 {
    f64::from_bits(b)
}
#[inline]
pub fn Float32bits(f: f32) -> u32 {
    f.to_bits()
}
#[inline]
pub fn Float32frombits(b: u32) -> f32 {
    f32::from_bits(b)
}

#[inline]
pub fn Inf(sign: i64) -> f64 {
    if sign >= 0 {
        f64::from_bits(uvinf)
    } else {
        f64::from_bits(uvneginf)
    }
}
#[inline]
pub fn NaN() -> f64 {
    f64::from_bits(uvnan)
}
#[inline]
pub fn IsNaN(f: f64) -> bool {
    f != f
}
#[inline]
pub fn IsInf(f: f64, sign: i64) -> bool {
    let f = if sign == 0 {
        Abs(f)
    } else if sign < 0 {
        -f
    } else {
        f
    };
    f > MaxFloat64
}
#[inline]
pub fn Abs(x: f64) -> f64 {
    f64::from_bits(x.to_bits() & !signMask)
}
#[inline]
pub fn Signbit(x: f64) -> bool {
    x.to_bits() & signMask != 0
}
#[inline]
pub fn Copysign(f: f64, sign: f64) -> f64 {
    f64::from_bits((f.to_bits() & !signMask) | (sign.to_bits() & signMask))
}
#[inline]
pub fn Sqrt(x: f64) -> f64 {
    x.sqrt()
}
/// `math.Floor` (SSE4.1 ROUNDSD: exact).
#[inline]
pub fn Floor(x: f64) -> f64 {
    x.floor()
}
#[inline]
pub fn Ceil(x: f64) -> f64 {
    x.ceil()
}
#[inline]
pub fn Trunc(x: f64) -> f64 {
    x.trunc()
}

/// `math.Max` (dim_amd64.s): +Inf wins over NaN; NaN → NaN; Max(-0, +0) = +0.
#[inline]
pub fn Max(x: f64, y: f64) -> f64 {
    let xb = x.to_bits();
    let yb = y.to_bits();
    if xb == uvinf || yb == uvinf {
        return f64::from_bits(uvinf);
    }
    let ax = xb & !signMask;
    let ay = yb & !signMask;
    if ax > uvinf || ay > uvinf {
        return f64::from_bits(uvnan);
    }
    if ax | ay == 0 {
        if xb == signMask {
            return y;
        }
        return x;
    }
    // MAXSD X1, X0: X0 > X1 ? X0 : X1
    if x > y {
        x
    } else {
        y
    }
}

/// `math.Min` (dim_amd64.s): -Inf wins over NaN; NaN → NaN; Min(-0, +0) = -0.
#[inline]
pub fn Min(x: f64, y: f64) -> f64 {
    let xb = x.to_bits();
    let yb = y.to_bits();
    if xb == uvneginf || yb == uvneginf {
        return f64::from_bits(uvneginf);
    }
    let ax = xb & !signMask;
    let ay = yb & !signMask;
    if ax > uvinf || ay > uvinf {
        return f64::from_bits(uvnan);
    }
    if ax | ay == 0 {
        if xb == signMask {
            return x;
        }
        return y;
    }
    if x < y {
        x
    } else {
        y
    }
}

#[inline]
fn minsd(a: f64, b: f64) -> f64 {
    // x86 MINSD a, b: a < b ? a : b (b when unordered or equal)
    if a < b {
        a
    } else {
        b
    }
}

/// Go builtin `min(x, y)` on float64 as compiled for amd64:
/// `POR(MINSD(MINSD(x, y), x), MINSD(x, y))` (any NaN → NaN, -0 < +0).
#[inline]
pub fn BuiltinMin(x: f64, y: f64) -> f64 {
    let m = minsd(x, y);
    f64::from_bits(minsd(m, x).to_bits() | m.to_bits())
}

/// Go builtin `max(x, y)` on float64 as compiled for amd64: `-min(-x, -y)`.
#[inline]
pub fn BuiltinMax(x: f64, y: f64) -> f64 {
    let n = |v: f64| f64::from_bits(v.to_bits() ^ signMask);
    n(BuiltinMin(n(x), n(y)))
}

/// Go builtin `min(a, b, c, ...)` = `min(min(a, b), c)...`
pub fn BuiltinMinN(xs: &[f64]) -> f64 {
    let mut r = xs[0];
    for &x in &xs[1..] {
        r = BuiltinMin(r, x);
    }
    r
}

/// Go builtin `max(a, b, c, ...)`
pub fn BuiltinMaxN(xs: &[f64]) -> f64 {
    let mut r = xs[0];
    for &x in &xs[1..] {
        r = BuiltinMax(r, x);
    }
    r
}

/// `math.Dim`
pub fn Dim(x: f64, y: f64) -> f64 {
    let v = x - y;
    if v <= 0.0 {
        return 0.0;
    }
    v
}

/// `math.Round`: half away from zero.
pub fn Round(x: f64) -> f64 {
    let mut bits = x.to_bits();
    let mut e = (bits >> shift) & mask;
    if e < bias as u64 {
        bits &= signMask;
        if e == bias as u64 - 1 {
            bits |= uvone;
        }
    } else if e < bias as u64 + shift {
        const half: u64 = 1 << (shift - 1);
        e -= bias as u64;
        bits = bits.wrapping_add(half >> e);
        bits &= !(fracMask >> e);
    }
    f64::from_bits(bits)
}

/// `math.RoundToEven`
pub fn RoundToEven(x: f64) -> f64 {
    let mut bits = x.to_bits();
    let mut e = (bits >> shift) & mask;
    if e >= bias as u64 {
        const halfMinusULP: u64 = (1 << (shift - 1)) - 1;
        e -= bias as u64;
        // Go: shifts >= 64 yield 0 for unsigned values.
        let sh = shift.wrapping_sub(e);
        let odd = if sh >= 64 { 0 } else { (bits >> sh) & 1 };
        let add = if e >= 64 { 0 } else { (halfMinusULP + odd) >> e };
        bits = bits.wrapping_add(add);
        let m = if e >= 64 { 0 } else { fracMask >> e };
        bits &= !m;
    } else if e == bias as u64 - 1 && bits & fracMask != 0 {
        bits = (bits & signMask) | uvone;
    } else {
        bits &= signMask;
    }
    f64::from_bits(bits)
}

fn normalize(x: f64) -> (f64, i64) {
    const SmallestNormal: f64 = 2.2250738585072014e-308;
    if Abs(x) < SmallestNormal {
        return (x * (1u64 << 52) as f64, -52);
    }
    (x, 0)
}

/// `math.Frexp`
pub fn Frexp(f: f64) -> (f64, i64) {
    if f == 0.0 {
        return (f, 0);
    }
    if IsInf(f, 0) || IsNaN(f) {
        return (f, 0);
    }
    let (f, mut exp) = normalize(f);
    let mut x = f.to_bits();
    exp += ((x >> shift) & mask) as i64 - bias + 1;
    x &= !(mask << shift);
    x |= ((-1 + bias) as u64) << shift;
    (f64::from_bits(x), exp)
}

/// `math.Ldexp`
pub fn Ldexp(frac: f64, exp: i64) -> f64 {
    if frac == 0.0 {
        return frac;
    }
    if IsInf(frac, 0) || IsNaN(frac) {
        return frac;
    }
    let (frac, e) = normalize(frac);
    let mut exp = exp.wrapping_add(e);
    let mut x = frac.to_bits();
    exp = exp.wrapping_add(((x >> shift) as i64 & mask as i64) - bias);
    if exp < -1075 {
        return Copysign(0.0, frac);
    }
    if exp > 1023 {
        if frac < 0.0 {
            return Inf(-1);
        }
        return Inf(1);
    }
    let mut m: f64 = 1.0;
    if exp < -1022 {
        exp += 53;
        m = 1.0 / (1u64 << 53) as f64;
    }
    x &= !(mask << shift);
    x |= ((exp + bias) as u64) << shift;
    m * f64::from_bits(x)
}

/// `math.Modf`
pub fn Modf(f: f64) -> (f64, f64) {
    let integer = Trunc(f);
    let fractional = Copysign(f - integer, f);
    (integer, fractional)
}

/// `math.Mod` (pure Go).
pub fn Mod(x: f64, y: f64) -> f64 {
    if y == 0.0 || IsInf(x, 0) || IsNaN(x) || IsNaN(y) {
        return NaN();
    }
    let y = Abs(y);
    let (yfr, yexp) = Frexp(y);
    let mut r = x;
    if x < 0.0 {
        r = -x;
    }
    while r >= y {
        let (rfr, mut rexp) = Frexp(r);
        if rfr < yfr {
            rexp = rexp - 1;
        }
        r = r - Ldexp(y, rexp - yexp);
    }
    if x < 0.0 {
        r = -r;
    }
    r
}

/// `math.Exp` = `exp_amd64.s` with the AVX/FMA path (`useFMA == true`).
pub fn Exp(x: f64) -> f64 {
    const LOG2E: f64 = 1.4426950408889634073599246810018920;
    const LN2U: f64 = 0.69314718055966295651160180568695068359375;
    const LN2L: f64 = 0.28235290563031577122588448175013436025525412068e-12;
    const Overflow: f64 = 7.09782712893384e+02;
    const C0: f64 = 0.5;
    const C1: f64 = 1.0;
    const C2: f64 = 2.0;
    const C3: f64 = 1.6666666666666666667e-1;
    const C4: f64 = 4.1666666666666666667e-2;
    const C5: f64 = 8.3333333333333333333e-3;
    const C6: f64 = 1.3888888888888888889e-3;
    const C7: f64 = 1.9841269841269841270e-4;
    const C8: f64 = 2.4801587301587301587e-5;

    let bx = x.to_bits();
    // test bits for not-finite
    if (bx & !signMask) as i64 >= uvinf as i64 {
        if bx == uvneginf {
            return 0.0;
        }
        return x; // NaN or +Inf
    }
    if x > Overflow {
        return Inf(1);
    }
    let x1 = LOG2E * x;
    // CVTSD2SL: round to nearest even; out-of-range → 0x80000000.
    let mut k: i32 = cvtsd2sl(x1);
    let kf = k as f64;
    // VFNMADD231SD: x0 = -(k*LN2U) + x0
    let mut x0 = (-kf).mul_add(LN2U, x);
    x0 = (-kf).mul_add(LN2L, x0);
    x0 *= 0.0625;
    let mut p = C8;
    p = p.mul_add(x0, C7);
    p = p.mul_add(x0, C6);
    p = p.mul_add(x0, C5);
    p = p.mul_add(x0, C4);
    p = p.mul_add(x0, C3);
    p = p.mul_add(x0, C0);
    p = p.mul_add(x0, C1);
    x0 = x0 * p;
    let mut t = x0 + C2;
    x0 = x0 * t;
    t = x0 + C2;
    x0 = x0 * t;
    t = x0 + C2;
    x0 = x0 * t;
    t = x0 + C2;
    x0 = x0.mul_add(t, C1);
    // ldexp
    k = k.wrapping_add(0x3FF);
    if k <= 0 {
        // denormal
        if k < -52 {
            return 0.0;
        }
        k = k.wrapping_add(0x3FE);
        x0 = x0 * f64::from_bits(((k as u32) as u64) << 52);
        k = 1;
    } else if k >= 0x7FF {
        return Inf(1);
    }
    x0 * f64::from_bits(((k as u32) as u64) << 52)
}

#[inline]
fn cvtsd2sl(x: f64) -> i32 {
    // x86 CVTSD2SI (32-bit) with MXCSR round-to-nearest-even.
    let r = x.round_ties_even();
    if r.is_nan() || r >= 2147483648.0 || r < -2147483648.0 {
        i32::MIN
    } else {
        r as i32
    }
}

/// `math.Log` = `log_amd64.s`.
pub fn Log(x: f64) -> f64 {
    const HSqrt2: f64 = 7.07106781186547524401e-01;
    const Ln2Hi: f64 = 6.93147180369123816490e-01;
    const Ln2Lo: f64 = 1.90821492927058770002e-10;
    const L1: f64 = 6.666666666666735130e-01;
    const L2: f64 = 3.999999999940941908e-01;
    const L3: f64 = 2.857142874366239149e-01;
    const L4: f64 = 2.222219843214978396e-01;
    const L5: f64 = 1.818357216161805012e-01;
    const L6: f64 = 1.531383769920937332e-01;
    const L7: f64 = 1.479819860511658591e-01;

    let bx = x.to_bits();
    if bx & !signMask == 0 {
        return Inf(-1);
    }
    if (bx as i64) < 0 {
        return f64::from_bits(uvnan);
    }
    if bx >= uvinf {
        return x;
    }
    let mut f1 = f64::from_bits((bx & 0x000FFFFFFFFFFFFF) | 0x3FE0000000000000);
    let ki = (((bx >> 52) & 0x7FF) as i32) - 0x3FE;
    let mut k = ki as f64;
    // CMPSD (cmpnlt): mask = !(HSqrt2 < f1)
    let x3 = if !(HSqrt2 < f1) { 1.0 } else { 0.0 };
    k = k - x3;
    f1 = f1 * (x3 + 1.0);
    let f = f1 - 1.0;
    let s = f / (2.0 + f);
    let s2 = s * s;
    let s4 = s2 * s2;
    let mut t = L7 * s4;
    t = t + L5;
    t = t * s4;
    t = t + L3;
    t = t * s4;
    t = t + L1;
    let t1 = s2 * t;
    let mut u = L6 * s4;
    u = u + L4;
    u = u * s4;
    u = u + L2;
    let t2 = s4 * u;
    let R = t1 + t2;
    let hfsq = (0.5 * f) * f;
    let a = s * (R + hfsq);
    let b = Ln2Lo * k;
    let c = a + b;
    let d = hfsq - c;
    let e = d - f;
    let g = k * Ln2Hi;
    g - e
}

/// `math.Hypot` = `hypot_amd64.s`.
pub fn Hypot(p: f64, q: f64) -> f64 {
    let pb = p.to_bits() & !signMask;
    let qb = q.to_bits() & !signMask;
    if pb >= uvinf || qb >= uvinf {
        if pb == uvinf || qb == uvinf {
            return Inf(1);
        }
        return f64::from_bits(uvnan);
    }
    if pb | qb == 0 {
        return 0.0;
    }
    let p = f64::from_bits(pb);
    let q = f64::from_bits(qb);
    let mx = if p > q { p } else { q };
    let mn = if p < q { p } else { q };
    let mut r = mn / mx;
    r = r * r;
    r = r + 1.0;
    r = r.sqrt();
    mx * r
}

fn isOddInt(x: f64) -> bool {
    if Abs(x) >= (1u64 << 53) as f64 {
        return false;
    }
    let (xi, xf) = Modf(x);
    xf == 0.0 && (xi as i64) & 1 == 1
}

/// `math.Pow` (pure Go, using the amd64 `Exp`/`Log`).
pub fn Pow(x: f64, y: f64) -> f64 {
    if y == 0.0 || x == 1.0 {
        return 1.0;
    } else if y == 1.0 {
        return x;
    } else if IsNaN(x) || IsNaN(y) {
        return NaN();
    } else if x == 0.0 {
        if y < 0.0 {
            if Signbit(x) && isOddInt(y) {
                return Inf(-1);
            }
            return Inf(1);
        } else if y > 0.0 {
            if Signbit(x) && isOddInt(y) {
                return x;
            }
            return 0.0;
        }
    } else if IsInf(y, 0) {
        if x == -1.0 {
            return 1.0;
        } else if (Abs(x) < 1.0) == IsInf(y, 1) {
            return 0.0;
        } else {
            return Inf(1);
        }
    } else if IsInf(x, 0) {
        if IsInf(x, -1) {
            return Pow(1.0 / x, -y);
        }
        if y < 0.0 {
            return 0.0;
        } else if y > 0.0 {
            return Inf(1);
        }
    } else if y == 0.5 {
        return Sqrt(x);
    } else if y == -0.5 {
        return 1.0 / Sqrt(x);
    }

    let (mut yi, mut yf) = Modf(Abs(y));
    if yf != 0.0 && x < 0.0 {
        return NaN();
    }
    if yi >= 9223372036854775808.0 {
        if x == -1.0 {
            return 1.0;
        } else if (Abs(x) < 1.0) == (y > 0.0) {
            return 0.0;
        } else {
            return Inf(1);
        }
    }

    let mut a1: f64 = 1.0;
    let mut ae: i64 = 0;

    if yf != 0.0 {
        if yf > 0.5 {
            yf -= 1.0;
            yi += 1.0;
        }
        a1 = Exp(yf * Log(x));
    }

    let (mut x1, mut xe) = Frexp(x);
    let mut i = yi as i64;
    while i != 0 {
        if xe < -(1 << 12) || (1 << 12) < xe {
            ae += xe;
            break;
        }
        if i & 1 == 1 {
            a1 *= x1;
            ae += xe;
        }
        x1 *= x1;
        xe <<= 1;
        if x1 < 0.5 {
            x1 += x1;
            xe -= 1;
        }
        i >>= 1;
    }

    if y < 0.0 {
        a1 = 1.0 / a1;
        ae = -ae;
    }
    Ldexp(a1, ae)
}

fn xatan(x: f64) -> f64 {
    const P0: f64 = -8.750608600031904122785e-01;
    const P1: f64 = -1.615753718733365076637e+01;
    const P2: f64 = -7.500855792314704667340e+01;
    const P3: f64 = -1.228866684490136173410e+02;
    const P4: f64 = -6.485021904942025371773e+01;
    const Q0: f64 = 2.485846490142306297962e+01;
    const Q1: f64 = 1.650270098316988542046e+02;
    const Q2: f64 = 4.328810604912902668951e+02;
    const Q3: f64 = 4.853903996359136964868e+02;
    const Q4: f64 = 1.945506571482613964425e+02;
    let mut z = x * x;
    z = z * ((((P0 * z + P1) * z + P2) * z + P3) * z + P4) / (((((z + Q0) * z + Q1) * z + Q2) * z + Q3) * z + Q4);
    z = x * z + x;
    z
}

fn satan(x: f64) -> f64 {
    const Morebits: f64 = 6.123233995736765886130e-17;
    const Tan3pio8: f64 = 2.41421356237309504880;
    if x <= 0.66 {
        return xatan(x);
    }
    if x > Tan3pio8 {
        return Pi / 2.0 - xatan(1.0 / x) + Morebits;
    }
    Pi / 4.0 + xatan((x - 1.0) / (x + 1.0)) + 0.5 * Morebits
}

/// `math.Atan`
pub fn Atan(x: f64) -> f64 {
    if x == 0.0 {
        return x;
    }
    if x > 0.0 {
        return satan(x);
    }
    -satan(-x)
}

/// `math.Atan2`
pub fn Atan2(y: f64, x: f64) -> f64 {
    if IsNaN(y) || IsNaN(x) {
        return NaN();
    } else if y == 0.0 {
        if x >= 0.0 && !Signbit(x) {
            return Copysign(0.0, y);
        }
        return Copysign(Pi, y);
    } else if x == 0.0 {
        return Copysign(Pi / 2.0, y);
    } else if IsInf(x, 0) {
        if IsInf(x, 1) {
            if IsInf(y, 0) {
                return Copysign(Pi / 4.0, y);
            }
            return Copysign(0.0, y);
        }
        if IsInf(y, 0) {
            // Go constant 3*Pi/4
            return Copysign(f64::from_bits(0x4002d97c7f3321d2), y);
        }
        return Copysign(Pi, y);
    } else if IsInf(y, 0) {
        return Copysign(Pi / 2.0, y);
    }
    let q = Atan(y / x);
    if x < 0.0 {
        if q <= 0.0 {
            return q + Pi;
        }
        return q - Pi;
    }
    q
}

/// `math.Asin`
pub fn Asin(x: f64) -> f64 {
    let mut x = x;
    if x == 0.0 {
        return x;
    }
    let mut sign = false;
    if x < 0.0 {
        x = -x;
        sign = true;
    }
    if x > 1.0 {
        return NaN();
    }
    let mut temp = Sqrt(1.0 - x * x);
    if x > 0.7 {
        temp = Pi / 2.0 - satan(temp / x);
    } else {
        temp = satan(x / temp);
    }
    if sign {
        temp = -temp;
    }
    temp
}

/// `math.Acos`
pub fn Acos(x: f64) -> f64 {
    Pi / 2.0 - Asin(x)
}

static _sin: [f64; 6] = [
    1.58962301576546568060e-10,
    -2.50507477628578072866e-8,
    2.75573136213857245213e-6,
    -1.98412698295895385996e-4,
    8.33333333332211858878e-3,
    -1.66666666666666307295e-1,
];
static _cos: [f64; 6] = [
    -1.13585365213876817300e-11,
    2.08757008419747316778e-9,
    -2.75573141792967388112e-7,
    2.48015872888517045348e-5,
    -1.38888888888730564116e-3,
    4.16666666666665929218e-2,
];

const PI4A: f64 = 7.85398125648498535156e-1;
const PI4B: f64 = 3.77489470793079817668e-8;
const PI4C: f64 = 2.69515142907905952645e-15;
const reduceThreshold: f64 = (1u64 << 29) as f64;
// Go constant 4/Pi
const FourOverPi: f64 = f64::from_bits(0x3ff45f306dc9c883);

/// `math.Cos`
pub fn Cos(x: f64) -> f64 {
    if IsNaN(x) || IsInf(x, 0) {
        return NaN();
    }
    let mut sign = false;
    let x = Abs(x);
    let mut j: u64;
    let mut y: f64;
    let z: f64;
    if x >= reduceThreshold {
        let (jj, zz) = trigReduce(x);
        j = jj;
        z = zz;
    } else {
        j = (x * FourOverPi) as u64;
        y = j as f64;
        if j & 1 == 1 {
            j += 1;
            y += 1.0;
        }
        j &= 7;
        z = ((x - y * PI4A) - y * PI4B) - y * PI4C;
    }
    if j > 3 {
        j -= 4;
        sign = !sign;
    }
    if j > 1 {
        sign = !sign;
    }
    let zz = z * z;
    if j == 1 || j == 2 {
        y = z + z * zz * ((((((_sin[0] * zz) + _sin[1]) * zz + _sin[2]) * zz + _sin[3]) * zz + _sin[4]) * zz + _sin[5]);
    } else {
        y = 1.0 - 0.5 * zz + zz * zz * ((((((_cos[0] * zz) + _cos[1]) * zz + _cos[2]) * zz + _cos[3]) * zz + _cos[4]) * zz + _cos[5]);
    }
    if sign {
        y = -y;
    }
    y
}

/// `math.Sin`
pub fn Sin(x: f64) -> f64 {
    if x == 0.0 || IsNaN(x) {
        return x;
    }
    if IsInf(x, 0) {
        return NaN();
    }
    let mut x = x;
    let mut sign = false;
    if x < 0.0 {
        x = -x;
        sign = true;
    }
    let mut j: u64;
    let mut y: f64;
    let z: f64;
    if x >= reduceThreshold {
        let (jj, zz) = trigReduce(x);
        j = jj;
        z = zz;
    } else {
        j = (x * FourOverPi) as u64;
        y = j as f64;
        if j & 1 == 1 {
            j += 1;
            y += 1.0;
        }
        j &= 7;
        z = ((x - y * PI4A) - y * PI4B) - y * PI4C;
    }
    if j > 3 {
        sign = !sign;
        j -= 4;
    }
    let zz = z * z;
    if j == 1 || j == 2 {
        y = 1.0 - 0.5 * zz + zz * zz * ((((((_cos[0] * zz) + _cos[1]) * zz + _cos[2]) * zz + _cos[3]) * zz + _cos[4]) * zz + _cos[5]);
    } else {
        y = z + z * zz * ((((((_sin[0] * zz) + _sin[1]) * zz + _sin[2]) * zz + _sin[3]) * zz + _sin[4]) * zz + _sin[5]);
    }
    if sign {
        y = -y;
    }
    y
}

static mPi4: [u64; 20] = [
    0x0000000000000001,
    0x45f306dc9c882a53,
    0xf84eafa3ea69bb81,
    0xb6c52b3278872083,
    0xfca2c757bd778ac3,
    0x6e48dc74849ba5c0,
    0x0c925dd413a32439,
    0xfc3bd63962534e7d,
    0xd1046bea5d768909,
    0xd338e04d68befc82,
    0x7323ac7306a673e9,
    0x3908bf177bf25076,
    0x3ff12fffbc0b301f,
    0xde5e2316b414da3e,
    0xda6cfd9e4f96136e,
    0x9e8c7ecd3cbfd45a,
    0xea4f758fd7cbe2f6,
    0x7a0e73ef14a525d4,
    0xd7f6bf623f1aba10,
    0xac06608df8f6d757,
];

#[inline]
fn shl(x: u64, s: u64) -> u64 {
    if s >= 64 {
        0
    } else {
        x << s
    }
}
#[inline]
fn shr(x: u64, s: u64) -> u64 {
    if s >= 64 {
        0
    } else {
        x >> s
    }
}

fn trigReduce(x: f64) -> (u64, f64) {
    const PI4: f64 = Pi / 4.0;
    if x < PI4 {
        return (0, x);
    }
    let mut ix = x.to_bits();
    let exp = ((ix >> shift) & mask) as i64 - bias - shift as i64;
    ix &= !(mask << shift);
    ix |= 1 << shift;
    let digit = ((exp + 61) as u64 / 64) as usize;
    let bitshift = (exp + 61) as u64 % 64;
    let z0 = shl(mPi4[digit], bitshift) | shr(mPi4[digit + 1], 64 - bitshift);
    let z1 = shl(mPi4[digit + 1], bitshift) | shr(mPi4[digit + 2], 64 - bitshift);
    let z2 = shl(mPi4[digit + 2], bitshift) | shr(mPi4[digit + 3], 64 - bitshift);
    let (z2hi, _) = crate::bits::Mul64(z2, ix);
    let (z1hi, z1lo) = crate::bits::Mul64(z1, ix);
    let z0lo = z0.wrapping_mul(ix);
    let (lo, c) = crate::bits::Add64(z1lo, z2hi, 0);
    let (mut hi, _) = crate::bits::Add64(z0lo, z1hi, c);
    let mut j = hi >> 61;
    hi = hi << 3 | lo >> 61;
    let lz = hi.leading_zeros() as u64;
    let e = (bias as u64).wrapping_sub(lz + 1);
    hi = shl(hi, lz + 1) | shr(lo, 64 - (lz + 1));
    hi >>= 64 - shift;
    hi |= e << shift;
    let mut z = f64::from_bits(hi);
    if j & 1 == 1 {
        j += 1;
        j &= 7;
        z -= 1.0;
    }
    (j, z * PI4)
}

/// `math.Nextafter`
pub fn Nextafter(x: f64, y: f64) -> f64 {
    if IsNaN(x) || IsNaN(y) {
        NaN()
    } else if x == y {
        x
    } else if x == 0.0 {
        Copysign(f64::from_bits(1), y)
    } else if (y > x) == (x > 0.0) {
        f64::from_bits(x.to_bits() + 1)
    } else {
        f64::from_bits(x.to_bits() - 1)
    }
}

/// Go `int(f)` conversion for float64 → int64 on amd64 (CVTTSD2SQ: truncation;
/// NaN and out-of-range values give 0x8000000000000000).
#[inline]
pub fn F64ToInt(f: f64) -> i64 {
    if f.is_nan() || f >= 9223372036854775808.0 || f < -9223372036854775808.0 {
        i64::MIN
    } else {
        f as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn simple() {
        assert_eq!(Exp(0.0), 1.0);
        assert_eq!(Log(1.0), 0.0);
        assert_eq!(Hypot(3.0, 4.0), 5.0);
        assert_eq!(Pow(2.0, 10.0), 1024.0);
        assert!(Signbit(Min(0.0, -0.0)));
        assert!(!Signbit(Max(0.0, -0.0)));
        assert_eq!(Max(f64::NAN, f64::INFINITY), f64::INFINITY);
        assert!(BuiltinMax(f64::NAN, f64::INFINITY).is_nan());
        assert_eq!(Round(-2.5), -3.0);
        assert_eq!(Mod(7.5, 2.0), 1.5);
    }
}
