// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

//! Rust twin of the Go oracle `goref/d2/d2layouts/d2talalayout/cmd/graphparity`.
//! Prints the same report for every fixture listed in
//! `$ORTHOFLOW_GOREF/graph-parity/files.txt` (paths relative to
//! `$ORTHOFLOW_GOREF`) and compares it with the Go output
//! (`graph-parity/expected.txt`, both written by `scripts/gen-goref.sh`).
#![allow(non_snake_case)]
extern crate orthoflow_graphjson as graphjson;
use gocompat::Ctx;
use layoutgraph::*;
use std::fmt::Write as _;


fn f(x: f64) -> String {
    gocompat::strconv::FormatFloatV(x)
}
fn p(pt: Option<&geo::PointRef>) -> String {
    match pt {
        None => "nil".into(),
        Some(pt) => {
            let v = *pt.b();
            format!("{},{}", f(v.X), f(v.Y))
        }
    }
}
fn pv(pt: geo::Point) -> String {
    format!("{},{}", f(pt.X), f(pt.Y))
}
fn id(n: Option<&NodeRef>) -> String {
    match n {
        None => "0".into(),
        Some(n) => n.id().to_string(),
    }
}
fn ids(ns: &[NodeRef]) -> String {
    let v: Vec<String> = ns.iter().map(|n| n.id().to_string()).collect();
    format!("[{}]", v.join(" "))
}
fn optids(ns: &[Option<NodeRef>]) -> String {
    let v: Vec<String> = ns.iter().map(|n| id(n.as_ref())).collect();
    format!("[{}]", v.join(" "))
}
fn sortedIDs(ns: &[NodeRef]) -> String {
    let mut c = ns.to_vec();
    c.sort_by_key(|n| n.id());
    ids(&c)
}
fn errs<T>(r: &gocompat::GoResult<T>) -> &'static str {
    match r {
        Ok(_) => "ok",
        Err(e) if IsCandidateRejection(Some(e)) => "reject",
        Err(_) => "error",
    }
}
fn positions(g: &GraphRef) -> String {
    let v: Vec<String> = g
        .nodes()
        .iter()
        .map(|n| format!("{}@{}/{}x{}", n.id(), p(n.tl().as_ref()), f(n.w()), f(n.h())))
        .collect();
    v.join(" ")
}

fn load(ctx: &Ctx, path: &str) -> GraphRef {
    let data = std::fs::read(path).unwrap();
    let g = NewGraph();
    graphjson::Unmarshal(ctx, &data, Some(&g)).unwrap();
    g
}

fn report(ctx: &Ctx, path: &str, w: &mut String) {
    let g = load(ctx, path);
    writeln!(w, "== fixture {}", path).unwrap();
    let placed = g.nodes().iter().all(|n| n.has_tl());
    let guard = limits::NewWorkGuard(ctx, "parity", limits::MaxEngineWorkUnits).unwrap();
    if placed {
        let (tl, br) = g.BoundingBox();
        writeln!(w, "graph.BoundingBox {} {}", p(tl.as_ref()), p(br.as_ref())).unwrap();
        writeln!(w, "graph.Area {}", f(g.Area())).unwrap();
        let (ntl, nbr) = g.nodes().BoundingBox();
        writeln!(w, "nodes.BoundingBox {} {}", p(ntl.as_ref()), p(nbr.as_ref())).unwrap();
        let (ftl, fbr) = g.nodes().FixedBoundingBox();
        writeln!(w, "nodes.FixedBoundingBox {} {}", p(ftl.as_ref()), p(fbr.as_ref())).unwrap();
        writeln!(w, "nodes.Center {}", p(Some(&g.nodes().Center()))).unwrap();
        let r = graphbounds::FixedBoundingBox(&g.nodes(), &guard);
        match &r {
            Ok((a, b)) => writeln!(w, "graphbounds.FixedBoundingBox {} {} ok", p(Some(a)), p(Some(b))).unwrap(),
            Err(_) => writeln!(w, "graphbounds.FixedBoundingBox nil nil {}", errs(&r)).unwrap(),
        }
    }
    writeln!(w, "ClusterOrder {}", ids(&g.ClusterOrder())).unwrap();
    writeln!(w, "SequenceOrder {}", ids(&g.SequenceOrder())).unwrap();
    writeln!(w, "TreeOrder {}", optids(&g.TreeOrder())).unwrap();
    writeln!(w, "HubOrder {}", ids(&g.HubOrder())).unwrap();
    writeln!(w, "ContainerRDFSOrder {}", ids(&g.ContainerRDFSOrderUnbounded(None))).unwrap();
    writeln!(w, "ClusterRDFSOrder {}", ids(&g.ClusterRDFSOrder())).unwrap();
    writeln!(w, "AllDescendantNodes {} {}", ids(&g.AllDescendantNodes(None, true)), ids(&g.AllDescendantNodes(None, false))).unwrap();
    writeln!(w, "HasFixedNode {}", g.HasFixedNode()).unwrap();

    let gnodes = g.nodes();
    for n in &gnodes {
        let pre = format!("node {}", n.id());
        writeln!(
            w,
            "{} DebugID {} Level {} ContainerLevel {} Owning {} IsContainer {} Leaky {} ShapeType {}",
            pre,
            n.DebugID(),
            n.Level(),
            n.ContainerLevel(),
            id(n.OwningContainer().as_ref()),
            n.IsContainer(),
            n.HasLeakyEdge(),
            n.ShapeType()
        )
        .unwrap();
        if !n.has_tl() {
            continue;
        }
        let (btl, bbr) = n.BoundingBox(Some(&gnodes));
        let (btl2, bbr2) = n.BoundingBox(None);
        writeln!(w, "{} BoundingBox {} {} {} {}", pre, p(Some(&btl)), p(Some(&bbr)), p(Some(&btl2)), p(Some(&bbr2))).unwrap();
        let r = graphbounds::NodeBoundingBox(Some(n), Some(&gnodes), &guard);
        match &r {
            Ok((a, b)) => writeln!(w, "{} graphbounds.NodeBoundingBox {} {} ok", pre, p(Some(a)), p(Some(b))).unwrap(),
            Err(_) => writeln!(w, "{} graphbounds.NodeBoundingBox nil nil {}", pre, errs(&r)).unwrap(),
        }
        writeln!(w, "{} Center {} Area {}", pre, p(Some(&n.Center())), f(n.Area())).unwrap();
        let ports: Vec<String> = n.Ports().into_iter().map(pv).collect();
        writeln!(w, "{} Ports {}", pre, ports.join(" ")).unwrap();
        let cps: Vec<String> = n.CenterPorts().into_iter().map(pv).collect();
        writeln!(w, "{} CenterPorts {}", pre, cps.join(" ")).unwrap();
        for o in [geo::Top, geo::Right, geo::Bottom, geo::Left] {
            let ps: Vec<String> = n.PortsByOrientation(o).into_iter().map(|pt| format!("{}:{}", pv(pt), n.PortOrientation(&pt).0)).collect();
            writeln!(w, "{} PortsByOrientation {} {}", pre, o.0, ps.join(" ")).unwrap();
        }
        let ib = n.InnerBox();
        writeln!(w, "{} InnerBox {} {} {}", pre, p(ib.TopLeft.as_ref()), f(ib.Width), f(ib.Height)).unwrap();
        for pos in [label::InsideMiddleCenter, label::OutsideTopCenter, label::InsideTopLeft, label::OutsideBottomRight] {
            writeln!(w, "{} IconSize {} {} LabelTopLeft {}", pre, pos.0, f(n.IconSize(pos)), p(Some(&n.LabelTopLeft(pos, 30.0, 12.0)))).unwrap();
        }
        let ip = n.InsidePlacement(40.0, 30.0, UniformSpacing(10.0));
        writeln!(w, "{} InsidePlacement {}", pre, pv(ip)).unwrap();
        if n.IsContainer() {
            let sp = g.ContainerPadding(Some(n), true);
            let sp2 = g.ContainerPadding(Some(n), false);
            writeln!(
                w,
                "{} ContainerPadding {} {} {} {} {} {} {} {}",
                pre,
                f(sp.Top()),
                f(sp.Right()),
                f(sp.Bottom()),
                f(sp.Left()),
                f(sp2.Top()),
                f(sp2.Right()),
                f(sp2.Bottom()),
                f(sp2.Left())
            )
            .unwrap();
        }
        writeln!(w, "{} PointPastFixedOrigin {} {}", pre, n.PointPastFixedOrigin(n.x(), n.y(), true), n.PointPastFixedOrigin(1.0, 1.0, false)).unwrap();
        let c = *n.Center().b();
        let mods: Vec<String> = [c, geo::Point { X: n.x() - 25.0, Y: n.y() - 25.0 }]
            .iter()
            .map(|mp| format!("{} {} {}", n.ContainsPoint(mp, 5.0), n.IsPointNear(mp), n.ContainsPointOnBox(mp)))
            .collect();
        writeln!(w, "{} Points {}", pre, mods.join(" ")).unwrap();
    }
    let lim = gnodes.len().min(12);
    for i in 0..lim {
        for j in 0..lim {
            let (a, b) = (&gnodes[i], &gnodes[j]);
            if i == j || !a.has_tl() || !b.has_tl() {
                continue;
            }
            let (ox, oy) = a.OrthogonalDistanceTo(b);
            writeln!(
                w,
                "pair {} {} DeltaTo {} Overlap {} {} Dist {} {} Orient {} Adj {} {} Covers {} Surrounds {} Ortho {} {} Desc {} NSA {} Along {} {} Vis {} {} Spill {} Conn {}",
                a.id(),
                b.id(),
                a.DeltaTo(b, a.tl().as_ref()),
                a.DoesOverlap(b),
                a.DoesOverlapExact(b),
                f(a.DistanceTo(b, true)),
                f(a.DistanceTo(b, false)),
                a.Orientation(b).0,
                a.IsAdjacentTo(b, true),
                a.IsAdjacentTo(b, false),
                a.Covers(b),
                a.Surrounds(b, 0.0),
                f(ox),
                f(oy),
                a.IsDescendantOf(Some(b)),
                id(a.NearestSharedAncestor(b).as_ref()),
                a.OverlapsAlongDimension(b, true, true),
                a.OverlapsAlongDimension(b, false, false),
                a.VisibilityGraphCandidate(true, true, true, b, 10.0),
                a.VisibilityGraphCandidate(false, false, true, b, 10.0),
                a.SpillsOutOf(b),
                a.ConnectionTo(b).is_some()
            )
            .unwrap();
        }
    }
    let mut segments: Vec<CrossingSegment> = Vec::new();
    for e in g.edges() {
        let pre = format!("edge {}", e.id());
        let (etl, ebr) = e.BoundingBoxValues();
        writeln!(
            w,
            "{} DebugID {} BBox {} {} Length {} Segs {} Directed {} {} {} Loop {} Tree {}",
            pre,
            e.DebugID(),
            pv(etl),
            pv(ebr),
            f(e.Length()),
            e.NumSegments(),
            e.IsDirected(),
            e.IsBidirectional(),
            e.IsUndirected(),
            e.IsLoop(),
            e.IsTreeEdge()
        )
        .unwrap();
        if e.from().has_tl() && e.to().has_tl() {
            writeln!(w, "{} AxisAligned {} Euclid {}", pre, e.IsAxisAligned(), f(e.EuclideanDistance())).unwrap();
            let (fp, tp, hf, ht, o) = e.FacingTablePortValues(None, None);
            writeln!(w, "{} Facing {} {} {} {} {}", pre, pv(fp), pv(tp), hf, ht, o.0).unwrap();
        }
        let pts = e.points();
        if pts.len() >= 2 {
            writeln!(w, "{} Straight {} OverlappingEnd {} Intersects {}", pre, e.IsStraight(), e.HasOverlappingEnd(), gnodes.IntersectsNode(&e)).unwrap();
            for i in 0..pts.len() - 1 {
                segments.push(CrossingSegment { Start: *pts[i].b(), End: *pts[i + 1].b() });
            }
            if let Some(l) = e.b().Label.clone() {
                let l = l.b().clone();
                writeln!(w, "{} LabelTopLeft {}", pre, p(e.LabelTopLeft(l.Position, l.Width, l.Height).as_ref())).unwrap();
            }
        }
    }
    writeln!(w, "CountSegmentCrossings {}", CountSegmentCrossings(&segments)).unwrap();

    let c = Clone(ctx, Some(&g)).unwrap();
    writeln!(w, "costs {} {} {}", f(c.CrossingCost()), f(c.TurnCost()), f(c.NonCenterPortCost())).unwrap();
    c.ComputeCellSize();
    writeln!(w, "CellSize {}", f(c.b().CellSize)).unwrap();

    for opt in [
        SplitOptions::default(),
        SplitOptions { IncludeContainers: true, ..Default::default() },
        SplitOptions { IncludeContainers: true, IncludeNears: true, TraverseTrees: true },
    ] {
        let c = Clone(ctx, Some(&g)).unwrap();
        let r = c.SplitSubgraphs(ctx, opt);
        let subs = r.as_ref().map(|s| s.clone()).unwrap_or_default();
        writeln!(w, "split {} {} {} {} {}", opt.IncludeContainers, opt.IncludeNears, opt.TraverseTrees, errs(&r), subs.len()).unwrap();
        for s in &subs {
            let eids: Vec<String> = s.edges().iter().map(|e| e.id().to_string()).collect();
            let owned = s.nodes().iter().filter(|n| n.graph().as_ref() == Some(s)).count();
            writeln!(w, "  sub {} edges {} owned {}", ids(&s.nodes()), eids.join(" "), owned).unwrap();
        }
    }

    if !gnodes.is_empty() && gnodes[0].has_tl() {
        writeln!(w, "ConnectedNodes {} {}", sortedIDs(&gnodes[0].ConnectedNodes(&[], &g)), sortedIDs(&gnodes[0].ConnectedNodeSet(&[], &g))).unwrap();
    }

    for opts in [TransactionOptions::default(), TransactionOptions { AffectContainers: true, ..Default::default() }] {
        let c = Clone(ctx, Some(&g)).unwrap();
        if c.nodes().is_empty() {
            break;
        }
        let txn = match c.NewRequestTransaction(ctx, opts) {
            Ok(t) => t,
            Err(_) => {
                writeln!(w, "txn new error").unwrap();
                continue;
            }
        };
        let before = positions(&c);
        let cn = c.nodes();
        let lim = cn.len().min(6);
        for i in 0..lim {
            let n = cn[i].clone();
            if !n.has_tl() {
                continue;
            }
            for d in [(7.0, 3.0), (-200.0, 0.0), (0.0, 61.0)] {
                txn.Clear();
                let nn = n.clone();
                txn.AddOp(move || {
                    nn.MoveWithChildren(d.0, d.1);
                    Ok(())
                });
                let r = txn.Commit(ctx);
                writeln!(w, "txn {} {} {} {} {} {}", opts.AffectContainers, n.id(), f(d.0), f(d.1), errs(&r), positions(&c)).unwrap();
                if r.is_ok() {
                    txn.Rollback();
                    writeln!(w, "  rollback restored {}", positions(&c) == before).unwrap();
                }
            }
        }
    }

    {
        let c = Clone(ctx, Some(&g)).unwrap();
        let orig = graphjson::Marshal(ctx, Some(&c)).unwrap();
        let state = NewGraphStateSnapshot(GraphStateSnapshotOptions { CaptureTopology: true, CaptureEdgeRoutes: true });
        let guard = limits::NewWorkGuard(ctx, "parity", limits::MaxTransactionWorkUnits).unwrap();
        let r = state.UpdateWithWorkGuard(&c, &guard);
        writeln!(w, "snapshot {}", errs(&r)).unwrap();
        if r.is_ok() && c.nodes().len() > 1 {
            if placed {
                c.nodes()[0].MoveWithChildren(13.0, 17.0);
            }
            if !c.edges().is_empty() {
                c.Disconnect(&c.edges()[0]);
            }
            c.RemoveNode(&c.nodes()[1]);
            RestoreGraphState(&c, &state);
            let after = graphjson::Marshal(ctx, Some(&c)).unwrap();
            writeln!(w, "snapshot restored {}", orig == after).unwrap();
        }
    }
    if placed {
        let c = Clone(ctx, Some(&g)).unwrap();
        for n in c.ContainerRDFSOrderUnbounded(None) {
            if !n.has_tl() {
                continue;
            }
            n.WrapChildren();
            writeln!(w, "wrap {} {} {} {}", n.id(), p(n.tl().as_ref()), f(n.w()), f(n.h())).unwrap();
        }
        c.SyncNestedGeometry();
        c.ComputeNodeSpacing();
        writeln!(w, "synced {}", positions(&c)).unwrap();
    }
}

#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn graph_parity_with_go() {
    let goref = std::env::var_os("ORTHOFLOW_GOREF")
        .expect("needs Go reference data: set ORTHOFLOW_GOREF to the output of scripts/gen-goref.sh");
    let root = &std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).join(goref);
    let files = std::fs::read_to_string(root.join("graph-parity/files.txt")).unwrap();
    let expected = std::fs::read_to_string(root.join("graph-parity/expected.txt")).unwrap();
    let ctx = gocompat::context::Background();
    let mut out = String::new();
    std::env::set_current_dir(root).unwrap();
    for line in files.lines() {
        if line.trim().is_empty() {
            continue;
        }
        report(&ctx, line, &mut out);
    }
    if let Ok(dump) = std::env::var("ORTHOFLOW_DUMP") {
        std::fs::write(std::path::Path::new(&dump).join("graph-parity-actual.txt"), &out).unwrap();
    }
    let el: Vec<&str> = expected.lines().collect();
    let al: Vec<&str> = out.lines().collect();
    let mut fixture = "";
    let mut diffs = 0;
    for i in 0..el.len().max(al.len()) {
        let (e, a) = (el.get(i).copied().unwrap_or("<eof>"), al.get(i).copied().unwrap_or("<eof>"));
        if e.starts_with("== fixture") {
            fixture = e;
        }
        if e != a {
            diffs += 1;
            if diffs <= 10 {
                eprintln!("{}\n line {}:\n  go:   {}\n  rust: {}", fixture, i + 1, &e[..e.len().min(300)], &a[..a.len().min(300)]);
            }
        }
    }
    assert_eq!(diffs, 0, "{} differing lines (expected {} lines, got {})", diffs, el.len(), al.len());
}
