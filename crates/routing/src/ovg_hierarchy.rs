// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/routing/ovg_hierarchy.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/ovg_hierarchy.go.
#[allow(unused_imports)]
use crate::*;
use gocompat::{math, GoResult, Map};
use layoutgraph::{GraphRef, HierarchyRef, NodeRef};

pub(crate) fn newOVGForHierarchy(g: &GraphRef, h: &HierarchyRef, guardRef: &ovgBuildGuardRef, guard: &mut ovgBuildGuard) -> GoResult<OVG> {
    guard.check()?;
    // only for SQL Table hierarchies
    // ports must be added before we transpose because transpose can change the node size and then the ports positions
    let levels = h.Levels();
    let levelKeys = levels.keys();
    let mut nodes: Vec<NodeRef> = Vec::with_capacity(levelKeys.len());
    for n in levelKeys {
        guard.step()?;
        nodes.push(n);
    }
    guard.reserveSortWork(nodes.len())?;
    gocompat::sort::SlicesSortFunc(&mut nodes, |a, b| gocompat::sort::cmpCompare(a.b().ID, b.b().ID));
    guard.check()?;

    let mut ovg = newBuildOVG(nodes.clone(), Some(guardRef));
    ovg.addPorts(g, guard)?;

    // if the current direction is Left or Top, we need to mirror for the OVG creation, then mirror back at the very end
    let direction = nodes[0].ContainerDirection();
    let mirror: Option<(bool, bool)> = if direction == geo::Left {
        Some((true, false))
    } else if direction == geo::Top {
        Some((false, true))
    } else {
        None
    };
    if let Some((mx, my)) = mirror {
        guard.reserveHierarchyTransform(&ovg, nodes.len())?;
        applyHierarchyMirror(&mut ovg, &nodes, mx, my);
    }
    let res = (|| -> GoResult<()> {
        if mirror.is_some() {
            guard.check()?;
        }
        // the hierarchy must be Top to Bottom, Left to Right when creating the OVG
        // if `isHorizontalHierarchy`, the hierarchy is transposed to vertical, the OVG is created, and then transposed back
        guard.reserveWork(nodes.len() as u64)?;
        let isHorizontalHierarchy = hierarchy::IsHorizontal(&nodes);
        if isHorizontalHierarchy {
            let n = ovg.NodesInsideBoundingBox.len();
            guard.reserveHierarchyTransform(&ovg, n)?;
            ovg.transpose();
            let r = buildHierarchyLevels(&mut ovg, g, h, guard, true);
            ovg.transpose();
            r
        } else {
            buildHierarchyLevels(&mut ovg, g, h, guard, false)
        }
    })();
    if let Some((mx, my)) = mirror {
        applyHierarchyMirror(&mut ovg, &nodes, mx, my);
    }
    res?;
    Ok(ovg)
}

fn applyHierarchyMirror(ovg: &mut OVG, nodes: &[NodeRef], mx: bool, my: bool) {
    for node in nodes {
        node.Mirror(mx, my);
    }
    ovg.transformHierarchyOVGNodes(
        |node: &OVGNodeRef| {
            let mut n = node.bm();
            if mx {
                n.Point.X = -n.Point.X;
            }
            if my {
                n.Point.Y = -n.Point.Y;
            }
        },
        |direction| mirrorPortDirection(direction, mx, my),
    );
}

fn buildHierarchyLevels(ovg: &mut OVG, g: &GraphRef, h: &HierarchyRef, guard: &mut ovgBuildGuard, transposed: bool) -> GoResult<()> {
    if transposed {
        guard.check()?;
    }
    ovg.addNodesIntersections(g, guard)?;
    ovg.addHierarchyLevelNodes(g, h, guard)?;
    let (tl, br) = guard.ovgBoundingBox(ovg)?;
    ovg.addNewBoundaryLayers(g, tl, br, guard)?;
    ovg.addCornerNodes(g, tl, br, guard)?;
    guard.check()
}

impl OVG {
    pub(crate) fn addHierarchyLevelNodes(&mut self, g: &GraphRef, hierarchy: &HierarchyRef, guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        let mut levelToNodes: Map<i64, Vec<NodeRef>> = Map::default();
        for (n, level) in hierarchy.Levels().entries() {
            guard.step()?;
            levelToNodes.entry(level).or_default().push(n);
        }
        let nlevels = levelToNodes.len() as i64;
        for level in 0..nlevels {
            guard.step()?;
            let len = levelToNodes.get(&level).map_or(0, |v| v.len());
            guard.reserveSortWork(len)?;
            if let Some(v) = levelToNodes.get_mut(&level) {
                let mut keyed: Vec<(f64, NodeRef)> = v.iter().map(|n| (n.x(), n.clone())).collect();
                gocompat::sort::SortSlice(&mut keyed, |a, b| a.0 < b.0);
                *v = keyed.into_iter().map(|(_, n)| n).collect();
            }
        }
        let (mut tl, mut br) = guard.ovgBoundingBox(self)?;
        // Align the bounds to ovgPadding so OVG nodes line up across levels.
        // Subtract one interval to leave at least one OVG node before the first node.
        tl.X -= ovgPadding;
        tl.X = math::Floor(tl.X / ovgPadding) * ovgPadding;
        let width = math::Ceil((br.X - tl.X) / ovgPadding) * ovgPadding;
        // Add one interval to leave at least one OVG node after the last node.
        br.X = tl.X + width + ovgPadding;
        let mut ys = goFloatSet::default();
        let empty: Vec<NodeRef> = Vec::new();
        for level in 1..nlevels {
            guard.step()?;
            let above = levelToNodes.get(&(level - 1)).unwrap_or(&empty);
            let below = levelToNodes.get(&level).unwrap_or(&empty);
            let levelYs = self.addBoundaryNodesAboveLevelNodes(above, below, tl.X, br.X, guard)?;
            self.addPortAlignedNodes(above, below, &levelYs, guard)?;
            for y in levelYs {
                ys.insert(y);
            }
        }

        let xs = self.addLevelNodes(&levelToNodes, tl.X, br.X, guard)?;
        self.addIntersections(g, &xs, &ys, guard)
    }

    /// addBoundaryNodesAboveLevelNodes adds nodes between two levels at the graph boundaries.
    pub(crate) fn addBoundaryNodesAboveLevelNodes(&mut self, nodesAbove: &[NodeRef], nodesBelow: &[NodeRef], leftX: f64, rightX: f64, guard: &mut ovgBuildGuard) -> GoResult<Vec<f64>> {
        guard.check()?;
        let mut nodesBelowMinY = math::Inf(1);
        let mut nEdgesAbove = 1.0f64;
        for node in nodesBelow {
            guard.step()?;
            // top is the closest to 0, 0 in SVG space
            nodesBelowMinY = math::Min(nodesBelowMinY, node.y());
            for e in &node.edges() {
                guard.step()?;
                let adj = node.Adjacent(e);
                if e.IsLoop() || adj.b().Hierarchy.is_none() {
                    // hierarchical nodes can be connected to nodes outside the hierarchy
                    continue;
                }
                if adj.HierarchyLevel() < node.HierarchyLevel() {
                    nEdgesAbove += 1.0;
                }
            }
        }

        let mut nodesAboveMaxY = math::Inf(-1);
        for node in nodesAbove {
            guard.step()?;
            nodesAboveMaxY = math::Max(nodesAboveMaxY, node.y() + node.h());
        }

        let levelDistance = nodesBelowMinY - nodesAboveMaxY;
        let maxHorizontalLines = levelDistance / ovgPadding;
        let nHorizontalLines = math::Min(nEdgesAbove, maxHorizontalLines);
        let mut pad = math::Ceil(levelDistance / nHorizontalLines);
        pad = math::Max(pad, ovgPadding);

        // these will form the horizontal lines above `nodesBelow`
        let mut ys: Vec<f64> = Vec::new();
        let mut y = nodesBelowMinY;
        let n = math::F64ToInt(nHorizontalLines) - 1;
        let mut i: i64 = 0;
        while i < n {
            guard.step()?;
            y -= pad;
            ys.push(y);
            guard.addPoint(self, geo::Point { X: leftX, Y: y })?;
            guard.addPoint(self, geo::Point { X: rightX, Y: y })?;
            i += 1;
        }
        guard.check()?;
        Ok(ys)
    }

    /// addPortAlignedNodes adds OVG Nodes aligned with ports (up/down) at the Y coordinates of the level
    pub(crate) fn addPortAlignedNodes(&mut self, nodesAbove: &[NodeRef], nodesBelow: &[NodeRef], ys: &[f64], guard: &mut ovgBuildGuard) -> GoResult<()> {
        guard.check()?;
        for &y in ys {
            guard.step()?;
            for node in nodesAbove {
                guard.step()?;
                let ports = guard.portsByOrientation(self, node, geo::Bottom)?;
                for port in &ports {
                    guard.addPoint(self, geo::Point { X: port.X(), Y: y })?;
                }
            }
            for node in nodesBelow {
                guard.step()?;
                let ports = guard.portsByOrientation(self, node, geo::Top)?;
                for port in &ports {
                    guard.addPoint(self, geo::Point { X: port.X(), Y: y })?;
                }
            }
        }
        guard.check()
    }

    /// addLevelNodes creates port aligned nodes between ordered siblings.
    /// Returns the X coordinate of added nodes.
    pub(crate) fn addLevelNodes(&mut self, levelToNodes: &Map<i64, Vec<NodeRef>>, minXBound: f64, maxXBound: f64, guard: &mut ovgBuildGuard) -> GoResult<goFloatSet> {
        guard.check()?;
        let mut xs = goFloatSet::default();
        let empty: Vec<NodeRef> = Vec::new();
        for l in 0..levelToNodes.len() as i64 {
            guard.step()?;
            let levelNodes = levelToNodes.get(&l).unwrap_or(&empty);
            let mut isLastNode = false;
            let mut ni = 0usize;
            let mut node = levelNodes[ni].clone();
            let mut ports = guard.portsByOrientation(self, &node, geo::Left)?;
            let mut x = minXBound;
            while x <= maxXBound {
                guard.step()?;
                xs.insert(x);
                for port in &ports {
                    guard.addPoint(self, geo::Point { X: x, Y: port.Y() })?;
                }
                if !isLastNode && x >= node.x() - ovgPadding {
                    // set `x` as the next X coordinate aligned with "common OVG nodes" (see the method docs)
                    let nextX = node.x() + node.w();
                    x = math::Ceil(nextX / ovgPadding) * ovgPadding;
                    // now use the ports on the right of this node
                    ports = guard.portsByOrientation(self, &node, geo::Right)?;
                    isLastNode = ni == levelNodes.len() - 1;
                    if !isLastNode {
                        // if there is other node to the right, use the left ports of this node too
                        ni += 1;
                        node = levelNodes[ni].clone();
                        let leftPorts = guard.portsByOrientation(self, &node, geo::Left)?;
                        ports.extend(leftPorts);
                    }
                }
                x += ovgPadding;
            }
        }
        guard.check()?;
        Ok(xs)
    }

    pub(crate) fn transpose(&mut self) {
        // Transpose each canonical OVG point and each owner's complete direction
        // set once.
        self.transformHierarchyOVGNodes(
            |node: &OVGNodeRef| {
                let mut n = node.bm();
                let (x, y) = (n.Point.X, n.Point.Y);
                n.Point.X = y;
                n.Point.Y = x;
            },
            transposePortDirection,
        );
        for node in &self.NodesInsideBoundingBox {
            node.Transpose();
        }
    }

    pub(crate) fn transformHierarchyOVGNodes(&mut self, transformPoint: impl Fn(&OVGNodeRef), transformDirection: impl Fn(geo::Orientation) -> geo::Orientation) {
        for node in &self.Nodes {
            transformPoint(node);
            let mut n = node.bm();
            for (_, metadata) in n.portOwnersByNode.iter_mut() {
                metadata.directions = metadata.directions.transformed(&transformDirection);
            }
        }
        self.reindexOccupiedPoints();
    }
}

pub(crate) fn transposePortDirection(direction: geo::Orientation) -> geo::Orientation {
    if direction == geo::Top {
        geo::Left
    } else if direction == geo::Bottom {
        geo::Right
    } else if direction == geo::Left {
        geo::Top
    } else if direction == geo::Right {
        geo::Bottom
    } else if direction == geo::TopRight {
        geo::BottomLeft
    } else if direction == geo::BottomLeft {
        geo::TopRight
    } else {
        direction
    }
}

pub(crate) fn mirrorPortDirection(mut direction: geo::Orientation, x: bool, y: bool) -> geo::Orientation {
    if x {
        if direction == geo::Left {
            direction = geo::Right;
        } else if direction == geo::Right {
            direction = geo::Left;
        } else if direction == geo::TopLeft {
            direction = geo::TopRight;
        } else if direction == geo::TopRight {
            direction = geo::TopLeft;
        } else if direction == geo::BottomLeft {
            direction = geo::BottomRight;
        } else if direction == geo::BottomRight {
            direction = geo::BottomLeft;
        }
    }
    if y {
        if direction == geo::Top {
            direction = geo::Bottom;
        } else if direction == geo::Bottom {
            direction = geo::Top;
        } else if direction == geo::TopLeft {
            direction = geo::BottomLeft;
        } else if direction == geo::TopRight {
            direction = geo::BottomRight;
        } else if direction == geo::BottomLeft {
            direction = geo::TopLeft;
        } else if direction == geo::BottomRight {
            direction = geo::TopRight;
        }
    }
    direction
}
