// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/labeling/placement.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;
use label::Position;

const LABEL_PADDING: f64 = label::PADDING as f64;

/// SharedSegmentClearance reserves space around shared route segments where
/// an edge label would otherwise be ambiguous.
pub const SharedSegmentClearance: f64 = 2.5 * (label::PADDING as f64);
pub(crate) const placementSearchInterval: f64 = 0.025;

/// Go `&layoutgraph.Node{Box: box, D2ID: d2id, Graph: g}` + SetShape(SQUARE).
pub(crate) fn fakeNode(bx: geo::Box, d2id: Option<String>, g: &GraphRef) -> NodeRef {
    let mut node = layoutgraph::Node::default();
    node.Box = geo::BoxRef::new(bx);
    node.D2ID = d2id;
    node.Graph = Some(g.clone());
    let n = NodeRef::new(node);
    n.SetShape(shape::SQUARE_TYPE);
    n
}

fn boxOf(tl: Option<geo::PointRef>, w: f64, h: f64) -> geo::Box {
    geo::Box { TopLeft: tl, Width: w, Height: h }
}

/// Place chooses positions for every movable node label, icon, and routed edge
/// label in graph.
pub fn Place(ctx: &Ctx, graph: &GraphRef) -> GoResult<()> {
    place(ctx, graph, maxLabelPlacementWorkUnits)
}

pub(crate) fn place(ctx: &Ctx, g: &GraphRef, workLimit: i64) -> GoResult<()> {
    let location = "PlaceLabels";
    layoutgraph::ValidatePositionedGraphSelection(ctx, location, Some(g), &[])?;
    let snapshot = captureLabelPlacement(Some(g), &[]);
    let r = placeInner(ctx, g, workLimit, location);
    if r.is_err() {
        snapshot.restore();
    }
    r
}

fn arrowheadFakeNodes(g: &GraphRef, e: &EdgeRef, guard: &mut labelPlacementWorkGuard, placedFakeNodes: &mut Vec<NodeRef>) -> GoResult<()> {
    if let Some(lbl) = e.b().SourceArrowheadLabel.clone() {
        let pal = positionedArrowheadLabel(e, false, guard)?.expect("nil arrowhead label");
        let text = lbl.b().Text.clone();
        placedFakeNodes.push(fakeNode(pal.Box, Some(text), g));
    }
    if let Some(lbl) = e.b().TargetArrowheadLabel.clone() {
        let pal = positionedArrowheadLabel(e, true, guard)?.expect("nil arrowhead label");
        let text = lbl.b().Text.clone();
        placedFakeNodes.push(fakeNode(pal.Box, Some(text), g));
    }
    Ok(())
}

fn edgeLabelFakeNode(g: &GraphRef, e: &EdgeRef, guard: &mut labelPlacementWorkGuard) -> GoResult<NodeRef> {
    let lbl = e.b().Label.clone().unwrap();
    let (pos, w, h, text) = {
        let l = lbl.b();
        (l.Position, l.Width, l.Height, l.Text.clone())
    };
    let topLeft = edgeLabelTopLeft(e, pos, w, h, guard)?;
    Ok(fakeNode(boxOf(topLeft, w, h), Some(text), g))
}

fn iconFakeNode(g: &GraphRef, node: &NodeRef, position: Position) -> NodeRef {
    let iconSize = node.IconSize(position);
    fakeNode(boxOf(Some(node.LabelTopLeft(position, iconSize, iconSize)), iconSize, iconSize), None, g)
}

fn sharedSegmentFakeNodesFor(g: &GraphRef, edges: &[EdgeRef], guard: &mut labelPlacementWorkGuard) -> GoResult<Vec<NodeRef>> {
    let sharedSegments = {
        let mut step = || guard.step();
        findSharedSegmentsChecked(edges, Some(&mut step))?
    };
    let mut sharedSegmentFakeNodes = Vec::with_capacity(sharedSegments.len());
    for (i, seg) in sharedSegments.iter().enumerate() {
        guard.step()?;
        let (start, end) = *seg;
        let mut tl = start;
        let mut width = end.X - start.X;
        let mut height = end.Y - start.Y;
        if end.X == start.X {
            // vertical
            width = 2.0 * SharedSegmentClearance;
            tl.X -= SharedSegmentClearance;
        } else {
            // horizontal
            height = 2.0 * SharedSegmentClearance;
            tl.Y -= SharedSegmentClearance;
        }
        sharedSegmentFakeNodes.push(fakeNode(
            boxOf(Some(geo::PointRef::new(tl)), width, height),
            Some(format!("fake_shared_segment_{}", i)),
            g,
        ));
    }
    Ok(sharedSegmentFakeNodes)
}

fn placeInner(ctx: &Ctx, g: &GraphRef, workLimit: i64, location: &str) -> GoResult<()> {
    let mut guard = newLabelPlacementWorkGuard(ctx, location, workLimit)?;
    let guard = &mut guard;

    let mut placedFakeNodes: Vec<NodeRef> = Vec::new();
    let (gnodes, gedges, containers) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.clone(), gb.Containers.clone())
    };

    // add all arrowhead labels to placedFakeNodes
    for e in &gedges {
        guard.step()?;
        arrowheadFakeNodes(g, e, guard, &mut placedFakeNodes)?;
    }
    for e in &gedges {
        guard.step()?;
        let lbl = e.b().Label.clone();
        if let Some(lbl) = lbl {
            if e.IsLoop() || lbl.PositionFixed() {
                placedFakeNodes.push(edgeLabelFakeNode(g, e, guard)?);
            }
        }
    }

    // reposition node labels if label is on the same side as a used port or if there is an edge that passes nearby
    for node in &gnodes {
        guard.step()?;
        let nodeContainer = node.containerField();
        let siblings = containers.at(&nodeContainer);
        let children = containers.at(&Some(node.clone()));
        let mut siblingsCapacity = siblings.len() + children.len();
        if siblingsCapacity > 0 {
            siblingsCapacity -= 1;
        }
        let mut siblingsAndChildren: Vec<NodeRef> = Vec::with_capacity(siblingsCapacity);
        for n in &siblings {
            guard.step()?;
            if n != node {
                siblingsAndChildren.push(n.clone());
            }
        }
        if node.IsContainer() {
            for child in &children {
                guard.step()?;
                siblingsAndChildren.push(child.clone());
            }
        }

        let ancestors = collectLabelPlacementAncestors(node, guard)?;

        let mut bestIconPosition = label::Unset;
        let mut bestFakeNode: Option<NodeRef> = None;
        let mut bestScore = math::Inf(1);
        if let Some(icon) = node.icon() {
            if !node.IsImage() {
                if !icon.PositionFixed() {
                    let iconPositions = labelPositionPreferences(node);
                    for iconPosition in iconPositions {
                        guard.step()?;
                        let fakeIconNode = iconFakeNode(g, node, iconPosition);

                        // GO-ALIAS: `nodeset := siblingsAndChildren; nodeset = append(nodeset, x)`
                        // may write into siblingsAndChildren's spare capacity, but
                        // siblingsAndChildren's own length is unchanged and later appends
                        // overwrite that slot, so a copy is equivalent.
                        let mut nodeset = siblingsAndChildren.clone();

                        if let Some(lbl) = node.label() {
                            if lbl.PositionFixed() {
                                let (pos, w, h, text) = {
                                    let l = lbl.b();
                                    (l.Position, l.Width, l.Height, l.Text.clone())
                                };
                                let fixedLabelNode =
                                    fakeNode(boxOf(Some(node.LabelTopLeft(pos, w, h)), w, h), Some(text), g);
                                nodeset.push(fixedLabelNode);
                            }
                        }

                        let nodeOverlaps = nodeOverlapCount(&fakeIconNode, &nodeset, 0.0, guard)?;
                        let edgeOverlaps = edgeOverlapCount(&fakeIconNode, &gedges, 0.0, guard)?;
                        let labelOverlaps = nodeOverlapCount(&fakeIconNode, &placedFakeNodes, 0.0, guard)?;
                        let score = scoreNodeLabelOverlaps(nodeOverlaps, edgeOverlaps, labelOverlaps);
                        if score < bestScore {
                            bestIconPosition = iconPosition;
                            bestFakeNode = Some(fakeIconNode);
                            bestScore = score;
                            if score == 0.0 {
                                break;
                            }
                        }
                    }
                } else {
                    let ipos = icon.b().Position;
                    bestFakeNode = Some(iconFakeNode(g, node, ipos));
                    bestIconPosition = ipos;
                }
                guard.step()?;
                placedFakeNodes.push(bestFakeNode.clone().expect("nil icon fake node"));
                icon.bm().Position = bestIconPosition;
            }
        }

        let Some(nodeLabel) = node.label() else { continue };
        if nodeLabel.PositionFixed() {
            continue;
        }

        if let Some(icon) = node.icon() {
            if !node.IsImage() {
                let ipos = icon.b().Position;
                siblingsAndChildren.push(iconFakeNode(g, node, ipos));
            }
        }

        let (lw, lh, ltext) = {
            let l = nodeLabel.b();
            (l.Width, l.Height, l.Text.clone())
        };

        bestScore = math::Inf(1);
        let mut bestLabelPosition = label::Unset;
        let tranches = labelPositionPreferenceTranches(node);
        for tranch in &tranches {
            guard.step()?;
            let mut tiedBest: Vec<Position> = Vec::new();
            let mut tiedBestFakeLabelNodes: Vec<NodeRef> = Vec::new();
            for &labelPosition in tranch {
                guard.step()?;
                if labelPosition == bestIconPosition {
                    continue;
                }
                let fakeLabelNode = fakeNode(boxOf(Some(node.LabelTopLeft(labelPosition, lw, lh)), lw, lh), Some(ltext.clone()), g);
                if !labelPosition.IsOutside() {
                    fakeLabelNode.PadLabelCandidate(label::PADDING as i64);
                    if !layoutgraph::LabelBoxFits(&node.InnerBox(), &fakeLabelNode.boxValue()) {
                        continue;
                    }
                    fakeLabelNode.PadLabelCandidate(-(label::PADDING as i64));
                }

                let siblingOverlaps = nodeOverlapCount(&fakeLabelNode, &siblingsAndChildren, LABEL_PADDING, guard)?;
                let ancestorOverlaps = partialNodeOverlapCount(&fakeLabelNode, &ancestors, LABEL_PADDING, guard)?;
                let edgeOverlaps = edgeOverlapCount(&fakeLabelNode, &gedges, LABEL_PADDING - 1.0, guard)?;
                let labelOverlaps = nodeOverlapCount(&fakeLabelNode, &placedFakeNodes, LABEL_PADDING, guard)?;
                let score = scoreNodeLabelOverlaps(siblingOverlaps + ancestorOverlaps, edgeOverlaps, labelOverlaps);
                if score < bestScore {
                    bestLabelPosition = labelPosition;
                    bestFakeNode = Some(fakeLabelNode.clone());
                    bestScore = score;
                    if labelPosition.IsOutside() && bestLabelPosition.IsOutside() {
                        tiedBest = vec![labelPosition];
                        tiedBestFakeLabelNodes = vec![fakeLabelNode];
                    } else {
                        tiedBest = vec![];
                        tiedBestFakeLabelNodes = vec![];
                    }
                } else if score == bestScore
                    && compareLabelPositions(node, labelPosition, bestLabelPosition) == 0
                    && bestLabelPosition.IsOutside()
                    && labelPosition.IsOutside()
                {
                    tiedBest.push(labelPosition);
                    tiedBestFakeLabelNodes.push(fakeLabelNode);
                }
            }
            if tiedBest.len() > 1 {
                let mut bestTiebreakScore = math::Inf(1);
                let mut bestTieBreakLabelPosition = label::Unset;
                let mut bestTieBreakFakeNode: Option<NodeRef> = None;
                for (i, &labelPosition) in tiedBest.iter().enumerate() {
                    guard.step()?;
                    let tiebreakFakeLabelNode = fakeNode(
                        boxOf(Some(node.LabelTopLeft(labelPosition, lw * 2.0, lh * 2.0)), lw * 2.0, lh * 2.0),
                        Some(ltext.clone()),
                        g,
                    );
                    if !labelPosition.IsOutside() {
                        tiebreakFakeLabelNode.PadLabelCandidate(label::PADDING as i64);
                        if !layoutgraph::LabelBoxFits(&node.InnerBox(), &tiebreakFakeLabelNode.boxValue()) {
                            continue;
                        }
                        tiebreakFakeLabelNode.PadLabelCandidate(-(label::PADDING as i64));
                    }

                    let siblingOverlaps = nodeOverlapCount(&tiebreakFakeLabelNode, &siblingsAndChildren, LABEL_PADDING, guard)?;
                    let ancestorOverlaps = partialNodeOverlapCount(&tiebreakFakeLabelNode, &ancestors, LABEL_PADDING, guard)?;
                    let edgeOverlaps = edgeOverlapCount(&tiebreakFakeLabelNode, &gedges, LABEL_PADDING - 1.0, guard)?;
                    let labelOverlaps = nodeOverlapCount(&tiebreakFakeLabelNode, &placedFakeNodes, LABEL_PADDING, guard)?;
                    let score = scoreNodeLabelOverlaps(siblingOverlaps + ancestorOverlaps, edgeOverlaps, labelOverlaps);
                    if score < bestTiebreakScore {
                        bestTiebreakScore = score;
                        bestTieBreakLabelPosition = labelPosition;
                        bestTieBreakFakeNode = Some(tiedBestFakeLabelNodes[i].clone());
                    }
                }
                bestLabelPosition = bestTieBreakLabelPosition;
                bestFakeNode = bestTieBreakFakeNode;
            }
        }
        guard.step()?;
        // Go appends a possibly-nil *Node; nil entries are skipped by the overlap
        // kernels only if they never occur. Keep Go's behaviour (nil deref) by expect.
        placedFakeNodes.push(bestFakeNode.clone().expect("nil best label fake node"));

        nodeLabel.bm().Position = bestLabelPosition;
    }

    let sharedSegmentFakeNodes = sharedSegmentFakeNodesFor(g, &gedges, guard)?;

    let mut sortedEdges: Vec<EdgeRef> = Vec::with_capacity(gedges.len());
    for e in &gedges {
        guard.step()?;
        let lbl = e.b().Label.clone();
        match lbl {
            None => continue,
            Some(l) => {
                if e.IsLoop() || l.PositionFixed() {
                    continue;
                }
            }
        }
        sortedEdges.push(e.clone());
    }

    let sortedEdges = sortLabelPlacementEdges(&sortedEdges, guard)?;

    for edge in &sortedEdges {
        guard.step()?;
        let (fakeNode, position, percentage) =
            findBestEdgeLabelPosition(edge, g, &placedFakeNodes, &sharedSegmentFakeNodes, guard)?;
        placedFakeNodes.push(fakeNode.expect("nil edge label fake node"));
        let lbl = edge.b().Label.clone().unwrap();
        lbl.bm().Position = position;
        edge.bm().LabelPercentage = percentage;
    }

    guard.check()
}

/// PlaceNewEdges chooses label positions only for edges, preserving all
/// existing label placements in graph as obstacles.
pub fn PlaceNewEdges(ctx: &Ctx, graph: &GraphRef, edges: &[EdgeRef]) -> GoResult<()> {
    placeNewEdges(ctx, graph, edges, maxLabelPlacementWorkUnits)
}

pub(crate) fn placeNewEdges(ctx: &Ctx, g: &GraphRef, edges: &[EdgeRef], workLimit: i64) -> GoResult<()> {
    let location = "PlaceNewEdgeLabels";
    layoutgraph::ValidatePositionedGraphSelection(ctx, location, Some(g), edges)?;
    let snapshot = captureLabelPlacement(Some(g), edges);
    let r = placeNewEdgesInner(ctx, g, edges, workLimit, location);
    if r.is_err() {
        snapshot.restore();
    }
    r
}

fn placeNewEdgesInner(ctx: &Ctx, g: &GraphRef, edges: &[EdgeRef], workLimit: i64, location: &str) -> GoResult<()> {
    let mut guard = newLabelPlacementWorkGuard(ctx, location, workLimit)?;
    let guard = &mut guard;

    let mut placedFakeNodes: Vec<NodeRef> = Vec::new();
    let (gnodes, gedges) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.clone())
    };

    let mut newEdges: Set<EdgeRef> = Set::default();
    for e in edges {
        guard.step()?;
        newEdges.insert(e.clone());
    }

    // Reserve existing and caller-fixed edge labels, plus all arrowhead labels.
    for e in &gedges {
        guard.step()?;
        arrowheadFakeNodes(g, e, guard, &mut placedFakeNodes)?;

        let Some(lbl) = e.b().Label.clone() else { continue };
        if e.IsLoop() {
            continue;
        }
        if newEdges.contains(e) && !lbl.PositionFixed() {
            continue;
        }
        placedFakeNodes.push(edgeLabelFakeNode(g, e, guard)?);
    }

    for e in &gedges {
        guard.step()?;
        if e.IsLoop() && e.b().Label.is_some() {
            placedFakeNodes.push(edgeLabelFakeNode(g, e, guard)?);
        }
    }

    for node in &gnodes {
        guard.step()?;
        if let Some(icon) = node.icon() {
            if !node.IsImage() {
                let ipos = icon.b().Position;
                placedFakeNodes.push(iconFakeNode(g, node, ipos));
            }
        }

        let Some(lbl) = node.label() else { continue };
        if lbl.PositionFixed() {
            continue;
        }
        let (pos, w, h, text) = {
            let l = lbl.b();
            (l.Position, l.Width, l.Height, l.Text.clone())
        };
        placedFakeNodes.push(fakeNode(boxOf(Some(node.LabelTopLeft(pos, w, h)), w, h), Some(text), g));
    }

    let sharedSegmentFakeNodes = sharedSegmentFakeNodesFor(g, &gedges, guard)?;

    let mut sortedEdges: Vec<EdgeRef> = Vec::with_capacity(edges.len());
    for e in edges {
        guard.step()?;
        let lbl = e.b().Label.clone();
        match lbl {
            None => continue,
            Some(l) => {
                if e.IsLoop() || l.PositionFixed() {
                    continue;
                }
            }
        }
        sortedEdges.push(e.clone());
    }

    let sortedEdges = sortLabelPlacementEdges(&sortedEdges, guard)?;

    for edge in &sortedEdges {
        guard.step()?;
        let (fakeNode, position, percentage) =
            findBestEdgeLabelPosition(edge, g, &placedFakeNodes, &sharedSegmentFakeNodes, guard)?;
        placedFakeNodes.push(fakeNode.expect("nil edge label fake node"));
        let lbl = edge.b().Label.clone().unwrap();
        lbl.bm().Position = position;
        edge.bm().LabelPercentage = percentage;
    }
    guard.check()
}

pub(crate) fn findBestEdgeLabelPosition(
    edge: &EdgeRef,
    g: &GraphRef,
    placedFakeNodes: &[NodeRef],
    sharedSegmentFakeNodes: &[NodeRef],
    guard: &mut labelPlacementWorkGuard,
) -> GoResult<(Option<NodeRef>, Position, f64)> {
    let (gnodes, gedges) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.clone())
    };
    let otherEdges: Vec<EdgeRef> = {
        let mut v = Vec::with_capacity(gedges.len());
        for otherEdge in &gedges {
            guard.step()?;
            if otherEdge == edge {
                continue;
            }
            v.push(otherEdge.clone());
        }
        v
    };

    let edgeLabel = edge.b().Label.clone().unwrap();
    let (from, to) = (edge.from(), edge.to());
    let fromCluster = from.clusterField();
    let toCluster = to.clusterField();

    let mut positions: Vec<Position> = Vec::with_capacity(edgeLabelPreferenceOrder.len() + 3);
    let curPos = edgeLabel.b().Position;
    if curPos.IsUnlocked() {
        if curPos == label::UnlockedMiddle {
            positions.push(label::UnlockedMiddle);
            positions.push(label::UnlockedTop);
            positions.push(label::UnlockedBottom);
        } else {
            positions.push(curPos);
            positions.push(curPos.Mirrored());
            positions.push(label::UnlockedMiddle);
        }
        positions.extend_from_slice(&edgeLabelPreferenceOrder);
    } else if edge.IsClusterEdge() {
        let pathShared = isClusterPathSharedPlacement(edge, guard)?;
        if !pathShared {
            positions.extend_from_slice(&edgeLabelPreferenceOrder);
            positions.push(label::UnlockedTop);
            positions.push(label::UnlockedBottom);
            positions.push(label::UnlockedMiddle);
        } else {
            if let Some(fc) = fromCluster.clone() {
                let (orientation, clusterIndex) = clusterLabelPlacementOrientation(&to, &from, &fc, guard)?;
                let half = (fc.b().Nodes.len() as i64 - 1) as f64 / 2.0;
                let ci = clusterIndex as f64;
                if orientation == geo::Right || orientation == geo::Top {
                    if ci < half {
                        positions.push(label::UnlockedTop);
                    } else if ci > half {
                        positions.push(label::UnlockedBottom);
                    }
                } else if orientation == geo::Left || orientation == geo::Bottom {
                    if ci < half {
                        positions.push(label::UnlockedBottom);
                    } else if ci > half {
                        positions.push(label::UnlockedTop);
                    }
                }
            } else {
                let tc = toCluster.clone().expect("nil pointer dereference: edge.To.Cluster");
                let (orientation, clusterIndex) = clusterLabelPlacementOrientation(&from, &to, &tc, guard)?;
                let half = (tc.b().Nodes.len() as i64 - 1) as f64 / 2.0;
                let ci = clusterIndex as f64;
                if orientation == geo::Left || orientation == geo::Bottom {
                    if ci < half {
                        positions.push(label::UnlockedTop);
                    } else if ci > half {
                        positions.push(label::UnlockedBottom);
                    }
                } else if orientation == geo::Right || orientation == geo::Top {
                    if ci < half {
                        positions.push(label::UnlockedBottom);
                    } else if ci > half {
                        positions.push(label::UnlockedTop);
                    }
                }
            }
            positions.push(label::UnlockedMiddle);
            positions.extend_from_slice(&edgeLabelPreferenceOrder);
        }
    } else {
        positions.extend_from_slice(&edgeLabelPreferenceOrder);
        positions.push(label::UnlockedTop);
        positions.push(label::UnlockedBottom);
        positions.push(label::UnlockedMiddle);
    }

    let mut ancestors: Vec<NodeRef> = Vec::new();
    let mut nonAncestors: Vec<NodeRef> = Vec::new();
    for n in &gnodes {
        guard.step()?;
        let fromDescendant = isLabelPlacementDescendantOf(Some(&from), Some(n), guard)?;
        let toDescendant = isLabelPlacementDescendantOf(Some(&to), Some(n), guard)?;
        if *n != from && *n != to && (fromDescendant || toDescendant) {
            ancestors.push(n.clone());
        } else {
            nonAncestors.push(n.clone());
        }
    }

    let mut bestLabelPosition = label::Unset;
    let mut bestLabelPercentage = 0.0;
    let mut bestLabelFakeNode: Option<NodeRef> = None;
    let mut bestScore = math::Inf(1);

    let (lw, lh, ltext) = {
        let l = edgeLabel.b();
        (l.Width, l.Height, l.Text.clone())
    };

    let mut checkPosition = |labelPosition: Position,
                             guard: &mut labelPlacementWorkGuard,
                             bestScore: &mut f64,
                             bestLabelPosition: &mut Position,
                             bestLabelFakeNode: &mut Option<NodeRef>,
                             bestLabelPercentage: &mut f64|
     -> GoResult<()> {
        guard.step()?;
        let topLeft = edgeLabelTopLeft(edge, labelPosition, lw, lh, guard)?;
        let fakeLabelNode = fakeNode(boxOf(topLeft, lw, lh), Some(ltext.clone()), g);

        let mut edges: Vec<EdgeRef> = if labelPosition.IsOnEdge() { otherEdges.clone() } else { gedges.clone() };

        if let Some(fc) = &fromCluster {
            for otherEdge in &otherEdges {
                guard.step()?;
                if otherEdge.from().clusterField().as_ref() == Some(fc) {
                    edges.push(otherEdge.clone());
                }
            }
        } else if let Some(tc) = &toCluster {
            for otherEdge in &otherEdges {
                guard.step()?;
                if otherEdge.to().clusterField().as_ref() == Some(tc) {
                    edges.push(otherEdge.clone());
                }
            }
        }

        let (ancestorPartialOverlapArea, ancestorCount) = nodeOverlapArea(&fakeLabelNode, &ancestors, LABEL_PADDING, true, guard)?;
        let (nonAncestorOverlapArea, nonAncestorCount) = nodeOverlapArea(&fakeLabelNode, &nonAncestors, LABEL_PADDING, false, guard)?;
        let edgePaddingOverlaps = edgeOverlapCount(&fakeLabelNode, &edges, LABEL_PADDING, guard)?;
        let edgeExactOverlaps = edgeOverlapCount(&fakeLabelNode, &edges, 0.0, guard)?;
        let edgeOverlaps = edgePaddingOverlaps + math::Ceil(edgeExactOverlaps as f64 * 0.5) as i64;
        let almostLabelPaddingOverlaps = nodeOverlapCount(&fakeLabelNode, placedFakeNodes, LABEL_PADDING, guard)?;
        let almostLabelExactOverlaps = nodeOverlapCount(&fakeLabelNode, placedFakeNodes, 0.0, guard)?;
        let almostLabelOverlaps = almostLabelPaddingOverlaps + math::Ceil(almostLabelExactOverlaps as f64 * 0.5) as i64;
        let labelOverlaps = nodeOverlapCount(&fakeLabelNode, placedFakeNodes, 0.0, guard)?;
        let sharedSegmentOverlap = nodeOverlapCount(&fakeLabelNode, sharedSegmentFakeNodes, LABEL_PADDING, guard)?;
        let score = scoreEdgeLabelOverlaps(
            fakeLabelNode.Area(),
            ancestorPartialOverlapArea + nonAncestorOverlapArea,
            ancestorCount + nonAncestorCount,
            edgeOverlaps,
            almostLabelOverlaps,
            labelOverlaps,
            sharedSegmentOverlap,
        );

        if score < *bestScore {
            *bestLabelPosition = labelPosition;
            *bestLabelFakeNode = Some(fakeLabelNode);
            *bestLabelPercentage = edge.b().LabelPercentage;
            *bestScore = score;
        }
        Ok(())
    };

    let mut routeLength = 0.0;
    let mut routeLengthCalculated = false;
    for &labelPosition in &positions {
        guard.step()?;
        if bestScore == 0.0 {
            break;
        }
        if labelPosition.IsUnlocked() {
            if edge.b().LabelPercentage != 0.0 {
                checkPosition(labelPosition, guard, &mut bestScore, &mut bestLabelPosition, &mut bestLabelFakeNode, &mut bestLabelPercentage)?;
            } else {
                if !routeLengthCalculated {
                    routeLength = routeLengthPlacement(edge, guard)?;
                    routeLengthCalculated = true;
                }
                let r = labelPercentageSearchRange(edge, routeLength);
                let mut i = r.start;
                while i < r.end {
                    guard.step()?;
                    edge.bm().LabelPercentage = i;
                    checkPosition(labelPosition, guard, &mut bestScore, &mut bestLabelPosition, &mut bestLabelFakeNode, &mut bestLabelPercentage)?;
                    if bestScore == 0.0 {
                        break;
                    }
                    i += placementSearchInterval;
                }
                edge.bm().LabelPercentage = 0.0;
            }
        } else {
            checkPosition(labelPosition, guard, &mut bestScore, &mut bestLabelPosition, &mut bestLabelFakeNode, &mut bestLabelPercentage)?;
        }
    }

    guard.check()?;
    Ok((bestLabelFakeNode, bestLabelPosition, bestLabelPercentage))
}

pub(crate) fn labelPercentageSearchRange(edge: &EdgeRef, length: f64) -> searchRange {
    let lbl = edge.b().Label.clone().unwrap();
    let (lw, lh) = {
        let l = lbl.b();
        (l.Width, l.Height)
    };
    let halfHeight = lh / 2.0;
    let halfWidth = lw / 2.0;
    let points = edge.points();
    if edge.from().clusterField().is_some() {
        let p1 = *points[0].b();
        let p2 = *points[1].b();
        if p1.X == p2.X && math::Abs(p1.Y - p2.Y) > halfHeight {
            let end = (math::Abs(p1.Y - p2.Y) - halfHeight) / length;
            return searchRange { start: 0.0, end: math::Min(1.0, end) };
        } else if math::Abs(p1.X - p2.X) > halfWidth {
            let end = (math::Abs(p1.X - p2.X) - halfWidth) / length;
            return searchRange { start: 0.0, end: math::Min(1.0, end) };
        }
    } else if edge.to().clusterField().is_some() {
        let p1 = *points[points.len() - 1].b();
        let p2 = *points[points.len() - 2].b();
        if p1.X == p2.X && math::Abs(p1.Y - p2.Y) > halfHeight {
            let start = (length - math::Abs(p1.Y - p2.Y) + halfHeight) / length;
            return searchRange { start: math::Max(0.0, start), end: 1.0 };
        } else if math::Abs(p1.X - p2.X) > halfWidth {
            let start = (length - math::Abs(p1.X - p2.X) + halfWidth) / length;
            return searchRange { start: math::Max(0.0, start), end: 1.0 };
        }
    }
    searchRange { start: 0.0, end: 1.0 }
}

/// findSharedSegmentsChecked finds the longest shared vertical/horizontal segments.
/// Segments are returned as (Start, End) point values.
///
/// Go iterates the per-coordinate maps in random order; the result order only
/// affects fake-node debug IDs and is otherwise unused (counts only).
pub(crate) fn findSharedSegmentsChecked(
    edges: &[EdgeRef],
    mut checkCanceled: Option<&mut dyn FnMut() -> GoResult<()>>,
) -> GoResult<Vec<(geo::Point, geo::Point)>> {
    if let Some(cc) = checkCanceled.as_mut() {
        cc()?;
    }
    // Keep insertion order of coordinates for determinism.
    let mut verticalSegments: Vec<(f64, Vec<(geo::Point, geo::Point)>)> = Vec::new();
    let mut horizontalSegments: Vec<(f64, Vec<(geo::Point, geo::Point)>)> = Vec::new();
    let mut vIndex: Map<u64, usize> = Map::default();
    let mut hIndex: Map<u64, usize> = Map::default();
    // Go float map keys: +0 == -0; NaN keys never match (each NaN is its own entry).
    fn key(v: f64) -> Option<u64> {
        if v.is_nan() {
            None
        } else if v == 0.0 {
            Some(0)
        } else {
            Some(v.to_bits())
        }
    }
    fn push(
        groups: &mut Vec<(f64, Vec<(geo::Point, geo::Point)>)>,
        index: &mut Map<u64, usize>,
        coord: f64,
        seg: (geo::Point, geo::Point),
    ) {
        match key(coord) {
            Some(k) => {
                if let Some(&i) = index.get(&k) {
                    groups[i].1.push(seg);
                } else {
                    index.insert(k, groups.len());
                    groups.push((coord, vec![seg]));
                }
            }
            None => groups.push((coord, vec![seg])),
        }
    }

    for edge in edges {
        if let Some(cc) = checkCanceled.as_mut() {
            cc()?;
        }
        let points = edge.points();
        let n = points.len() as i64;
        let mut i = 0i64;
        while i < n - 1 {
            if let Some(cc) = checkCanceled.as_mut() {
                cc()?;
            }
            let mut start = *points[i as usize].b();
            let mut end = *points[i as usize + 1].b();
            i += 1;
            if start.X == end.X && start.Y == end.Y {
                continue;
            }
            if start.X == end.X {
                if end.Y < start.Y {
                    std::mem::swap(&mut end.Y, &mut start.Y);
                }
                push(&mut verticalSegments, &mut vIndex, start.X, (start, end));
            } else if start.Y == end.Y {
                if end.X < start.X {
                    std::mem::swap(&mut end.X, &mut start.X);
                }
                push(&mut horizontalSegments, &mut hIndex, start.Y, (start, end));
            }
        }
    }

    let mut sharedSegments: Vec<(geo::Point, geo::Point)> =
        Vec::with_capacity(std::cmp::max(verticalSegments.len(), horizontalSegments.len()));

    fn addSharedSegments(
        segmentsByCoord: &mut Vec<(f64, Vec<(geo::Point, geo::Point)>)>,
        isHorizontalSegment: bool,
        checkCanceled: &mut Option<&mut dyn FnMut() -> GoResult<()>>,
        sharedSegments: &mut Vec<(geo::Point, geo::Point)>,
    ) -> GoResult<()> {
        let coordinate = |p: &geo::Point| -> f64 {
            if isHorizontalSegment {
                p.X
            } else {
                p.Y
            }
        };
        let setCoordinate = |p: &mut geo::Point, v: f64| {
            if isHorizontalSegment {
                p.X = v;
            } else {
                p.Y = v;
            }
        };
        for (_, segments) in segmentsByCoord.iter_mut() {
            if let Some(cc) = checkCanceled.as_mut() {
                cc()?;
            }
            if segments.len() == 1 {
                continue;
            }
            sortSharedSegmentGroup(segments, &coordinate, checkCanceled)?;

            let mut prev = segments[0];
            let mut shared: Option<(geo::Point, geo::Point)> = None;
            for i in 1..segments.len() {
                if let Some(cc) = checkCanceled.as_mut() {
                    cc()?;
                }
                let curr = segments[i];
                if coordinate(&prev.1) > coordinate(&curr.0) {
                    if let Some(s) = shared {
                        if coordinate(&curr.0) > coordinate(&s.1) {
                            sharedSegments.push(s);
                            shared = None;
                        }
                    }
                    match shared.as_mut() {
                        None => {
                            let mut s = (curr.0, curr.0);
                            setCoordinate(&mut s.1, math::Min(coordinate(&prev.1), coordinate(&curr.1)));
                            shared = Some(s);
                        }
                        Some(s) => {
                            let minOverlap = math::Min(coordinate(&prev.1), coordinate(&curr.1));
                            let v = math::Max(coordinate(&s.1), minOverlap);
                            setCoordinate(&mut s.1, v);
                        }
                    }
                } else if let Some(s) = shared {
                    sharedSegments.push(s);
                    shared = None;
                }
                if coordinate(&curr.1) > coordinate(&prev.1) {
                    prev = curr;
                }
            }
            if let Some(s) = shared {
                sharedSegments.push(s);
            }
        }
        Ok(())
    }

    addSharedSegments(&mut verticalSegments, false, &mut checkCanceled, &mut sharedSegments)?;
    addSharedSegments(&mut horizontalSegments, true, &mut checkCanceled, &mut sharedSegments)?;

    Ok(sharedSegments)
}

pub(crate) fn scoreEdgeLabelOverlaps(
    labelArea: f64,
    nodeOverlapArea: f64,
    nodeOverlapCount: i64,
    edgeOverlapCount: i64,
    almostLabelOverlapCount: i64,
    labelOverlapCount: i64,
    sharedSegmentOverlapCount: i64,
) -> f64 {
    let mut score = (nodeOverlapArea / labelArea) * 2.0;
    score += labelOverlapCount as f64 * 10.0;
    score += almostLabelOverlapCount as f64;
    score += nodeOverlapCount as f64 * 2.0;
    score += edgeOverlapCount as f64 * 2.0;
    score += sharedSegmentOverlapCount as f64;
    score
}

/// scoreNodeLabelOverlaps ranks a node-label candidate.
pub(crate) fn scoreNodeLabelOverlaps(nodeOverlapCount: i64, edgeOverlapCount: i64, labelOverlapCount: i64) -> f64 {
    (nodeOverlapCount + edgeOverlapCount + 2 * labelOverlapCount) as f64
}

pub(crate) fn isClusterPathSharedPlacement(e: &EdgeRef, guard: &mut labelPlacementWorkGuard) -> GoResult<bool> {
    let mut step = || guard.step();
    isClusterPathSharedChecked(e, Some(&mut step))
}

/// isClusterPathSharedChecked checks if a given cluster edge has a shared path with the other cluster edges.
pub(crate) fn isClusterPathSharedChecked(e: &EdgeRef, mut checkWork: Option<&mut dyn FnMut() -> GoResult<()>>) -> GoResult<bool> {
    if let Some(cw) = checkWork.as_mut() {
        cw()?;
    }
    let (from, to) = (e.from(), e.to());
    let clusterNodes: Vec<NodeRef>;
    let adjacentNode: NodeRef;
    let mut fromCluster = false;
    if let Some(c) = from.clusterField() {
        clusterNodes = c.b().Nodes.clone();
        adjacentNode = to.clone();
        fromCluster = true;
    } else if let Some(c) = to.clusterField() {
        clusterNodes = c.b().Nodes.clone();
        adjacentNode = from.clone();
    } else {
        return Ok(false);
    }

    let pts = e.points();
    let (p1, p2) = if fromCluster {
        (*pts[pts.len() - 1].b(), *pts[pts.len() - 2].b())
    } else {
        (*pts[0].b(), *pts[1].b())
    };
    for n in &clusterNodes {
        if let Some(cw) = checkWork.as_mut() {
            cw()?;
        }
        for ce in n.edges() {
            if let Some(cw) = checkWork.as_mut() {
                cw()?;
            }
            if ce == *e {
                continue;
            }
            if n.Adjacent(&ce) != adjacentNode {
                continue;
            }
            let ops = ce.points();
            let (op1, op2) = if fromCluster {
                (*ops[ops.len() - 1].b(), *ops[ops.len() - 2].b())
            } else {
                (*ops[0].b(), *ops[1].b())
            };
            if p1.X == p2.X && op1.X == op2.X && p1.X == op1.X {
                if math::Max(p1.Y, p2.Y) >= math::Min(op1.Y, op2.Y) && math::Max(op1.Y, op2.Y) >= math::Min(p1.Y, p2.Y) {
                    return Ok(true);
                }
            } else if p1.Y == p2.Y && op1.Y == op2.Y && p1.Y == op1.Y {
                if math::Max(p1.X, p2.X) >= math::Min(op1.X, op2.X) && math::Max(op1.X, op2.X) >= math::Min(p1.X, p2.X) {
                    return Ok(true);
                }
            }
        }
    }

    Ok(false)
}
