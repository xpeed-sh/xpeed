"""Compile the supplied logo's relative paths and letter holes; build time only.
Requires numpy and mapbox-earcut (pip install -r scripts/svg-build-requirements.txt).
The supplied transform is applied before trimming the logo's transparent margins.
"""
import re
import sys
import xml.etree.ElementTree as ET


def contours(data):
    tokens = re.findall(r'[A-Za-z]|[-+]?(?:\d*\.\d+|\d+)(?:[eE][-+]?\d+)?', data)
    result = []
    points = []
    cursor = (0.0, 0.0)
    start = cursor
    index = 0
    command = None
    while index < len(tokens):
        if tokens[index].isalpha():
            command = tokens[index]
            index += 1
        absolute = command.isupper()
        kind = command.upper()
        if kind == 'Z':
            if points[-1] == points[0]:
                points.pop()
            result.append(points)
            points = []
            cursor = start
            command = None
            continue
        count = {'M': 2, 'L': 2, 'C': 6}.get(kind)
        if count is None:
            raise ValueError(f'Unsupported logo command: {command}')
        values = list(map(float, tokens[index:index+count]))
        index += count
        pairs = [tuple(values[i:i+2]) for i in range(0, count, 2)]
        if not absolute:
            pairs = [(x+cursor[0], y+cursor[1]) for x,y in pairs]
        if kind in ('M', 'L'):
            cursor = pairs[0]
            points.append(cursor)
            if kind == 'M':
                start = cursor
                command = 'L' if absolute else 'l'
        else:
            a = cursor
            b,c,d = pairs
            for step in range(1, 13):
                t = step/12
                u = 1-t
                points.append(tuple(u**3*a[k]+3*u*u*t*b[k]+3*u*t*t*c[k]+t**3*d[k] for k in (0,1)))
            cursor = d
    if points:
        raise ValueError('Logo path must be closed')
    return result


def compile_logo(root):
    # Local build helpers are optional for normal Cargo builds; meshes are checked in.
    sys.path.insert(0, str(root/'.tools/svg-build'))
    import numpy as np
    import mapbox_earcut
    svg = ET.parse(root/'assets/icons/logo.svg')
    group = svg.find('{http://www.w3.org/2000/svg}g')
    if group.attrib.get('transform') != 'translate(0,724) scale(0.1,-0.1)':
        raise ValueError('Unsupported logo transform')
    paths = [contours(path.attrib['d']) for path in group]
    paths = [[[(x*0.1, 724-y*0.1) for x,y in ring] for ring in path] for path in paths]
    all_points = [p for path in paths for ring in path for p in ring]
    left = min(x for x,y in all_points)
    top = min(y for x,y in all_points)
    width = max(x for x,y in all_points)-left
    height = max(y for x,y in all_points)-top
    output = [f'pub const LOGO_ASPECT: f32 = {width/height:.6f};', 'pub const LOGO: &[Layer] = &[']
    for path in paths:
        # Each supplied path is a glyph: first ring is outer, remaining rings are holes.
        points = [(24*(x-left)/width, 24*(y-top)/height) for ring in path for x,y in ring]
        ends = np.cumsum([len(ring) for ring in path], dtype=np.uint32)
        indices = mapbox_earcut.triangulate_float64(np.array(points, dtype=np.float64), ends).tolist()
        if not indices:
            raise ValueError('Logo triangulation produced no triangles')
        vertices = ', '.join(f'[{x:.6f}, {y:.6f}]' for x,y in points)
        output.append(f'(1.0, &[{vertices}], &{indices}),')
    output.append('];')
    svg.getroot().set('viewBox', f'{left:.6f} {top:.6f} {width:.6f} {height:.6f}')
    ET.register_namespace('', 'http://www.w3.org/2000/svg')
    svg.write(root/'assets/icons/logo.svg', encoding='unicode')
    return output
