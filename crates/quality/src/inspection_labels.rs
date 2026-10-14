// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/quality/inspection_labels.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

#[derive(Clone)]
pub(crate) struct positionedLabel {
    pub(crate) bx: qbox,
    pub(crate) node: Option<NodeRef>,
    pub(crate) edge: Option<EdgeRef>,
}

/// measureLabels treats all rendered label and icon boxes consistently.
pub(crate) fn measureLabels(g: &GraphRef, score: &mut Metrics, guard: &WorkGuardRef) -> GoResult<()> {
    guard.Step()?;
    let (gnodes, gedges) = {
        let gb = g.b();
        (gb.Nodes.clone(), gb.Edges.clone())
    };
    let mut labels: Vec<positionedLabel> = Vec::new();
    let mut appendLabel = |labels: &mut Vec<positionedLabel>, bx: qbox, node: Option<NodeRef>, edge: Option<EdgeRef>| {
        if bx.Width > 0.0 && bx.Height > 0.0 {
            labels.push(positionedLabel { bx, node, edge });
        }
    };
    for node in &gnodes {
        guard.Step()?;
        if node.b().IsInvisible {
            continue;
        }
        if let Some(l) = node.label() {
            let (pos, w, h) = {
                let lb = l.b();
                (lb.Position, lb.Width, lb.Height)
            };
            if pos != label::Unset {
                if pos.IsOutside() {
                    chargeEvaluationWork(guard, 4 * gnodes.len() as i64)?;
                }
                appendLabel(&mut labels, qbox::new(Some(node.LabelTopLeft(pos, w, h)), w, h), Some(node.clone()), None);
            }
        }
        if let Some(icon) = node.icon() {
            if !node.IsImage() {
                let ipos = icon.b().Position;
                if ipos.IsOutside() {
                    chargeEvaluationWork(guard, 4 * gnodes.len() as i64)?;
                }
                let size = node.IconSize(ipos);
                appendLabel(&mut labels, qbox::new(Some(node.LabelTopLeft(ipos, size, size)), size, size), Some(node.clone()), None);
            }
        }
    }
    let mut visibleEdges: Vec<EdgeRef> = Vec::with_capacity(gedges.len());
    for edge in &gedges {
        guard.Step()?;
        if edge.b().IsInvisible {
            continue;
        }
        visibleEdges.push(edge.clone());
        let n = edge.b().Points.len();
        if n < 2 {
            continue;
        }
        let lbl = edge.b().Label.clone();
        if let Some(l) = lbl {
            if l.b().Position != label::Unset {
                chargeEvaluationWork(guard, n as i64)?;
                appendLabel(&mut labels, labelBox(edge), None, Some(edge.clone()));
            }
        }
        if edge.b().SourceArrowheadLabel.is_some() {
            chargeEvaluationWork(guard, n as i64)?;
            let pal = labeling::PositionArrowheadLabel(edge, false, &edge.points()).expect("nil arrowhead label");
            appendLabel(&mut labels, qbox::of(&pal.Box), None, Some(edge.clone()));
        }
        if edge.b().TargetArrowheadLabel.is_some() {
            chargeEvaluationWork(guard, n as i64)?;
            let pal = labeling::PositionArrowheadLabel(edge, true, &edge.points()).expect("nil arrowhead label");
            appendLabel(&mut labels, qbox::of(&pal.Box), None, Some(edge.clone()));
        }
    }
    for (i, placed) in labels.iter().enumerate() {
        guard.Step()?;
        for other in &labels[..i] {
            guard.Step()?;
            if boxesOverlapWithPadding(placed.bx, other.bx, 0.0) {
                score.TextOcclusions += 1;
            }
        }
        for node in &gnodes {
            guard.Step()?;
            if node.b().IsInvisible || !node.shape().IsRectangular() {
                continue;
            }
            let nb = qbox::node(node);
            if !boxesOverlapWithPadding(placed.bx, nb, 0.0) {
                continue;
            }
            let ancestor;
            if let Some(pn) = &placed.node {
                ancestor = evaluationIsDescendantOf(Some(pn), Some(node), guard)?;
            } else {
                let pe = placed.edge.as_ref().unwrap();
                let a = endpointAncestor(pe, node, guard)?;
                ancestor = a && *node != pe.from() && *node != pe.to();
            }
            if ancestor && boxCovers(nb, placed.bx) {
                continue;
            }
            score.TextOcclusions += 1;
        }
        for edge in &visibleEdges {
            guard.Step()?;
            if placed.edge.as_ref() == Some(edge) || edge.b().IsCurve {
                continue;
            }
            let mut overlap = false;
            let points: Vec<geo::Point> = edge.b().Points.iter().map(|p| *p.b()).collect();
            for j in 1..points.len() {
                guard.Step()?;
                if segmentEntersBox(placed.bx, &points[j - 1], &points[j]) {
                    overlap = true;
                    break;
                }
            }
            if overlap {
                score.TextOcclusions += 1;
            }
        }
    }
    Ok(())
}
