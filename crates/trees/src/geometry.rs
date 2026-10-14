// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/trees/geometry.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const siblingSpacing: f64 = 50.0;
pub(crate) const directionPenalty: f64 = 2.0 * layoutgraph::TreeParentSpacing;

/// get a map from depth level 0-n to tree nodes at that depth (levels are
/// contiguous from 0, so a Vec indexed by level is equivalent to Go's map).
pub(crate) fn levelsByDepth(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<Vec<Vec<TreeRef>>> {
    let mut queue: Vec<(TreeRef, usize)> = vec![(t.clone(), 0)];
    let mut seen: Set<TreeRef> = Set::default();
    let mut treeLevels: Vec<Vec<TreeRef>> = Vec::new();
    let mut index = 0;
    while index < queue.len() {
        guard.Step()?;
        let (tree, level) = queue[index].clone();
        if seen.contains(&tree) {
            return Err(treePreprocessBadState("tree placement encountered a repeated tree node"));
        }
        seen.insert(tree.clone());
        while treeLevels.len() <= level {
            treeLevels.push(Vec::new());
        }
        treeLevels[level].push(tree.clone());
        let children = tree.b().Children.clone();
        for child in children {
            guard.Step()?;
            queue.push((child, level + 1));
        }
        index += 1;
    }
    Ok(treeLevels)
}

pub(crate) fn shiftSubtreeHorizontally(t: &TreeRef, delta: f64, guard: &WorkGuardRef) -> GoResult<()> {
    offsetSubtree(t, delta, 0.0, guard)
}

pub(crate) fn spacingToChildren(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<f64> {
    let (children, orientation) = {
        let tb = t.b();
        (tb.Children.clone(), tb.Orientation)
    };
    let mut maxLabelSize = 0.0f64;
    for c in &children {
        guard.Step()?;
        let Some(se) = c.b().SentinelEdge.clone() else {
            return Err(treePreprocessBadState("tree placement encountered an incomplete child edge"));
        };
        let e = se.b();
        let mut labelSize = e.MinHeight as f64;
        if orientation == geo::Left || orientation == geo::Right {
            labelSize = e.MinWidth as f64;
        }
        maxLabelSize = math::Max(maxLabelSize, labelSize);
    }
    if maxLabelSize == 0.0 {
        return Ok(layoutgraph::TreeParentSpacing);
    }
    let labelSpacingWithArrowheads = maxLabelSize + 2.0 * layoutgraph::MinArrowheadClearance;
    if children.len() == 1 {
        return Ok(math::Max(layoutgraph::TreeParentSpacing, labelSpacingWithArrowheads));
    }
    Ok(layoutgraph::TreeParentSpacing / 2.0 + math::Max(layoutgraph::TreeParentSpacing / 2.0, labelSpacingWithArrowheads))
}

fn node_of(t: &TreeRef) -> NodeRef {
    t.b().Node.clone()
}

pub(crate) fn layoutTree(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let treeLevels = levelsByDepth(t, guard)?;
    let nLevels = treeLevels.len();

    let mut levelHeights: Vec<f64> = vec![0.0; nLevels];
    for level in 0..nLevels {
        let mut levelHeight = 0.0f64;
        for levelNode in &treeLevels[level] {
            guard.Step()?;
            levelHeight = math::Max(levelHeight, node_of(levelNode).h());
        }
        levelHeights[level] = levelHeight;
    }
    let mut levelSpacings: Vec<f64> = vec![0.0; nLevels + 1];
    if nLevels > 0 {
        for level in 0..nLevels - 1 {
            for levelNode in &treeLevels[level] {
                guard.Step()?;
                let spacing = spacingToChildren(levelNode, guard)?;
                levelSpacings[level + 1] = math::Max(levelSpacings[level + 1], spacing);
            }
        }
    }

    let mut levelBottom = 0.0f64;
    for level in 1..nLevels {
        let levelTop = levelBottom + levelSpacings[level];
        for levelNode in &treeLevels[level] {
            guard.Step()?;
            let n = node_of(levelNode);
            let centerOffset = ((levelHeights[level] - n.h()) / 2.0).round();
            moveTreeNodeAbsWithChildren(&n, n.x(), levelTop + centerOffset, guard)?;
        }
        levelBottom = levelTop + levelHeights[level];
    }

    let mut level = nLevels as i64 - 1;
    while level > 0 {
        let lv = level as usize;
        let mut nextPosition = 0.0f64;
        for levelNode in &treeLevels[lv] {
            guard.Step()?;
            let n = node_of(levelNode);
            let hasChildren = !levelNode.b().Children.is_empty();
            if !hasChildren {
                moveTreeNodeAbsWithChildren(&n, nextPosition, n.y(), guard)?;
            } else {
                let next = &treeLevels[lv + 1];
                let mut lastChildPosition = next.len();
                let mut i = next.len() as i64 - 1;
                while i >= 0 {
                    guard.Step()?;
                    if next[i as usize].b().Parent.as_ref() == Some(levelNode) {
                        lastChildPosition = i as usize;
                        break;
                    }
                    i -= 1;
                }
                let shiftSubsequentLevelNodes = |dx: f64| -> GoResult<()> {
                    for i in lastChildPosition + 1..next.len() {
                        guard.Step()?;
                        shiftSubtreeHorizontally(&next[i], dx, guard)?;
                    }
                    Ok(())
                };

                let spacingBefore = totalChildrenSpacing(levelNode, guard)?;
                spaceChildrenEvenly(levelNode, guard)?;
                let spacingAfter = totalChildrenSpacing(levelNode, guard)?;
                let totalShift = spacingAfter - spacingBefore;
                if totalShift > 0.0 {
                    shiftSubsequentLevelNodes(totalShift)?;
                }
                let siblingCenter = childrenCenterX(levelNode, guard)?;
                moveTreeNodeAbsWithChildren(&n, (siblingCenter - n.w() / 2.0).round(), n.y(), guard)?;

                let positionDiff = nextPosition - n.x();
                if positionDiff > 0.0 {
                    shiftSubtreeHorizontally(levelNode, positionDiff, guard)?;
                    shiftSubsequentLevelNodes(positionDiff)?;
                }
            }
            nextPosition = n.x() + n.w() + siblingSpacing;
        }
        level -= 1;
    }
    guard.Finish()
}

pub(crate) fn validateBottomOrientation(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let mut stack: Vec<TreeRef> = vec![t.clone()];
    let mut seen: Set<TreeRef> = Set::default();
    while let Some(current) = stack.pop() {
        guard.Step()?;
        let (node, parent, children) = {
            let c = current.b();
            (c.Node.clone(), c.Parent.clone(), c.Children.clone())
        };
        if !node.has_tl() {
            return Err(treePreprocessBadState("tree placement encountered an incomplete tree"));
        }
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree placement encountered a repeated tree node"));
        }
        seen.insert(current.clone());
        if let Some(parent) = parent {
            let pn = parent.b().Node.clone();
            if !pn.has_tl() {
                return Err(treePreprocessBadState("tree placement encountered an incomplete parent"));
            }
            if node.y() < pn.y() + pn.h() {
                return Err(invariant::Errorf(format!("tree node {} is not in Bottom orientation", node.id())));
            }
        }
        for v in children.iter().rev() {
            guard.Step()?;
            stack.push(v.clone());
        }
    }
    guard.Check()
}

pub(crate) fn positionEdgeLabels(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let mut stack: Vec<(TreeRef, bool)> = vec![(t.clone(), false)];
    let mut seen: Set<TreeRef> = Set::default();
    while let Some((current, emit)) = stack.pop() {
        guard.Step()?;
        if !emit {
            if seen.contains(&current) {
                return Err(treePreprocessBadState("tree placement encountered a repeated tree node"));
            }
            seen.insert(current.clone());
            stack.push((current.clone(), true));
            let children = current.b().Children.clone();
            for v in children.iter().rev() {
                guard.Step()?;
                stack.push((v.clone(), false));
            }
            continue;
        }

        let (node, edge, parent, orientation) = {
            let c = current.b();
            (c.Node.clone(), c.SentinelEdge.clone(), c.Parent.clone(), c.Orientation)
        };
        let Some(edge) = edge else {
            return Err(treePreprocessBadState(format!(
                "tree node {} has no sentinel edge for label placement",
                node.id()
            )));
        };
        let Some(lbl) = edge.b().Label.clone() else { continue };
        let mut labelSize = edge.b().MinHeight as f64;
        if orientation.IsHorizontal() {
            labelSize = edge.b().MinWidth as f64;
        }
        if labelSize != 0.0 {
            let Some(parent) = parent else {
                return Err(treePreprocessBadState(format!("tree node {} has no parent for label placement", node.id())));
            };
            let (parentNode, siblingCount) = {
                let p = parent.b();
                (p.Node.clone(), p.Children.len())
            };
            if siblingCount == 1 {
                lbl.bm().Position = label::UnlockedMiddle;
                edge.bm().LabelPercentage = 0.5;
            } else {
                let childNode = node.clone();
                let dx = childNode.Center().b().X - parentNode.Center().b().X;
                let dy = childNode.y() - (parentNode.y() + parentNode.h());
                let totalLength = dx.abs() + dy;
                let childSegmentLength = dy - layoutgraph::TreeParentSpacing / 2.0;

                let mut distanceAlongEdge = childSegmentLength / 2.0;
                let isChildToParent = edge.from() == childNode;
                if !isChildToParent {
                    distanceAlongEdge = totalLength - childSegmentLength / 2.0;
                }
                edge.bm().LabelPercentage = distanceAlongEdge / totalLength;
                let mut position = if dx < 0.0 { label::UnlockedBottom } else { label::UnlockedTop };
                if isChildToParent {
                    position = position.Mirrored();
                }
                if orientation.IsHorizontal() {
                    position = position.Mirrored();
                }
                lbl.bm().Position = position;
            }
        }
        guard.Finish()?;
    }
    Ok(())
}

/// We compute the children's center by averaging the their centers
pub(crate) fn childrenCenterX(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<f64> {
    let children = t.b().Children.clone();
    if children.is_empty() {
        return Err(treePreprocessBadState("tree placement cannot center a leaf"));
    }
    let mut centerX = 0.0f64;
    for child in &children {
        guard.Step()?;
        let n = node_of(child);
        if !n.has_tl() {
            return Err(treePreprocessBadState("tree placement encountered an incomplete child"));
        }
        centerX += n.x() + n.w() / 2.0;
    }
    centerX /= children.len() as f64;
    Ok(centerX)
}

pub(crate) fn spaceChildrenEvenly(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let children = t.b().Children.clone();
    let mut maxSpacing = siblingSpacing;
    let mut childrenSpacing: Vec<f64> = vec![0.0];
    for i in 1..children.len() {
        guard.Step()?;
        let (a, b) = (node_of(&children[i]), node_of(&children[i - 1]));
        let s = a.x() - (b.x() + b.w());
        maxSpacing = math::Max(maxSpacing, s);
        childrenSpacing.push(s);
    }

    for i in 1..children.len() {
        guard.Step()?;
        let shiftAmount = maxSpacing - childrenSpacing[i];
        if shiftAmount == 0.0 {
            continue;
        }
        let sb = spacingBefore(t, i, guard)?;
        if sb + shiftAmount > 10.0 * siblingSpacing {
            return Ok(());
        }
        if i + 1 < children.len() {
            childrenSpacing[i + 1] -= shiftAmount;
        }
    }

    for i in 1..children.len() {
        guard.Step()?;
        let shiftAmount = maxSpacing - childrenSpacing[i];
        if shiftAmount > 0.0 {
            shiftSubtreeHorizontally(&children[i], shiftAmount, guard)?;
        }
    }
    Ok(())
}

pub(crate) fn totalChildrenSpacing(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<f64> {
    let children = t.b().Children.clone();
    let mut spacing = 0.0f64;
    for i in 1..children.len() {
        guard.Step()?;
        let (a, b) = (node_of(&children[i]), node_of(&children[i - 1]));
        spacing += a.x() - (b.x() + b.w());
    }
    Ok(spacing)
}

pub(crate) fn spacingBefore(t: &TreeRef, childIndex: usize, guard: &WorkGuardRef) -> GoResult<f64> {
    let children = t.b().Children.clone();
    let rights = levelRights(&children[childIndex - 1], guard)?;
    let lefts = levelLefts(&children[childIndex], guard)?;
    let mut minSpace = lefts[0] - rights[0];
    let mut level = 1;
    while level < lefts.len() && level < rights.len() {
        guard.Step()?;
        minSpace = math::Min(minSpace, lefts[level] - rights[level]);
        level += 1;
    }
    Ok(minSpace)
}

pub(crate) fn levelLefts(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<Vec<f64>> {
    let levels = levelsByDepth(t, guard)?;
    let mut lefts = vec![0.0f64; levels.len()];
    for level in 0..levels.len() {
        if levels[level].is_empty() || !node_of(&levels[level][0]).has_tl() {
            return Err(treePreprocessBadState("tree placement encountered an incomplete level"));
        }
        guard.Step()?;
        lefts[level] = node_of(&levels[level][0]).x();
    }
    Ok(lefts)
}

pub(crate) fn levelRights(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<Vec<f64>> {
    let levels = levelsByDepth(t, guard)?;
    let mut rights = vec![0.0f64; levels.len()];
    for level in 0..levels.len() {
        if levels[level].is_empty() {
            return Err(treePreprocessBadState("tree placement encountered an empty level"));
        }
        let last = node_of(levels[level].last().unwrap());
        if !last.has_tl() {
            return Err(treePreprocessBadState("tree placement encountered an incomplete level"));
        }
        guard.Step()?;
        rights[level] = last.x() + last.w();
    }
    Ok(rights)
}

pub(crate) fn centerAlignChildren(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let childrenCenter = childrenCenterX(t, guard)?;
    let n = node_of(t);
    let xOffset = ((n.x() + n.w() / 2.0) - childrenCenter).round();
    let children = t.b().Children.clone();
    for child in &children {
        guard.Step()?;
        offsetSubtree(child, xOffset, 0.0, guard)?;
    }
    guard.Finish()
}

/// treePlacementDescendants is the guarded equivalent of
/// Graph.AllDescendantNodes for tree-placement movement.
pub(crate) fn treePlacementDescendants(node: &NodeRef, guard: &WorkGuardRef) -> GoResult<Vec<NodeRef>> {
    let Some(g) = node.graph() else {
        return Err(treePreprocessBadState("tree placement cannot move a node without graph ownership"));
    };
    let (sequences, clusters, containers) = (g.sequences(), g.clusters(), g.containers());
    let mut seen: Set<NodeRef> = Set::default();
    seen.insert(node.clone());
    let mut stack: Vec<NodeRef> = Vec::new();
    let pushChildren = |parent: &NodeRef, stack: &mut Vec<NodeRef>| -> GoResult<()> {
        if let Some(sequence) = sequences.get(parent) {
            let nodes = sequence.b().Nodes.clone();
            for v in nodes.iter().rev() {
                guard.Step()?;
                stack.push(v.clone());
            }
        }
        if parent.IsClusterVessel() {
            if let Some(cluster) = clusters.get(parent) {
                let nodes = cluster.b().Nodes.clone();
                for v in nodes.iter().rev() {
                    guard.Step()?;
                    stack.push(v.clone());
                }
            }
        }
        if parent.IsContainer() {
            let children = containers.at(&Some(parent.clone()));
            for c in children.iter().rev() {
                guard.Step()?;
                stack.push(c.clone());
            }
        }
        Ok(())
    };
    pushChildren(node, &mut stack)?;
    let mut descendants: Vec<NodeRef> = Vec::with_capacity(stack.len());
    while let Some(current) = stack.pop() {
        guard.Step()?;
        if seen.contains(&current) {
            continue;
        }
        seen.insert(current.clone());
        descendants.push(current.clone());
        pushChildren(&current, &mut stack)?;
    }
    Ok(descendants)
}

pub(crate) fn moveTreeNodeWithChildren(node: &NodeRef, dx: f64, dy: f64, guard: &WorkGuardRef) -> GoResult<()> {
    if !node.has_tl() {
        return Err(treePreprocessBadState("tree placement cannot move an incomplete node"));
    }
    if dx == 0.0 && dy == 0.0 {
        return guard.Check();
    }
    let descendants = treePlacementDescendants(node, guard)?;
    guard.Step()?;
    node.Translate(dx, dy);
    guard.Finish()?;
    for descendant in &descendants {
        guard.Step()?;
        if !descendant.has_tl() {
            return Err(treePreprocessBadState("tree placement encountered a descendant without a position"));
        }
        descendant.Translate(dx, dy);
        guard.Finish()?;
    }
    Ok(())
}

pub(crate) fn moveTreeNodeAbsWithChildren(node: &NodeRef, x: f64, y: f64, guard: &WorkGuardRef) -> GoResult<()> {
    if !node.has_tl() {
        return Err(treePreprocessBadState("tree placement cannot move an incomplete node"));
    }
    let (nx, ny) = (node.x(), node.y());
    if nx == x && ny == y {
        return guard.Check();
    }
    moveTreeNodeWithChildren(node, x - nx, y - ny, guard)
}

pub(crate) fn offsetSubtree(t: &TreeRef, xOffset: f64, yOffset: f64, guard: &WorkGuardRef) -> GoResult<()> {
    let mut stack: Vec<TreeRef> = vec![t.clone()];
    let mut seen: Set<TreeRef> = Set::default();
    while let Some(current) = stack.pop() {
        guard.Step()?;
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree placement encountered a repeated tree node"));
        }
        seen.insert(current.clone());
        let (node, children) = {
            let c = current.b();
            (c.Node.clone(), c.Children.clone())
        };
        moveTreeNodeWithChildren(&node, xOffset, yOffset, guard)?;
        for v in children.iter().rev() {
            guard.Step()?;
            stack.push(v.clone());
        }
    }
    guard.Finish()
}

/// swaps the x and y coordinates as well as the width and height of all nodes in the tree
pub(crate) fn swapDimensions(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let mut stack: Vec<TreeRef> = vec![t.clone()];
    let mut seen: Set<TreeRef> = Set::default();
    while let Some(current) = stack.pop() {
        guard.Step()?;
        let (node, children) = {
            let c = current.b();
            (c.Node.clone(), c.Children.clone())
        };
        if !node.has_tl() {
            return Err(treePreprocessBadState("tree placement encountered an incomplete tree"));
        }
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree placement encountered a repeated tree node"));
        }
        seen.insert(current.clone());
        moveTreeNodeAbsWithChildren(&node, node.y(), node.x(), guard)?;
        let (w, h) = (node.w(), node.h());
        node.set_w(h);
        node.set_h(w);
        guard.Finish()?;
        for v in children.iter().rev() {
            guard.Step()?;
            stack.push(v.clone());
        }
    }
    Ok(())
}

/// flips all nodes in the tree such that the top left is the mirror topleft accounting for the width and height
pub(crate) fn flip(t: &TreeRef, guard: &WorkGuardRef) -> GoResult<()> {
    let mut stack: Vec<TreeRef> = vec![t.clone()];
    let mut seen: Set<TreeRef> = Set::default();
    while let Some(current) = stack.pop() {
        guard.Step()?;
        let (node, children) = {
            let c = current.b();
            (c.Node.clone(), c.Children.clone())
        };
        if !node.has_tl() {
            return Err(treePreprocessBadState("tree placement encountered an incomplete tree"));
        }
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree placement encountered a repeated tree node"));
        }
        seen.insert(current.clone());
        moveTreeNodeAbsWithChildren(&node, -(node.x() + node.w()), -(node.y() + node.h()), guard)?;
        for v in children.iter().rev() {
            guard.Step()?;
            stack.push(v.clone());
        }
    }
    guard.Finish()
}

pub(crate) fn setOrientation(t: &TreeRef, o: geo::Orientation, guard: &WorkGuardRef) -> GoResult<()> {
    let mut stack: Vec<TreeRef> = vec![t.clone()];
    let mut seen: Set<TreeRef> = Set::default();
    while let Some(current) = stack.pop() {
        guard.Step()?;
        if seen.contains(&current) {
            return Err(treePreprocessBadState("tree placement encountered a repeated tree node"));
        }
        seen.insert(current.clone());
        current.bm().Orientation = o;
        guard.Finish()?;
        let children = current.b().Children.clone();
        for v in children.iter().rev() {
            guard.Step()?;
            stack.push(v.clone());
        }
    }
    Ok(())
}
