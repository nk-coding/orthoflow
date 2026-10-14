// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Copyright 2026 Niklas Krieger

extern crate orthoflow_testkit as testkit;
// Harness self-check: a no-op "stage" passes exactly where Go's stage was a no-op.
#[test]
#[ignore = "needs Go reference data (ORTHOFLOW_GOREF), see README"]
fn noop_replay_selfcheck() {
    let r = testkit::replay_stage("PreprocessHubs", |_, _| Ok(()));
    eprintln!("noop as PreprocessHubs: {} passed, {} failed", r.passed, r.failed.len());
    assert!(r.passed > 0 && r.passed + r.failed.len() == 81);
}
