// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/errors.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

use crate::*;

/// ErrInvalidCandidate rejects a speculative layout candidate whose geometry
/// cannot be committed.
pub static ErrInvalidCandidate: Sentinel = Sentinel::new("invalid layout candidate");
/// ErrNonImprovingCandidate rejects a valid candidate that does not improve
/// the current layout.
pub static ErrNonImprovingCandidate: Sentinel = Sentinel::new("layout candidate did not improve");

/// IsCandidateRejection reports whether err rejects only the current
/// speculative candidate. Callers must propagate every other error.
pub fn IsCandidateRejection(err: Option<&GoError>) -> bool {
    match err {
        Some(e) => e.Is(&ErrInvalidCandidate) || e.Is(&ErrNonImprovingCandidate),
        None => false,
    }
}
