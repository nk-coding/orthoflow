// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from d2layouts/d2talalayout/internal/graphjson/direction.go
// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).
// Rust port: Copyright 2026 Niklas Krieger.

#[allow(unused_imports)]
use crate::*;

pub(crate) fn serializeDirection(direction: geo::Orientation) -> String {
    if direction == geo::Top {
        "up".to_string()
    } else if direction == geo::Bottom {
        "down".to_string()
    } else if direction == geo::Left {
        "left".to_string()
    } else if direction == geo::Right {
        "right".to_string()
    } else {
        String::new()
    }
}

pub(crate) fn deserializeDirection(direction: &str) -> geo::Orientation {
    match direction {
        "up" => geo::Top,
        "down" => geo::Bottom,
        "left" => geo::Left,
        "right" => geo::Right,
        _ => geo::NONE,
    }
}
