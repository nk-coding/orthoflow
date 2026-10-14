// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/layoutgraph/shape.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

#[allow(unused_imports)]
use crate::*;

pub(crate) const stepType: nodeshape::Kind = nodeshape::Step;
pub(crate) const circleType: nodeshape::Kind = nodeshape::Circle;
pub(crate) const hexagonType: nodeshape::Kind = nodeshape::Hexagon;
pub(crate) const tableType: nodeshape::Kind = nodeshape::Table;
pub(crate) const classType: nodeshape::Kind = nodeshape::Class;
pub(crate) const textType: nodeshape::Kind = nodeshape::Text;
pub(crate) const codeType: nodeshape::Kind = nodeshape::Code;
pub(crate) const imageType: nodeshape::Kind = nodeshape::Image;
