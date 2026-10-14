// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/tree_routes.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/tree_routes.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::math;
use gocompat::{GoResult, Map, Set};
use layoutgraph::{Arrowhead, NodeRef, TreeRef};

#[derive(Clone)]
pub struct TreeEdgePath {
    pub SourcePortNode: OVGNodeRef,
    pub TargetPortNode: OVGNodeRef,
    /// Go `*geo.Point` (fresh, never aliased).
    pub SourceMidpoint: geo::Point,
    pub TargetMidpoint: geo::Point,
    pub SourceOrientationToTarget: geo::Orientation,
}

pub(crate) fn isSentinelEdgeSource(tree: Option<&TreeRef>) -> bool {
    let Some(tree) = tree else { return false };
    let t = tree.b();
    match &t.SentinelEdge {
        Some(e) => e.from() == t.Node,
        None => false,
    }
}

// get the S-shaped path information for the tree node's sentinelEdge
pub(crate) fn treeEdgePath(
    t: &TreeRef,
    portNodes: &Map<NodeRef, Vec<OVGNodeRef>>,
    portArrowheads: &Map<OVGNodeRef, Set<Arrowhead>>,
) -> TreeEdgePath {
    // Without a guard nothing can fail.
    treeEdgePathForBuild(t, portNodes, portArrowheads, None).expect("unguarded tree edge path")
}

pub(crate) fn treeEdgePathForBuild(
    t: &TreeRef,
    portNodes: &Map<NodeRef, Vec<OVGNodeRef>>,
    portArrowheads: &Map<OVGNodeRef, Set<Arrowhead>>,
    mut guard: Option<&mut ovgBuildGuard>,
) -> GoResult<TreeEdgePath> {
    macro_rules! step {
        () => {
            if let Some(g) = guard.as_deref_mut() {
                g.step()?;
            }
        };
    }
    step!();
    let (child, orientation, sentinelEdge) = {
        let tb = t.b();
        (tb.Node.clone(), tb.Orientation, tb.SentinelEdge.clone().expect("nil pointer dereference: SentinelEdge"))
    };
    let parent = t.SentinelNode();

    let parentShape = parent.shape();
    let parentPortIndex = parentShape.CenterPortIndex(orientation);
    let childPortIndex = child.shape().CenterPortIndex(orientation.GetOpposite());
    let parentPorts = portNodes.get(&parent).cloned().unwrap_or_default();
    let childPorts = portNodes.get(&child).cloned().unwrap_or_default();
    let mut parentPortNode = parentPorts[parentPortIndex as usize].clone();
    let mut childPortNode = childPorts[childPortIndex as usize].clone();

    if let Some(arrowheads) = portArrowheads.get(&parentPortNode) {
        // check the arrowhead on the parent
        let mut arrowhead = sentinelEdge.b().TargetArrowhead.clone();
        if !isSentinelEdgeSource(Some(t)) {
            arrowhead = sentinelEdge.b().SourceArrowhead.clone();
        }
        if !arrowheads.is_empty() && !arrowheads.contains(&arrowhead) {
            let mut closestPort: Option<OVGNodeRef> = None;
            let mut closestDistance = math::Inf(1);
            // try to find a different port since we have a mismatched arrowhead
            for portIndex in parentShape.PortIndices(orientation) {
                step!();
                if portIndex == parentPortIndex {
                    continue;
                }
                let mut usePort = false;
                let port = parentPorts[portIndex as usize].clone();
                match portArrowheads.get(&port) {
                    None => usePort = true,
                    Some(set) => {
                        if set.contains(&arrowhead) {
                            usePort = true;
                        }
                    }
                }
                if usePort {
                    let (pp, cp) = (port.P(), childPortNode.P());
                    let distance = geo::EuclideanDistance(pp.X, pp.Y, cp.X, cp.Y);
                    if distance < closestDistance {
                        closestPort = Some(port);
                        closestDistance = distance;
                    }
                }
            }
            if let Some(closestPort) = closestPort {
                parentPortNode = closestPort;
            }
        }
    }

    childPortNode = alignedPortNodeForBuild(t, &parentPortNode, &childPortNode, guard.as_deref_mut())?;
    // alignedPortNodeForBuild may synthesize a port so that a nearly aligned tree
    // edge stays straight. Resolve it to the canonical child port here.
    for portNode in &childPorts {
        step!();
        if nonNilEquals(&portNode.P(), &childPortNode.P()) {
            childPortNode = portNode.clone();
            break;
        }
    }

    let (parentMidpoint, childMidpoint) = treeEdgeMidpoints(&parent, orientation, &parentPortNode.P(), &childPortNode.P());

    // if we are the target, swap these around
    if isSentinelEdgeSource(Some(t)) {
        Ok(TreeEdgePath {
            SourcePortNode: childPortNode,
            TargetPortNode: parentPortNode,
            SourceMidpoint: childMidpoint,
            TargetMidpoint: parentMidpoint,
            SourceOrientationToTarget: orientation,
        })
    } else {
        Ok(TreeEdgePath {
            SourcePortNode: parentPortNode,
            TargetPortNode: childPortNode,
            SourceMidpoint: parentMidpoint,
            TargetMidpoint: childMidpoint,
            SourceOrientationToTarget: orientation.GetOpposite(),
        })
    }
}

pub(crate) fn alignedPortNodeForBuild(
    tree: &TreeRef,
    parentPort: &OVGNodeRef,
    childPort: &OVGNodeRef,
    guard: Option<&mut ovgBuildGuard>,
) -> GoResult<OVGNodeRef> {
    let isWithinThreshold = |a: f64, b: f64, t: f64| (a - b).abs() <= t;
    let parent = tree.SentinelNode();
    let (child, orientation) = {
        let t = tree.b();
        (t.Node.clone(), t.Orientation)
    };

    let endOfNodeBuffer = 5.0;
    let (pp, cp) = (parentPort.P(), childPort.P());
    let (parentCenter, childCenter) = (parent.Center().get(), child.Center().get());
    let ct = child.tlv();

    // in some cases, parent.center = (100.5, 100) and child.center = (100, 150)
    // however, this is just a rounding error and then we compare equality by upto 1 in difference
    let nodesAreVerticallyAligned = isWithinThreshold(parentCenter.X, childCenter.X, treeChildAlignmentTolerance);
    let portsAreVerticallyMisaligned = pp.X != cp.X;
    if orientation.IsVertical() && nodesAreVerticallyAligned && portsAreVerticallyMisaligned {
        if ct.X + endOfNodeBuffer < pp.X && pp.X < ct.X + child.w() - endOfNodeBuffer {
            let p = geo::Point { X: pp.X, Y: cp.Y };
            return match guard {
                Some(guard) => guard.newDerivedNode(p),
                None => Ok(NewOVGNode(p)),
            };
        }
    }

    let nodesAreHorizontallyAligned = isWithinThreshold(parentCenter.Y, childCenter.Y, treeChildAlignmentTolerance);
    let portsAreHorizontallyMisaligned = pp.Y != cp.Y;
    if orientation.IsHorizontal() && nodesAreHorizontallyAligned && portsAreHorizontallyMisaligned {
        if ct.Y + endOfNodeBuffer < pp.Y && pp.Y < ct.Y + child.h() - endOfNodeBuffer {
            let p = geo::Point { X: cp.X, Y: pp.Y };
            return match guard {
                Some(guard) => guard.newDerivedNode(p),
                None => Ok(NewOVGNode(p)),
            };
        }
    }

    Ok(childPort.clone())
}

// treeEdgeMidpoints computes the two bends in a tree edge's S-shaped route.
// The bend distance is relative to the parent, even when the child is farther
// away to accommodate an edge label.
pub(crate) fn treeEdgeMidpoints(
    parent: &NodeRef,
    treeOrientation: geo::Orientation,
    parentPortPosition: &geo::Point,
    childPortPosition: &geo::Point,
) -> (geo::Point, geo::Point) {
    let mut parentMidpoint = geo::Point::default();
    let mut childMidpoint = geo::Point::default();
    let half = layoutgraph::TreeParentSpacing / 2.0;
    let pt = parent.tlv();
    if treeOrientation == geo::Left {
        parentMidpoint.Y = parentPortPosition.Y;
        childMidpoint.Y = childPortPosition.Y;
        let centerX = pt.X - half;
        parentMidpoint.X = centerX;
        childMidpoint.X = centerX;
    } else if treeOrientation == geo::Right {
        parentMidpoint.Y = parentPortPosition.Y;
        childMidpoint.Y = childPortPosition.Y;
        let centerX = pt.X + parent.w() + half;
        parentMidpoint.X = centerX;
        childMidpoint.X = centerX;
    } else if treeOrientation == geo::Top {
        parentMidpoint.X = parentPortPosition.X;
        childMidpoint.X = childPortPosition.X;
        let centerY = pt.Y - half;
        parentMidpoint.Y = centerY;
        childMidpoint.Y = centerY;
    } else if treeOrientation == geo::Bottom {
        parentMidpoint.X = parentPortPosition.X;
        childMidpoint.X = childPortPosition.X;
        let centerY = pt.Y + parent.h() + half;
        parentMidpoint.Y = centerY;
        childMidpoint.Y = centerY;
    }
    (parentMidpoint, childMidpoint)
}
