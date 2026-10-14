// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/engine/pipeline.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

pub(crate) const maxGraphSize: f64 = limits::MaxGraphSize as f64;

/// LayoutOptions configures one deterministic layout attempt.
#[derive(Clone, Copy, Debug, Default)]
pub struct LayoutOptions {
    pub Seed: i64,
}

pub(crate) type StageFn = fn(&mut pipeline, &Ctx) -> GoResult<()>;

#[derive(Clone, Copy)]
pub(crate) struct pipelineStage {
    pub(crate) name: &'static str,
    pub(crate) run: StageFn,
}

/// The Go pipeline also owns optional routing snapshots and stage timings
/// (developer instrumentation). Neither influences the layout, and the port
/// omits them (no clocks inside WASM); see DEVIATIONS.md.
pub(crate) struct pipeline {
    pub(crate) graph: GraphRef,
    pub(crate) seed: i64,
    pub(crate) random: RandRef,
    pub(crate) hierarchyPlacementRandom: RandRef,
    pub(crate) stages: Option<Vec<pipelineStage>>,
    pub(crate) forceReroute: bool,
    pub(crate) edgeRoutingComplete: bool,
    pub(crate) alignAxesNeeded: bool,
}

const fn st(name: &'static str, run: StageFn) -> pipelineStage {
    pipelineStage { name, run }
}

/// defaultPipelineStages is immutable after initialization.
pub(crate) static defaultPipelineStages: [pipelineStage; 37] = [
    st("Prescale", pipeline::prescaleStage),
    st("PreprocessSequences", pipeline::preprocessSequenceStage),
    st("Preprocess", pipeline::preprocessStage),
    st("PreprocessTrees", pipeline::preprocessTreesStage),
    st("PreprocessHierarchies", pipeline::preprocessHierarchies),
    st("PreprocessClusters", pipeline::preprocessClusters),
    st("PreprocessHubs", pipeline::preprocessHubs),
    st("NodePlacement", pipeline::nodePlacementStage),
    st("SwapStuff", pipeline::swapNodesStage),
    st("Transpose", pipeline::transposeStage),
    st("AlignAxes", pipeline::alignAxes),
    st("GapNormalization", pipeline::gapNormalizationStage),
    st("AlignAxes", pipeline::alignAxes),
    st("OptimizeClusters", pipeline::optimizeClustersStage),
    st("AlignAxes", pipeline::alignAxes),
    st("BalanceSymmetry", pipeline::balanceSymmetryStage),
    st("Equidistance", pipeline::equidistanceStage),
    st("AlignAxes", pipeline::alignAxes),
    st("BinPack", pipeline::binPack),
    st("CleanupStuff", pipeline::cleanupGroupsStage),
    st("Rescale", pipeline::rescaleStage),
    st("EdgeRouting", pipeline::edgeRoutingStage),
    st("Crosshatch", pipeline::crosshatchStage),
    st("Dejitter", pipeline::dejitterStage),
    st("EdgeRouting", pipeline::edgeRoutingStage),
    st("SimplifyEdgeRoutes", pipeline::simplifyEdgeRoutes),
    st("SwapEdgePorts", pipeline::swapEdgePorts),
    st("StraightEdgesFallback", pipeline::straightEdgesFallback),
    st("BalanceEdgeSegments", pipeline::balanceEdgeSegments),
    st("FixClusterEdgeBranching", pipeline::fixClusterEdgeBranching),
    st("TraceEdgesToShapeBorder", pipeline::traceEdgesToShapeBorder),
    st("ReorderDuplicates", pipeline::reorderDuplicates),
    // Second binpack doesn't try to avoid edges.
    st("BinPack", pipeline::binPack),
    st("PlaceLabels", pipeline::placeLabels),
    st("NudgeEdgeChannels", pipeline::nudgeEdgeChannels),
    st("ShortcutEdgeRoutes", pipeline::shortcutEdgeRoutes),
    st("Normalize", pipeline::normalizeStage),
];

/// newPipeline initializes one deterministic layout pipeline.
pub(crate) fn newPipeline(graph: &GraphRef, randSeed: i64) -> pipeline {
    pipeline {
        graph: graph.clone(),
        seed: randSeed,
        hierarchyPlacementRandom: gocompat::rand::New(gocompat::rand::NewSource(randSeed)),
        random: gocompat::rand::New(gocompat::rand::NewSource(randSeed)),
        stages: None,
        forceReroute: false,
        edgeRoutingComplete: false,
        alignAxesNeeded: true,
    }
}

impl pipeline {
    pub(crate) fn stagePlan(&self) -> Vec<pipelineStage> {
        match &self.stages {
            Some(s) => s.clone(),
            None => defaultPipelineStages.to_vec(),
        }
    }

    pub(crate) fn simplifyEdgeRoutes(&mut self, ctx: &Ctx) -> GoResult<()> {
        routing::SimplifyEdgeRoutes(ctx, &self.graph)
    }

    pub(crate) fn shortcutEdgeRoutes(&mut self, ctx: &Ctx) -> GoResult<()> {
        routing::ShortcutEdgeRoutes(ctx, &self.graph)
    }

    // prescaleStage takes trivial time, so it does not poll the context itself.
    pub(crate) fn prescaleStage(&mut self, _ctx: &Ctx) -> GoResult<()> {
        placement::Prescale(&self.graph);
        Ok(())
    }

    pub(crate) fn preprocessSequenceStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        grouping::AddSequences(ctx, &self.graph, &self.random)
    }

    pub(crate) fn preprocessHubs(&mut self, ctx: &Ctx) -> GoResult<()> {
        proximity::AddHubs(ctx, &self.graph)
    }

    pub(crate) fn preprocessClusters(&mut self, ctx: &Ctx) -> GoResult<()> {
        grouping::AddClusters(ctx, &self.graph, self.seed, &self.random)
    }

    pub(crate) fn preprocessHierarchies(&mut self, ctx: &Ctx) -> GoResult<()> {
        let candidates = hierarchy::Candidates(&self.graph);
        hierarchy::Assign(ctx, &self.graph, None, &candidates)?;
        hierarchy::Place(ctx, &self.graph, None, Some(&self.hierarchyPlacementRandom))?;
        hierarchy::RemoveIsolatedMemberships(&self.graph);
        Ok(())
    }

    pub(crate) fn preprocessStage(&mut self, _ctx: &Ctx) -> GoResult<()> {
        placement::Prepare(&self.graph);
        Ok(())
    }

    pub(crate) fn preprocessTreesStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        trees::Preprocess(ctx, &self.graph)
    }

    pub(crate) fn gapNormalizationStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        // Go assigns `changed` even when an error is returned (it is false then).
        match placement::NormalizeGaps(ctx, &self.graph) {
            Ok(changed) => {
                self.alignAxesNeeded = changed;
                Ok(())
            }
            Err(err) => {
                self.alignAxesNeeded = false;
                Err(err)
            }
        }
    }

    pub(crate) fn nodePlacementStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        placement::Place(ctx, &self.graph, self.seed)
    }

    pub(crate) fn optimizeClustersStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        self.graph.ComputeCellSize();
        match placement::OptimizeClusters(ctx, &self.graph) {
            Ok(v) => {
                self.alignAxesNeeded = v;
                Ok(())
            }
            Err(err) => {
                self.alignAxesNeeded = false;
                Err(err)
            }
        }
    }

    pub(crate) fn balanceSymmetryStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        placement::BalanceSymmetry(ctx, &self.graph)
    }

    pub(crate) fn binPack(&mut self, ctx: &Ctx) -> GoResult<()> {
        packing::Pack(ctx, &self.graph, None)
    }

    pub(crate) fn swapNodesStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        placement::Swap(ctx, &self.graph)
    }

    pub(crate) fn transposeStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        placement::TransposeAll(ctx, &self.graph)
    }

    pub(crate) fn cleanupGroupsStage(&mut self, _ctx: &Ctx) -> GoResult<()> {
        grouping::Cleanup(&self.graph);
        Ok(())
    }

    pub(crate) fn rescaleStage(&mut self, _ctx: &Ctx) -> GoResult<()> {
        self.graph.ComputeCellSize();
        placement::Pad(&self.graph);
        Ok(())
    }

    pub(crate) fn edgeRoutingStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        self.runGraphRouting(ctx, None)
    }

    /// RoutingCompleted (Go RouteCompletionObserver) and SubgraphRouted (Go
    /// SubgraphRouteObserver, which only clears debug snapshots) have no effect
    /// beyond `edgeRoutingComplete`, which runGraphRouting sets from RouteGraph's
    /// result and restores on failure, exactly like Go's deferred rollback.
    pub(crate) fn runGraphRouting(&mut self, ctx: &Ctx, workLimit: Option<u64>) -> GoResult<()> {
        let options = routing::GraphRouteOptions {
            ForceReroute: self.forceReroute,
            RoutesPreviouslyCompleted: self.edgeRoutingComplete,
            Observer: Some(std::rc::Rc::new(ovgReleaser)),
            CompletionObserver: None,
        };
        let routingComplete = match workLimit {
            None => routing::RouteGraph(ctx, &self.graph, options)?,
            Some(limit) => routing::RouteGraphWithWorkLimit(ctx, &self.graph, options, limit)?,
        };
        self.edgeRoutingComplete = routingComplete;
        Ok(())
    }

    /// straightEdgesFallback replaces expensive orthogonal routes with suitable
    /// straight routes after the normal routing attempts have completed.
    pub(crate) fn straightEdgesFallback(&mut self, ctx: &Ctx) -> GoResult<()> {
        routing::StraightEdgesFallback(ctx, &self.graph)
    }

    pub(crate) fn traceEdgesToShapeBorder(&mut self, ctx: &Ctx) -> GoResult<()> {
        routing::TraceEdgesToShapeBorder(ctx, &self.graph)
    }

    pub(crate) fn balanceEdgeSegments(&mut self, ctx: &Ctx) -> GoResult<()> {
        routing::BalanceEdgeSegments(ctx, &self.graph)
    }

    pub(crate) fn nudgeEdgeChannels(&mut self, ctx: &Ctx) -> GoResult<()> {
        routing::NudgeEdgeChannels(ctx, &self.graph)
    }

    pub(crate) fn fixClusterEdgeBranching(&mut self, ctx: &Ctx) -> GoResult<()> {
        routing::FixClusterEdgeBranching(ctx, &self.graph)
    }

    pub(crate) fn dejitterStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        match placement::Dejitter(ctx, &self.graph) {
            Ok(v) => {
                self.forceReroute = v;
                Ok(())
            }
            Err(err) => {
                self.forceReroute = false;
                Err(err)
            }
        }
    }

    pub(crate) fn equidistanceStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        self.graph.ComputeCellSize();
        match placement::Equidistance(ctx, &self.graph) {
            Ok(v) => {
                self.alignAxesNeeded = v;
                Ok(())
            }
            Err(err) => {
                self.alignAxesNeeded = false;
                Err(err)
            }
        }
    }

    pub(crate) fn alignAxes(&mut self, ctx: &Ctx) -> GoResult<()> {
        if !self.alignAxesNeeded {
            return Ok(());
        }
        self.alignAxesNeeded = false;
        placement::Align(ctx, &self.graph)
    }

    pub(crate) fn normalizeStage(&mut self, _ctx: &Ctx) -> GoResult<()> {
        placement::Normalize(&self.graph);
        Ok(())
    }

    pub(crate) fn reorderDuplicates(&mut self, ctx: &Ctx) -> GoResult<()> {
        routing::ReorderDuplicates(ctx, &self.graph)
    }

    pub(crate) fn placeLabels(&mut self, ctx: &Ctx) -> GoResult<()> {
        labeling::Place(ctx, &self.graph)
    }

    pub(crate) fn swapEdgePorts(&mut self, ctx: &Ctx) -> GoResult<()> {
        routing::SwapAllEdgePorts(ctx, &self.graph)
    }

    pub(crate) fn crosshatchStage(&mut self, ctx: &Ctx) -> GoResult<()> {
        routing::Crosshatch(ctx, &self.graph)
    }

    pub(crate) fn resetFullLayoutRouteState(&mut self) {
        for edge in self.graph.edges() {
            let lbl = edge.b().Label.clone();
            let mut e = edge.bm();
            e.Points = Vec::new();
            let fixed = lbl.as_ref().map_or(false, |l| l.PositionFixed());
            if !fixed {
                e.LabelPercentage = 0.0;
                if let Some(l) = &lbl {
                    let pos = l.b().Position;
                    if pos.IsUnlocked() {
                        l.bm().Position = label::Unset;
                    }
                }
            }
            e.IsCurve = false;
        }
        self.graph.ResetPlacementCosts();
    }

    /// runAllStages executes the layout stages in order against the pipeline graph.
    pub(crate) fn runAllStages(&mut self, ctx: &Ctx) -> GoResult<()> {
        let (ctx, _) = layoutgraph::EnsureTransactionWorkGuard(ctx, "AutolayoutTransactions")?;
        let stages = self.stagePlan();
        for (stageIndex, stage) in stages.iter().enumerate() {
            if let Some(err) = ctx.Err() {
                return Err(GoError::wrap(format!("{}: {}", stage.name, err), err));
            }
            if stageIndex == 0 {
                grouping::ResetClusters(&self.graph);
                self.resetFullLayoutRouteState();
            }
            (stage.run)(self, &ctx)?;
            if let Some(err) = ctx.Err() {
                return Err(GoError::wrap(format!("{}: {}", stage.name, err), err));
            }

            // Only check overflow if nodes have been placed
            let (tl, br) = self.graph.BoundingBox();
            if let (Some(tl), Some(br)) = (tl, br) {
                let (tl, br) = (*tl.b(), *br.b());
                let width = br.X - tl.X;
                let height = br.Y - tl.Y;
                if width > maxGraphSize || height > maxGraphSize {
                    return Err(invariant::Errorf(format!(
                        "Dimensions w:{}, h:{} reached after stage {}",
                        gocompat::strconv::FormatFloatV(width),
                        gocompat::strconv::FormatFloatV(height),
                        stage.name
                    )));
                }
            }
        }
        Ok(())
    }
}

/// Layout returns one complete deterministic layout in a new graph. Input is
/// validated and remains unchanged, so callers may safely reuse it for other
/// layout attempts.
pub fn Layout(ctx: &Ctx, graph: Option<&GraphRef>, options: LayoutOptions) -> GoResult<GraphRef> {
    if let Some(err) = ctx.Err() {
        return Err(GoError::wrap(format!("Autolayout: {}", err), err));
    }
    let Some(graph) = graph else {
        return Err(errorf!("TALA Autolayout requires a graph"));
    };
    let workspace = match layoutgraph::Clone(ctx, Some(graph)) {
        Ok(w) => w,
        Err(err) => {
            if let Some(contextErr) = ctx.Err() {
                return Err(GoError::wrap(format!("Autolayout: {}", contextErr), contextErr));
            }
            return Err(err);
        }
    };
    if let Err(err) = runLayout(ctx, Some(&workspace), options, false) {
        layoutgraph::Teardown(&workspace);
        return Err(err);
    }
    if let Some(err) = ctx.Err() {
        layoutgraph::Teardown(&workspace);
        return Err(GoError::wrap(format!("Autolayout: {}", err), err));
    }
    Ok(workspace)
}

pub(crate) fn runLayout(ctx: &Ctx, graph: Option<&GraphRef>, options: LayoutOptions, validate: bool) -> GoResult<()> {
    if let Some(err) = ctx.Err() {
        return Err(GoError::wrap(format!("Autolayout: {}", err), err));
    }
    let Some(graph) = graph else {
        return Err(errorf!("TALA Autolayout requires a graph"));
    };
    if validate {
        layoutgraph::Validate(ctx, "Autolayout", graph)?;
    }
    if graph.b().Nodes.is_empty() {
        return Ok(());
    }
    let mut workCtx = ctx.clone();
    let mut cancelWork: Option<gocompat::context::CancelFunc> = None;
    if ctx.Done().is_none() {
        let (c, cancel) = gocompat::context::WithCancel(ctx);
        workCtx = c;
        cancelWork = Some(cancel);
    }
    let mut p = newPipeline(graph, options.Seed);
    let result = p.runAllStages(&workCtx);
    if let Some(cancel) = cancelWork {
        cancel();
    }
    result
}

/// Rust-only SubgraphRouteObserver (stands in for Go's pipeline observer, whose
/// only effect is clearing debug snapshots). Go drops each subgraph's OVG right
/// after this callback; its nodes and edges reference each other (Rc cycle),
/// so the port unlinks them here to avoid leaking memory. Nothing reads the OVG
/// after the callback.
pub(crate) struct ovgReleaser;

impl routing::SubgraphRouteObserver for ovgReleaser {
    fn SubgraphRouted(&self, ovg: &routing::OVGRef) -> GoResult<()> {
        let mut o = ovg.bm();
        for edge in &o.Edges {
            edge.From.bm().Edges.clear();
            edge.To.bm().Edges.clear();
        }
        for node in &o.Nodes {
            node.bm().Edges.clear();
        }
        for (_, n) in o.OccupiedPoints.iter() {
            if let Ok(mut nb) = n.0.try_borrow_mut() {
                nb.Edges.clear();
            }
        }
        o.Nodes.clear();
        o.Edges.clear();
        o.OccupiedPoints.clear();
        o.VerticalEdges.clear();
        o.HorizontalEdges.clear();
        o.Ports.clear();
        o.Centers.clear();
        Ok(())
    }
}
