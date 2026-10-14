// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
//
// Ported from the following files of terrastruct/d2, Copyright 2022 Terrastruct Inc.:
//   - d2layouts/d2talalayout/internal/nodeshape/shape_callout.go
//   - d2layouts/d2talalayout/internal/nodeshape/shape_cylinder.go
//   - d2layouts/d2talalayout/internal/nodeshape/shape_document.go
//   - d2layouts/d2talalayout/internal/nodeshape/shape_package.go
//   - d2layouts/d2talalayout/internal/nodeshape/shape_parallelogram.go
//   - d2layouts/d2talalayout/internal/nodeshape/shape_queue.go
//   - d2layouts/d2talalayout/internal/nodeshape/shape_step.go
//   - d2layouts/d2talalayout/internal/nodeshape/shape_stored_data.go
// Rust port: Copyright 2026 Niklas Krieger.

//! Hand-ported computed `SnapPointPercentages` bodies.
use crate::*;
use geo::{NewBezierCurve, NewPoint, NewRelativePoint as rp, RelativePointRef};
use gocompat::math;

fn bx(s: &dyn ::shape::Shape) -> geo::Box {
    s.GetBox().get()
}

impl shapeCallout {
    pub(crate) fn snapPointPercentages(&self) -> Vec<Vec<RelativePointRef>> {
        let b = bx(&*self.Shape);
        let width = b.Width;
        let height = b.Height;
        let mut tipWidth = 30.0;
        let mut tipHeight = 45.0;
        if width < tipWidth * 2.0 {
            tipWidth = width / 2.0;
        }
        if height < tipHeight * 2.0 {
            tipHeight = height / 2.0;
        }
        vec![
            vec![rp(0.25, 0.0), rp(0.5, 0.0), rp(0.75, 0.0)],
            vec![
                rp(0.0, ((height - tipHeight) / height) * 0.25),
                rp(0.0, ((height - tipHeight) / height) * 0.5),
                rp(0.0, ((height - tipHeight) / height) * 0.75),
            ],
            vec![
                // Go constants 0.5*0.33 and 0.5*0.66 (exact)
                rp(f64::from_bits(0x3fc51eb851eb851f), (height - tipHeight) / height),
                rp(f64::from_bits(0x3fd51eb851eb851f), (height - tipHeight) / height),
                rp(0.5, 1.0),
                rp(1.0 - ((width / 2.0 - tipWidth) / width) * 0.5, (height - tipHeight) / height),
            ],
            vec![
                rp(1.0, ((height - tipHeight) / height) * 0.25),
                rp(1.0, ((height - tipHeight) / height) * 0.5),
                rp(1.0, ((height - tipHeight) / height) * 0.75),
            ],
        ]
    }
}

impl shapeCylinder {
    pub(crate) fn snapPointPercentages(&self) -> Vec<Vec<RelativePointRef>> {
        let b = bx(&*self.Shape);
        let width = b.Width;
        let height = b.Height;
        let mut arcDepth = 24.0;
        let controlPointsMultiplier = 0.45;
        if height < arcDepth * 2.0 {
            arcDepth = height / 2.0;
        }
        let topLeftMidpoint = NewBezierCurve(&[
            NewPoint(0.0, arcDepth),
            NewPoint(0.0, 0.0),
            NewPoint(width * controlPointsMultiplier, 0.0),
            NewPoint(width / 2.0, 0.0),
        ])
        .At(0.5)
        .get();
        let topRightMidpoint = NewBezierCurve(&[
            NewPoint(width / 2.0, 0.0),
            NewPoint(width - (width * controlPointsMultiplier), 0.0),
            NewPoint(width, 0.0),
            NewPoint(width, arcDepth),
        ])
        .At(0.5)
        .get();
        let bottomRightMidpoint = NewBezierCurve(&[
            NewPoint(width, height - arcDepth),
            NewPoint(width, height),
            NewPoint(width - width * controlPointsMultiplier, height),
            NewPoint(width / 2.0, height),
        ])
        .At(0.5)
        .get();
        let bottomLeftMidpoint = NewBezierCurve(&[
            NewPoint(width / 2.0, height),
            NewPoint(width * controlPointsMultiplier, height),
            NewPoint(0.0, height),
            NewPoint(0.0, height - arcDepth),
        ])
        .At(0.5)
        .get();
        vec![
            vec![
                rp(topLeftMidpoint.X / width, topLeftMidpoint.Y / height),
                rp(0.5, 0.0),
                rp(topRightMidpoint.X / width, topRightMidpoint.Y / height),
            ],
            vec![
                rp(0.0, arcDepth / height + ((height - arcDepth * 2.0) / height) * 0.25),
                rp(0.0, arcDepth / height + ((height - arcDepth * 2.0) / height) * 0.5),
                rp(0.0, arcDepth / height + ((height - arcDepth * 2.0) / height) * 0.75),
            ],
            vec![
                rp(bottomLeftMidpoint.X / width, bottomLeftMidpoint.Y / height),
                rp(0.5, 1.0),
                rp(bottomRightMidpoint.X / width, bottomRightMidpoint.Y / height),
            ],
            vec![
                rp(1.0, arcDepth / height + ((height - arcDepth * 2.0) / height) * 0.25),
                rp(1.0, arcDepth / height + ((height - arcDepth * 2.0) / height) * 0.5),
                rp(1.0, arcDepth / height + ((height - arcDepth * 2.0) / height) * 0.75),
            ],
        ]
    }
}

impl shapeDocument {
    pub(crate) fn snapPointPercentages(&self) -> Vec<Vec<RelativePointRef>> {
        let b = bx(&*self.Shape);
        let width = b.Width;
        let height = b.Height;
        // Go constants 16.3/18.925, 19.8/18.925, 12.8/18.925 (exactly folded)
        let c163 = f64::from_bits(0x3feb8fb9a8c015a5);
        let c198 = f64::from_bits(0x3ff0bd610e8aa70f);
        let c128 = f64::from_bits(0x3fe5a4b1346add2b);
        let bottomLeftMidpoint = NewBezierCurve(&[
            NewPoint(0.0, height * c163),
            NewPoint(width / 6.0, height * c198),
            NewPoint(width / 3.0, height * c198),
            NewPoint(width / 2.0, height * c163),
        ])
        .At(0.5)
        .get();
        let bottomRightCurve = NewBezierCurve(&[
            NewPoint(width / 2.0, height * c163),
            NewPoint(width * 2.0 / 3.0, height * c128),
            NewPoint(width * 5.0 / 6.0, height * c128),
            NewPoint(width, height * c163),
        ]);
        let a0 = bottomRightCurve.At(0.0).get();
        let a0x = a0.X / width;
        let a0b = bottomRightCurve.At(0.0).get();
        let a0y = a0b.Y / height;
        let a5 = bottomRightCurve.At(0.5).get();
        let a5x = a5.X / width;
        let a5b = bottomRightCurve.At(0.5).get();
        let a5y = a5b.Y / height;
        vec![
            vec![rp(0.25, 0.0), rp(0.5, 0.0), rp(0.75, 0.0)],
            vec![rp(0.0, 0.25), rp(0.0, 0.5), rp(0.0, 0.75)],
            vec![rp(bottomLeftMidpoint.X / width, bottomLeftMidpoint.Y / height), rp(a0x, a0y), rp(a5x, a5y)],
            vec![rp(1.0, 0.25), rp(1.0, 0.5), rp(1.0, 0.75)],
        ]
    }
}

impl shapePackage {
    pub(crate) fn snapPointPercentages(&self) -> Vec<Vec<RelativePointRef>> {
        let b = bx(&*self.Shape);
        let width = b.Width;
        let height = b.Height;
        let verticalScalar = 0.2;
        let horizontalScalar = 0.5;
        let minTopHeight = 34.0;
        let maxTopHeight = 55.0;
        let minTopWidth = 50.0;
        let maxTopWidth = 150.0;
        let mut topWidth = math::Min(maxTopWidth, math::Max(minTopWidth, width * horizontalScalar));
        if width < 2.0 * minTopWidth {
            topWidth = width * horizontalScalar;
        }
        let mut topHeight = math::Min(maxTopHeight, math::Max(minTopHeight, height * verticalScalar));
        if height < 2.0 * minTopHeight {
            topHeight = height * verticalScalar;
        }
        let topWidthRatio = topWidth / width;
        let topHeightRatio = topHeight / height;
        let negativeTopWidthRatio = 1.0 - topWidthRatio;
        let negativeTopHeightRatio = 1.0 - topHeightRatio;
        vec![
            vec![
                rp(topWidthRatio * 0.33, 0.0),
                rp(topWidthRatio * 0.66, 0.0),
                rp(topWidthRatio + negativeTopWidthRatio * 0.5, topHeightRatio),
            ],
            vec![
                rp(0.0, topHeightRatio + negativeTopHeightRatio * 0.25),
                rp(0.0, topHeightRatio + negativeTopHeightRatio * 0.5),
                rp(0.0, topHeightRatio + negativeTopHeightRatio * 0.75),
            ],
            vec![rp(0.25, 1.0), rp(0.5, 1.0), rp(0.75, 1.0)],
            vec![
                rp(1.0, topHeightRatio + negativeTopHeightRatio * 0.25),
                rp(1.0, topHeightRatio + negativeTopHeightRatio * 0.5),
                rp(1.0, topHeightRatio + negativeTopHeightRatio * 0.75),
            ],
        ]
    }
}

impl shapeParallelogram {
    pub(crate) fn snapPointPercentages(&self) -> Vec<Vec<RelativePointRef>> {
        let width = bx(&*self.Shape).Width;
        let mut wedgeWidth = 26.0;
        if width < wedgeWidth {
            wedgeWidth = width / 2.0;
        }
        vec![
            vec![
                rp(wedgeWidth / width + ((width - wedgeWidth) / width) * 0.25, 0.0),
                rp(wedgeWidth / width + ((width - wedgeWidth) / width) * 0.5, 0.0),
                rp(wedgeWidth / width + ((width - wedgeWidth) / width) * 0.75, 0.0),
            ],
            vec![
                rp((wedgeWidth / 2.0 + wedgeWidth) / 2.0 / width, 0.25),
                rp(wedgeWidth / 2.0 / width, 0.5),
                rp(wedgeWidth / 2.0 / 2.0 / width, 0.75),
            ],
            vec![
                rp(((width - wedgeWidth) / width) * 0.25, 1.0),
                rp(((width - wedgeWidth) / width) * 0.5, 1.0),
                rp(((width - wedgeWidth) / width) * 0.75, 1.0),
            ],
            vec![
                rp((width - wedgeWidth) / width + (wedgeWidth / 2.0 + wedgeWidth) / 2.0 / width, 0.25),
                rp((width - wedgeWidth) / width + wedgeWidth / 2.0 / width, 0.5),
                rp((width - wedgeWidth) / width + wedgeWidth / 2.0 / 2.0 / width, 0.75),
            ],
        ]
    }
}

impl shapeQueue {
    pub(crate) fn snapPointPercentages(&self) -> Vec<Vec<RelativePointRef>> {
        let b = bx(&*self.Shape);
        let width = b.Width;
        let height = b.Height;
        let mut arcDepth = 24.0;
        let controlPointsMultiplier = 0.45;
        if width < arcDepth * 2.0 {
            arcDepth = width / 2.0;
        }
        let topLeftMidpoint = NewBezierCurve(&[
            NewPoint(arcDepth, 0.0),
            NewPoint(0.0, 0.0),
            NewPoint(0.0, height * controlPointsMultiplier),
            NewPoint(0.0, height / 2.0),
        ])
        .At(0.5)
        .get();
        let topRightMidpoint = NewBezierCurve(&[
            NewPoint(width - arcDepth, 0.0),
            NewPoint(width, 0.0),
            NewPoint(width, height * controlPointsMultiplier),
            NewPoint(width, height / 2.0),
        ])
        .At(0.5)
        .get();
        let bottomRightMidpoint = NewBezierCurve(&[
            NewPoint(width, height / 2.0),
            NewPoint(width, height - height * controlPointsMultiplier),
            NewPoint(width, height),
            NewPoint(width - arcDepth, height),
        ])
        .At(0.5)
        .get();
        let bottomLeftMidpoint = NewBezierCurve(&[
            NewPoint(0.0, height / 2.0),
            NewPoint(0.0, height - height * controlPointsMultiplier),
            NewPoint(0.0, height),
            NewPoint(arcDepth, height),
        ])
        .At(0.5)
        .get();
        vec![
            vec![
                rp(arcDepth / width + ((width - arcDepth * 2.0) / width) * 0.25, 0.0),
                rp(arcDepth / width + ((width - arcDepth * 2.0) / width) * 0.5, 0.0),
                rp(arcDepth / width + ((width - arcDepth * 2.0) / width) * 0.75, 0.0),
            ],
            vec![
                rp(topLeftMidpoint.X / width, topLeftMidpoint.Y / height),
                rp(0.0, 0.5),
                rp(bottomLeftMidpoint.X / width, bottomLeftMidpoint.Y / height),
            ],
            vec![
                rp(arcDepth / width + ((width - arcDepth * 2.0) / width) * 0.25, 1.0),
                rp(arcDepth / width + ((width - arcDepth * 2.0) / width) * 0.5, 1.0),
                rp(arcDepth / width + ((width - arcDepth * 2.0) / width) * 0.75, 1.0),
            ],
            vec![
                rp(topRightMidpoint.X / width, topRightMidpoint.Y / height),
                rp(1.0, 0.5),
                rp(bottomRightMidpoint.X / width, bottomRightMidpoint.Y / height),
            ],
        ]
    }
}

impl shapeStep {
    pub(crate) fn snapPointPercentages(&self) -> Vec<Vec<RelativePointRef>> {
        let width = bx(&*self.Shape).Width;
        let mut wedgeWidth = ::shape::STEP_WEDGE_WIDTH;
        if width < wedgeWidth {
            wedgeWidth = width / 2.0;
        }
        vec![
            vec![
                rp(((width - wedgeWidth) / width) * 0.25, 0.0),
                rp(((width - wedgeWidth) / width) * 0.5, 0.0),
                rp(((width - wedgeWidth) / width) * 0.75, 0.0),
            ],
            vec![rp(wedgeWidth / width / 2.0, 0.25), rp(wedgeWidth / width, 0.5), rp(wedgeWidth / width / 2.0, 0.75)],
            vec![
                rp(((width - wedgeWidth) / width) * 0.25, 1.0),
                rp(((width - wedgeWidth) / width) * 0.5, 1.0),
                rp(((width - wedgeWidth) / width) * 0.75, 1.0),
            ],
            vec![rp(1.0, 0.5)],
        ]
    }
}

impl shapeStoredData {
    pub(crate) fn snapPointPercentages(&self) -> Vec<Vec<RelativePointRef>> {
        let b = bx(&*self.Shape);
        let width = b.Width;
        let height = b.Height;
        let mut wedgeWidth = 15.0;
        if width < wedgeWidth {
            wedgeWidth = width / 2.0;
        }
        let controlPointsMultiplier = 0.27;
        let topLeftMidpoint = NewBezierCurve(&[
            NewPoint(wedgeWidth, 0.0),
            NewPoint(wedgeWidth - (wedgeWidth * controlPointsMultiplier), 0.0),
            NewPoint(0.0, height * controlPointsMultiplier),
            NewPoint(0.0, height / 2.0),
        ])
        .At(0.5)
        .get();
        let bottomLeftMidpoint = NewBezierCurve(&[
            NewPoint(wedgeWidth, height),
            NewPoint(wedgeWidth - (wedgeWidth * controlPointsMultiplier), height),
            NewPoint(0.0, height - height * controlPointsMultiplier),
            NewPoint(0.0, height / 2.0),
        ])
        .At(0.5)
        .get();
        let topRightMidpoint = NewBezierCurve(&[
            NewPoint(width, 0.0),
            NewPoint(width - (wedgeWidth * controlPointsMultiplier), 0.0),
            NewPoint(width - wedgeWidth, height * controlPointsMultiplier),
            NewPoint(width - wedgeWidth, height / 2.0),
        ])
        .At(0.5)
        .get();
        let bottomRightMidpoint = NewBezierCurve(&[
            NewPoint(width - wedgeWidth, height / 2.0),
            NewPoint(width - wedgeWidth, height - height * controlPointsMultiplier),
            NewPoint(width - wedgeWidth * controlPointsMultiplier, height),
            NewPoint(width, height),
        ])
        .At(0.5)
        .get();
        let sideWidthPercentage = (width - wedgeWidth) / width;
        let sideWidthStartPercentage = 1.0 - sideWidthPercentage;
        vec![
            vec![
                rp(sideWidthStartPercentage + (0.25 * sideWidthPercentage), 0.0),
                rp(sideWidthStartPercentage + (0.5 * sideWidthPercentage), 0.0),
                rp(sideWidthStartPercentage + (0.75 * sideWidthPercentage), 0.0),
            ],
            vec![
                rp(topLeftMidpoint.X / width, topLeftMidpoint.Y / height),
                rp(0.0, 0.5),
                rp(bottomLeftMidpoint.X / width, bottomLeftMidpoint.Y / height),
            ],
            vec![
                rp(sideWidthStartPercentage + (0.25 * sideWidthPercentage), 1.0),
                rp(sideWidthStartPercentage + (0.5 * sideWidthPercentage), 1.0),
                rp(sideWidthStartPercentage + (0.75 * sideWidthPercentage), 1.0),
            ],
            vec![
                rp(topRightMidpoint.X / width, topRightMidpoint.Y / height),
                rp((width - wedgeWidth) / width, 0.5),
                rp(bottomRightMidpoint.X / width, bottomRightMidpoint.Y / height),
            ],
        ]
    }
}
