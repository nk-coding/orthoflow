#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
#
# Copyright 2026 Niklas Krieger
#
# Generates crates/nodeshape/src/shape_*.rs (the table-like methods of TALA's
# internal/nodeshape shapes) from a terrastruct/d2 checkout:
#
#   scripts/gen_nodeshape.py <d2-checkout> [<out-dir>]
#
# <out-dir> defaults to crates/nodeshape/src. Computed SnapPointPercentages
# bodies are hand-ported in snap_manual.rs.
import re, os, sys
if len(sys.argv) not in (2, 3):
    sys.exit('usage: gen_nodeshape.py <d2-checkout> [<out-dir>]')
SRC = os.path.join(sys.argv[1], 'd2layouts/d2talalayout/internal/nodeshape')
OUT = sys.argv[2] if len(sys.argv) == 3 else os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'crates', 'nodeshape', 'src')

def header(go):
    return ('// This Source Code Form is subject to the terms of the Mozilla Public\n'
            '// License, v. 2.0. If a copy of the MPL was not distributed with this\n'
            '// file, You can obtain one at https://mozilla.org/MPL/2.0/.\n'
            '//\n'
            '// Ported from d2layouts/d2talalayout/internal/nodeshape/%s\n'
            '// (terrastruct/d2, Copyright 2022 Terrastruct Inc.).\n'
            '// Rust port: Copyright 2026 Niklas Krieger.\n'
            '//\n') % go

def func_body(src, recv_type, name):
    m = re.search(r'func \(s \*?%s\) %s\(([^)]*)\)[^\n]*\{\n' % (recv_type, name), src)
    if not m: return None
    i = m.end(); depth = 1
    while depth:
        c = src[i]
        if c == '{': depth += 1
        elif c == '}': depth -= 1
        i += 1
    return src[m.end():i-1]

def gen_lpp(body):
    out = ['        match tier {']
    for tier in ['Good','OK','Unideal','Bad']:
        m = re.search(r'case %s:\s*return map\[label\.Position\]struct\{\}\{(.*?)\n\t\t\}|case %s:\s*return map\[label\.Position\]struct\{\}\{\}' % (tier,tier), body, re.S)
        items = re.findall(r'label\.(\w+):', m.group(1) or '') if m and m.group(1) else []
        out.append('            %s => set(&[%s]),' % (tier, ', '.join('label::'+x for x in items)))
    out.append('            _ => set(&[]),')
    out.append('        }')
    return '\n'.join(out)

def gen_ports(body):
    out = ['        match orientation {']
    for o, lst in re.findall(r'case geo\.(\w+):\s*return \[\]int\{([^}]*)\}', body):
        out.append('            geo::%s => vec![%s],' % (o, lst))
    out.append('            _ => vec![],')
    out.append('        }')
    return '\n'.join(out)

def gen_mirror(body):
    pairs = re.findall(r'(\d+):\s*(\d+)', body)
    return '        mapOf(&[%s])' % ', '.join('(%s, %s)' % p for p in pairs)

def gen_center(body):
    lst = re.search(r'return \[\]int\{([^}]*)\}', body).group(1)
    return '        vec![%s]' % lst

def gen_cpi(body):
    out = ['        let centerPorts = self.CenterPortIndices();', '        let centerPortsIndex: usize = match o {']
    for o, idx in re.findall(r'case geo\.(\w+):\s*centerPortsIndex = (\d+)', body):
        out.append('            geo::%s => %s,' % (o, idx))
    out.append('            _ => return -1,')
    out.append('        };')
    out.append('        centerPorts[centerPortsIndex]')
    return '\n'.join(out)

def lit(v):
    v = v.strip()
    if re.fullmatch(r'-?\d+', v): return v + '.0'
    if re.fullmatch(r'-?\d*\.\d*', v):
        if v.endswith('.'): v += '0'
        if v.startswith('.'): v = '0' + v
        return v
    return None

def gen_snap_literal(body):
    groups = re.findall(r'\{\s*((?:geo\.NewRelativePoint\([^)]*\),?\s*(?://[^\n]*)?\s*)+)\}', body)
    out = ['        vec![']
    for g in groups:
        pts = re.findall(r'geo\.NewRelativePoint\(([^,]+),([^)]+)\)', g)
        items = []
        for a, b in pts:
            la, lb = lit(a), lit(b)
            if la is None or lb is None: return None
            items.append('geo::NewRelativePoint(%s, %s)' % (la, lb))
        out.append('            vec![%s],' % ', '.join(items))
    out.append('        ]')
    if sum(o.count('NewRelativePoint') for o in out) != body.count('NewRelativePoint'):
        return None
    return '\n'.join(out)

TYPES = {
 'shape_callout.go':'shapeCallout','shape_circle.go':'shapeCircle','shape_cloud.go':'shapeCloud','shape_cylinder.go':'shapeCylinder',
 'shape_diamond.go':'shapeDiamond','shape_document.go':'shapeDocument','shape_hexagon.go':'shapeHexagon','shape_image.go':'shapeImage',
 'shape_oval.go':'shapeOval','shape_package.go':'shapePackage','shape_page.go':'shapePage','shape_parallelogram.go':'shapeParallelogram',
 'shape_person.go':'shapePerson','shape_c4person.go':'shapeC4Person','shape_queue.go':'shapeQueue','shape_step.go':'shapeStep',
 'shape_stored_data.go':'shapeStoredData','shape_square.go':'shapeSquare',
}
for fn, T in TYPES.items():
    src = open(os.path.join(SRC, fn)).read()
    methods = []
    def add(sig, body): methods.append('    %s {\n%s\n    }' % (sig, body))
    b = func_body(src, T, 'LabelPositionPreferences')
    if b: add('fn LabelPositionPreferences(&self, tier: LabelTier) -> Set<label::Position>', gen_lpp(b))
    else: add('fn LabelPositionPreferences(&self, tier: LabelTier) -> Set<label::Position>', '        shapeSquare_LabelPositionPreferences(tier)')
    b = func_body(src, T, 'SnapPointPercentages')
    snap = None
    if b:
        snap = gen_snap_literal(b)
        if snap is None:
            snap = '        self.snapPointPercentages()'
        add('fn SnapPointPercentages(&self) -> Vec<Vec<geo::RelativePointRef>>', snap)
    else: add('fn SnapPointPercentages(&self) -> Vec<Vec<geo::RelativePointRef>>', '        shapeSquare_SnapPointPercentages()')
    b = func_body(src, T, 'PortIndices')
    add('fn PortIndices(&self, orientation: geo::Orientation) -> Vec<i64>', gen_ports(b) if b else '        shapeSquare_PortIndices(orientation)')
    b = func_body(src, T, 'MirroredPortIndices')
    add('fn MirroredPortIndices(&self) -> Map<i64, i64>', gen_mirror(b) if b else '        shapeSquare_MirroredPortIndices()')
    b = func_body(src, T, 'CenterPortIndices')
    add('fn CenterPortIndices(&self) -> Vec<i64>', gen_center(b) if b else '        shapeSquare_CenterPortIndices()')
    b = func_body(src, T, 'CenterPortIndex')
    add('fn CenterPortIndex(&self, o: geo::Orientation) -> i64', gen_cpi(b) if b else '        shapeSquare_CenterPortIndex(o)')
    if T == 'shapeSquare':
        # free functions for promoted shapeSquare methods
        free = []
        for m in methods:
            sig = m.split('{')[0].strip()
            name = re.search(r'fn (\w+)\(', sig).group(1)
            fsig = sig.replace('fn %s(&self, ' % name, 'pub(crate) fn shapeSquare_%s(' % name).replace('fn %s(&self)' % name, 'pub(crate) fn shapeSquare_%s()' % name)
            body = m[m.index('{')+1:m.rindex('}')]
            body = body.replace('self.CenterPortIndices()', 'shapeSquare_CenterPortIndices()')
            free.append(fsig + ' {' + body + '}')
        open(os.path.join(OUT, 'shape_square_gen.rs'), 'w').write(header('shape_square.go') + '// Generated by scripts/gen_nodeshape.py.\nuse crate::*;\n\n' + '\n\n'.join(free) + '\n')
        continue
    rs = header(fn) + '// Generated by scripts/gen_nodeshape.py from %s (do not edit by hand;\n// hand-written parts live in %s_manual.rs when present).\nuse crate::*;\n\n' % (fn, fn[:-3])
    rs += '/// Go `%s` (wraps a D2 `shape.Shape`).\npub struct %s {\n    pub Shape: ::shape::ShapeRc,\n}\n\n' % (T, T)
    rs += 'impl ::shape::Shape for %s {\n    ::shape::delegate_shape_methods!(Shape);\n}\n\n' % T
    rs += 'impl Shape for %s {\n%s\n}\n' % (T, '\n'.join(methods))
    open(os.path.join(OUT, fn[:-3] + '.rs'), 'w').write(rs)
    if 'self.snapPointPercentages()' in rs: print('manual snap needed:', fn)
