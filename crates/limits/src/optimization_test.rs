// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/limits/optimization_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use gocompat::context;
use gocompat::rand::Rand;

#[test]
fn TestOptimizationWorkGuardRejectsArithmeticOverflow() {
    let guard = NewOptimizationWorkGuard(&context::Background(), "test", u64::MAX).unwrap();
    guard.Add(u64::MAX).unwrap();
    assert!(guard.Step().unwrap_err().Is(&ErrOptimizationResourceLimit));
    let guard = NewOptimizationWorkGuard(&context::Background(), "test", u64::MAX).unwrap();
    assert!(guard.AddProduct(u64::MAX, 2).unwrap_err().Is(&ErrOptimizationResourceLimit));
}

#[test]
fn TestOptimizationWorkGuardTreatsZeroAsZeroWork() {
    let guard = NewOptimizationWorkGuard(&context::Background(), "test", 0).unwrap();
    assert!(guard.Step().unwrap_err().Is(&ErrOptimizationResourceLimit));
}

#[test]
fn TestOptimizationShuffleMatchesMathRand() {
    for count in [0usize, 1, 2, 3, 10, 127, 1024] {
        let mut want: Vec<usize> = (0..count).collect();
        let mut got: Vec<usize> = (0..count).collect();
        let mut wantRand = Rand::new(991);
        let mut gotRand = Rand::new(991);
        wantRand.Shuffle(want.len() as i64, |i, j| want.swap(i, j));
        let guard = NewOptimizationWorkGuard(&context::Background(), "test", MaxOptimizationWorkUnits).unwrap();
        Shuffle(&mut got, &mut gotRand, &mut guard.bm()).unwrap();
        assert_eq!(got, want, "count {count}");
        assert_eq!(gotRand.Int63(), wantRand.Int63());
    }
}

#[test]
fn TestOptimizationShuffleRetriesRejectedDraw() {
    let n: i32 = 1_431_655_766;
    let threshold = (n.wrapping_neg() as u32) % (n as u32);
    let mut seed: i64 = 0;
    let mut want: i32 = 0;
    let mut draws: u64 = 0;
    while seed < 100 {
        let mut random = Rand::new(seed);
        draws = 0;
        loop {
            draws += 1;
            let product = (random.Uint32() as u64) * (n as u64);
            if (product as u32) >= threshold {
                want = (product >> 32) as i32;
                break;
            }
        }
        if draws > 1 {
            break;
        }
        seed += 1;
    }
    assert!(draws > 1);
    let guard = NewOptimizationWorkGuard(&context::Background(), "test", MaxOptimizationWorkUnits).unwrap();
    let got = shuffleIndex(&mut Rand::new(seed), n, &mut guard.bm()).unwrap();
    assert_eq!(got, want);
    assert_eq!(guard.Used(), draws);
}
