// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger
//
// Parity tests against goref/d2/d2layouts/d2talalayout/cmd/basegen (Go 1.27.2 stdlib behaviour).

//! Bit-exact parity against fixtures produced by Go
//! (`goref/base-parity/gen`; `scripts/gen-goref.sh` runs it). `rand.txt` is
//! shipped in `tests/fixtures/go-unit`; the large `sort.txt` and `math.txt`
//! are read from `$ORTHOFLOW_GOREF/base-parity` (ignored tests).
#![allow(non_snake_case)]
extern crate orthoflow_gocompat as gocompat;
use gocompat::math;
use gocompat::rand::Rand;
use gocompat::sort as gsort;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let root = std::fs::canonicalize(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).unwrap();
    let shipped: PathBuf = root.join("tests/fixtures/go-unit").join(name);
    let p: PathBuf = if shipped.exists() {
        shipped
    } else {
        let goref = std::env::var_os("ORTHOFLOW_GOREF")
            .expect("needs Go reference data: set ORTHOFLOW_GOREF to the output of scripts/gen-goref.sh");
        root.join(goref).join("base-parity").join(name)
    };
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("missing fixture {}: {e}", p.display()))
}

fn hexf(f: f64) -> String {
    format!("{:016x}", f.to_bits())
}
fn fromhex(s: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(s, 16).unwrap())
}
fn ints(v: &[i64]) -> String {
    format!("[{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))
}
fn parse_ints(s: &str) -> Vec<i64> {
    let s = s.trim_start_matches('[').trim_end_matches(']');
    if s.is_empty() {
        return vec![];
    }
    s.split(',').map(|x| x.parse().unwrap()).collect()
}
fn parse_fl(s: &str) -> Vec<f64> {
    let s = s.trim_start_matches('[').trim_end_matches(']');
    if s.is_empty() {
        return vec![];
    }
    s.split(',').map(fromhex).collect()
}
fn fl(v: &[f64]) -> String {
    format!("[{}]", v.iter().map(|x| hexf(*x)).collect::<Vec<_>>().join(","))
}

#[test]
fn rand_parity() {
    let data = fixture("rand.txt");
    let mut r = Rand::new(0);
    let mut n = 0;
    for line in data.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        let got = match f[0] {
            "seed" => {
                r = if f[1] == "long" { Rand::new(12345) } else { Rand::new(f[1].parse().unwrap()) };
                continue;
            }
            "Int63" => format!("Int63 {}", r.Int63()),
            "Uint32" => format!("Uint32 {}", r.Uint32()),
            "Uint64" => format!("Uint64 {}", r.Uint64()),
            "Int31" => format!("Int31 {}", r.Int31()),
            "Int" => format!("Int {}", r.Int()),
            "Int63n" => {
                let a: i64 = f[1].parse().unwrap();
                format!("Int63n {} {}", a, r.Int63n(a))
            }
            "Int31n" => {
                let a: i32 = f[1].parse().unwrap();
                format!("Int31n {} {}", a, r.Int31n(a))
            }
            "Intn" => {
                let a: i64 = f[1].parse().unwrap();
                format!("Intn {} {}", a, r.Intn(a))
            }
            "Float64" => format!("Float64 {}", hexf(r.Float64())),
            "Perm" => {
                let a: i64 = f[1].parse().unwrap();
                format!("Perm {} {}", a, ints(&r.Perm(a)))
            }
            "Shuffle" => {
                let a: i64 = f[1].parse().unwrap();
                let mut v: Vec<i64> = (0..a).collect();
                r.Shuffle(a, |i, j| v.swap(i, j));
                format!("Shuffle {} {}", a, ints(&v))
            }
            other => panic!("unknown op {other}"),
        };
        assert_eq!(got, line);
        n += 1;
    }
    assert!(n > 9000);
}

#[derive(Clone, Copy)]
struct Item {
    key: i64,
    idx: i64,
}
struct ByKey<'a>(&'a mut Vec<Item>);
impl<'a> gsort::Interface for ByKey<'a> {
    fn Len(&self) -> i64 {
        self.0.len() as i64
    }
    fn Less(&mut self, i: usize, j: usize) -> bool {
        self.0[i].key < self.0[j].key
    }
    fn Swap(&mut self, i: usize, j: usize) {
        self.0.swap(i, j)
    }
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn sort_parity() {
    let data = fixture("sort.txt");
    let mut n = 0;
    for line in data.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        match f[0] {
            "FloatSort" => {
                let v = parse_fl(f[1]);
                let mut a = v.clone();
                gsort::SlicesSort(&mut a);
                let mut b = v.clone();
                gsort::Float64s(&mut b);
                assert_eq!(fl(&a), f[2], "{line}");
                assert_eq!(fl(&b), f[3], "{line}");
            }
            "Ints" => {
                let mut v = parse_ints(f[1]);
                gsort::Ints(&mut v);
                assert_eq!(ints(&v), f[2]);
            }
            "Search" => {
                let v = parse_ints(f[1]);
                let t: i64 = f[2].parse().unwrap();
                let s = gsort::Search(v.len() as i64, |i| v[i as usize] >= t);
                let (b, found) = gsort::SlicesBinarySearch(&v, &t);
                let (bf, foundf) = gsort::SlicesBinarySearchFunc(&v, &t, |a, b| *a - *b);
                let got = format!("Search {} {} {} {} {} {} {}", f[1], t, s, b, found, bf, foundf);
                assert_eq!(got, line);
            }
            entry => {
                let keys = parse_ints(f[2]);
                let mut items: Vec<Item> = keys.iter().enumerate().map(|(i, k)| Item { key: *k, idx: i as i64 }).collect();
                match entry {
                    "Slice" => gsort::SortSlice(&mut items, |a, b| a.key < b.key),
                    "SliceStable" => gsort::SortSliceStable(&mut items, |a, b| a.key < b.key),
                    "Sort" => gsort::Sort(&mut ByKey(&mut items)),
                    "Stable" => gsort::Stable(&mut ByKey(&mut items)),
                    "SortFunc" => gsort::SlicesSortFunc(&mut items, |a, b| a.key - b.key),
                    "SortStableFunc" => gsort::SlicesSortStableFunc(&mut items, |a, b| a.key - b.key),
                    _ => panic!("unknown entry {entry}"),
                }
                let perm: Vec<i64> = items.iter().map(|i| i.idx).collect();
                assert_eq!(ints(&perm), f[3], "{} {} n={}", entry, f[1], keys.len());
                // index-based variant must match too
                if entry == "Slice" {
                    let mut items2: Vec<Item> = keys.iter().enumerate().map(|(i, k)| Item { key: *k, idx: i as i64 }).collect();
                    gsort::SortSliceIdx(&mut items2, |x, i, j| x[i].key < x[j].key);
                    let perm2: Vec<i64> = items2.iter().map(|i| i.idx).collect();
                    assert_eq!(perm2, perm);
                }
            }
        }
        n += 1;
    }
    assert!(n > 18000);
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn math_parity() {
    let data = fixture("math.txt");
    let mut n = 0;
    let mut failures: Vec<String> = Vec::new();
    for line in data.lines() {
        let (op, rest) = line.split_once(' ').unwrap();
        let f: Vec<&str> = rest.split(' ').collect();
        let got: String = match op {
            "Frexp" => {
                let (fr, e) = math::Frexp(fromhex(f[0]));
                format!("{} {} {}", f[0], hexf(fr), e)
            }
            "Modf" => {
                let (a, b) = math::Modf(fromhex(f[0]));
                format!("{} {} {}", f[0], hexf(a), hexf(b))
            }
            "Ldexp" => format!("{} {} {}", f[0], f[1], hexf(math::Ldexp(fromhex(f[0]), f[1].parse().unwrap()))),
            "F64ToInt" => format!("{} {}", f[0], math::F64ToInt(fromhex(f[0]))),
            "FormatV" => {
                let x = fromhex(f[0]);
                format!("{} {}", f[0], gocompat::strconv::FormatFloatV(x))
            }
            _ if f.len() == 2 => {
                let x = fromhex(f[0]);
                let r = match op {
                    "Exp" => math::Exp(x),
                    "Log" => math::Log(x),
                    "Atan" => math::Atan(x),
                    "Asin" => math::Asin(x),
                    "Acos" => math::Acos(x),
                    "Sin" => math::Sin(x),
                    "Cos" => math::Cos(x),
                    "Round" => math::Round(x),
                    "RoundToEven" => math::RoundToEven(x),
                    "Floor" => math::Floor(x),
                    "Ceil" => math::Ceil(x),
                    "Trunc" => math::Trunc(x),
                    "Sqrt" => math::Sqrt(x),
                    "Abs" => math::Abs(x),
                    _ => panic!("unknown op {op}"),
                };
                format!("{} {}", f[0], hexf(r))
            }
            _ => {
                let (x, y) = (fromhex(f[0]), fromhex(f[1]));
                let r = match op {
                    "Pow" => math::Pow(x, y),
                    "Hypot" => math::Hypot(x, y),
                    "Atan2" => math::Atan2(x, y),
                    "Mod" => math::Mod(x, y),
                    "Max" => math::Max(x, y),
                    "Min" => math::Min(x, y),
                    "Nextafter" => math::Nextafter(x, y),
                    "Copysign" => math::Copysign(x, y),
                    "BuiltinMax" => math::BuiltinMax(x, y),
                    "BuiltinMin" => math::BuiltinMin(x, y),
                    _ => panic!("unknown op {op}"),
                };
                format!("{} {} {}", f[0], f[1], hexf(r))
            }
        };
        if got != rest {
            failures.push(format!("{op}: want {rest} got {got}"));
        }
        n += 1;
    }
    let mut by_op: std::collections::BTreeMap<String, usize> = Default::default();
    for f in &failures {
        *by_op.entry(f.split(':').next().unwrap().to_string()).or_default() += 1;
    }
    assert!(failures.is_empty(), "{} failures {:?}; first: {:#?}", failures.len(), by_op, &failures[..failures.len().min(15)]);
    assert!(n > 300000);
}
