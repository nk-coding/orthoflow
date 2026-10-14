// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/port_interior_balance_test.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/port_interior_balance_test.go.
use super::*;

fn fixedPortGraph(kind: &str) -> (GraphRef, EdgeRef) {
    let g = layoutgraph::NewGraph();
    let from = g.AddNode(&layoutgraph::NewNode(1, 100.0, 100.0));
    from.set_tl(Some(pt(0.0, 0.0)));
    from.SetShape(kind);
    let to = g.AddNode(&layoutgraph::NewNode(2, 100.0, 100.0));
    to.set_tl(Some(pt(300.0, 0.0)));
    to.SetShape(kind);
    let e = g.Connect(&from, &to);
    if kind == shape::TABLE_TYPE {
        from.SetNumColumns(1);
        to.SetNumColumns(1);
        e.bm().FromTableColumnIndex = Some(0);
        e.bm().ToTableColumnIndex = Some(0);
    }
    (g, e)
}

#[test]
fn TestBalanceFixedPortsMovesOnlyInteriorCorridor() {
    for kind in [shape::TABLE_TYPE, shape::DIAMOND_TYPE] {
        let (g, e) = fixedPortGraph(kind);
        setPoints(
            &e,
            &[(100.0, 50.0), (130.0, 50.0), (130.0, 200.0), (220.0, 200.0), (220.0, 300.0), (270.0, 300.0), (270.0, 50.0), (300.0, 50.0)],
        );
        let original = pointValues(&e.b().Points);
        let before = quality::Inspect(&bg(), Some(&g)).unwrap();
        BalanceEdgeSegments(&bg(), &g).unwrap();
        let now = pointValues(&e.b().Points);
        let (n, m) = (now.len(), original.len());
        assert!(
            now[0] == original[0] && now[1] == original[1] && now[n - 2] == original[m - 2] && now[n - 1] == original[m - 1],
            "fixed approach moved ({kind})"
        );
        let mut moved = n != m;
        for (i, p) in now.iter().enumerate() {
            if i >= m || *p != original[i] {
                moved = true;
            }
        }
        assert!(moved, "interior corridor remained frozen ({kind})");
        let after = quality::Inspect(&bg(), Some(&g)).unwrap();
        assert!(
            !(after.RouteObstructions > before.RouteObstructions || after.Crossings > before.Crossings),
            "balancing introduced conflict ({kind})"
        );
    }
}

#[test]
fn TestBalanceFixedPortsPreservesCurves() {
    for kind in [shape::TABLE_TYPE, shape::DIAMOND_TYPE] {
        let (g, e) = fixedPortGraph(kind);
        e.bm().IsCurve = true;
        setPoints(&e, &[(100.0, 50.0), (130.0, 50.0), (130.0, 200.0), (220.0, 200.0), (220.0, 300.0), (300.0, 300.0), (300.0, 50.0)]);
        let original = pointValues(&e.b().Points);
        BalanceEdgeSegments(&bg(), &g).unwrap();
        assert!(e.b().IsCurve && e.b().Points.len() == original.len(), "curve representation changed");
        for (i, p) in pointValues(&e.b().Points).iter().enumerate() {
            assert_eq!(*p, original[i], "curve point {i} moved");
        }
    }
}
