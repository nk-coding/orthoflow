// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/labelgeom/arrowhead_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use geo::NewPoint;

#[test]
fn TestArrowheadTopLeftPreservesHistoricalTruncation() {
    let route = vec![NewPoint(0.0, 0.0), NewPoint(100.0, 0.0)];
    let got = ArrowheadTopLeft(&route, false, "triangle", "none", 30.9, 12.9);
    assert!(got.Equals(&NewPoint(5.0, -20.0)), "{:?}", got.get());
}
