// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from the following files of terrastruct/d2, Copyright 2022 Terrastruct Inc.:
//   - d2layouts/d2talalayout/internal/routing/route_flavor_worker_test.go
//   - d2layouts/d2talalayout/internal/routing/cancellation_test.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Port of routing/route_flavor_worker_test.go (panic and goroutine-drain
//! cases do not apply: flavors run sequentially and panics propagate).
use super::*;

fn routeFlavorTestRouters() -> Vec<RouteGenerationFlavor> {
    vec![ShortestToLongest, LongestToShortest, Default]
}

fn response(err: Option<gocompat::GoError>, distance: f64) -> GenerateRouteResponse {
    GenerateRouteResponse { Err: err, Distance: distance, ..std::default::Default::default() }
}

#[test]
fn TestRouteFlavorOrdinaryFailureAllowsSuccessfulFallback() {
    let wantErr = gocompat::GoError::new("shortest failed");
    let mut routers = routeFlavorTestRouters();
    let responses = generateRouteFlavorResponsesWith(&bg(), &mut routers, |f| *f, false, |flavor, _, _| {
        if *flavor == ShortestToLongest {
            return response(Some(wantErr.clone()), 0.0);
        }
        response(None, 42.0)
    })
    .unwrap();
    let successful = successfulRouteFlavorResponses(responses).unwrap();
    assert!(!successful.contains_key(&ShortestToLongest), "failed flavor was reported as successful");
    for flavor in [LongestToShortest, Default] {
        let response = successful.get(&flavor).expect("missing successful fallback flavor");
        assert_eq!(response.Distance, 42.0);
    }
}

#[test]
fn TestRouteFlavorAggregateFailureRejectsSuccessfulFallback() {
    let aggregateErr = gocompat::GoError::wrap("injected aggregate exhaustion", errRouteStageWorkLimit.err());
    let mut responses = vec![response(Some(aggregateErr), 0.0), response(None, 42.0), response(None, 43.0)];
    for (r, f) in responses.iter_mut().zip(routeFlavorTestRouters()) {
        r.Flavor = f;
    }
    let err = successfulRouteFlavorResponses(responses).err().expect("aggregate stage limit accepted");
    assert!(err.Is(&errRouteStageWorkLimit), "error = {err}");
}

#[test]
fn TestAllRouteFlavorFailuresUseDeclaredOrder() {
    let errs = [
        gocompat::GoError::new("shortest failed"),
        gocompat::GoError::new("longest failed"),
        gocompat::GoError::new("default failed"),
    ];
    let mut routers = routeFlavorTestRouters();
    let responses = generateRouteFlavorResponsesWith(&bg(), &mut routers, |f| *f, false, |flavor, _, _| {
        let i = routeFlavorTestRouters().iter().position(|f| f == flavor).unwrap();
        response(Some(errs[i].clone()), 0.0)
    })
    .unwrap();
    let err = successfulRouteFlavorResponses(responses).err().unwrap();
    assert!(err.IsErr(&errs[0]), "error = {err}, want first declared flavor error");
}

#[test]
fn TestCollectRouteFlavorResponsesPreservesCancellation() {
    let mut routers = routeFlavorTestRouters();
    let err = generateRouteFlavorResponsesWith(&canceledContext(), &mut routers, |f| *f, false, |_, _, _| response(None, 1.0))
        .err()
        .unwrap();
    requireCanceledAt(&err, "EdgeRouting");
}

#[test]
fn TestRouteFlavorCountIsBounded() {
    let mut routers = vec![Default; 4];
    let err = generateRouteFlavorResponsesWith(&bg(), &mut routers, |f| *f, false, |_, _, _| response(None, 1.0)).err().unwrap();
    assert!(err.Is(&errRouteSearchWorkLimit), "{err}");
}
