"""Builds LarkAura's winter visitors in Blender and renders them as sprites.

Models: the sleigh and its driver, a reindeer with an eight-frame flying
gallop, the snowplough and its driver (who waves), the plough's wheel, and
three small snow visitors: a penguin in a knitted scarf, a red fox that
dives into the snow, and a snowy owl that brings a tiny present. Everything
is made from code, so the art can be rebuilt and tweaked without hand edits.
All of them share one look: soft, rounded forms, glossy ink eyes with a
catchlight, the LarkAura palette, warm key light and ink outlines.

    blender -b --factory-startup --python art/visitors/build.py -- \
        --out crates/aura-effects/assets [--samples 96] [--only sleigh,deer] \
        [--frames DIR] [--blend art/visitors/visitors.blend]

Writes `visitors.png` (the atlas, straight alpha) to --out and
`crates/aura-effects/src/sprites/table.rs` (frame rectangles and anchors).
--frames keeps every frame as its own PNG for checking by eye.
"""

import bpy
import bmesh
import json
import math
import os
import sys
from mathutils import Matrix, Quaternion, Vector

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))

# Atlas pixels per Blender unit. The engine draws sprites at 1 / SCALE of
# their atlas size, which keeps them sharp after the renderer's mipmaps.
PPU = 80.0
SCALE = 1.5
MARGIN = 6
# Each cell is padded and aligned so four mip levels never bleed together.
ALIGN = 8

# Camera: a little in front of the side view and a little above, so the
# models read as solid objects while they travel left to right.
YAW = math.radians(17)
PITCH = math.radians(9)


def parse_args():
    argv = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
    opts = {"out": os.path.join(REPO, "crates/aura-effects/assets"), "samples": 96,
            "only": None, "frames": None, "blend": os.path.join(HERE, "visitors.blend")}
    i = 0
    while i < len(argv):
        key = argv[i].lstrip("-")
        opts[key] = argv[i + 1]
        i += 2
    opts["samples"] = int(opts["samples"])
    if opts["only"]:
        opts["only"] = set(opts["only"].split(","))
    return opts


# ---------------------------------------------------------------- materials

def lin(c):
    """sRGB display colour to the linear values Blender's shaders expect."""
    def one(v):
        return v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4
    return (one(c[0]), one(c[1]), one(c[2]), 1.0)


# LarkAura's palette (see crates/aura-effects/src/scenery.rs), with a few
# companions for skin, fur and metal.
INK = (0.15, 0.13, 0.15)
BERRY = (0.64, 0.10, 0.14)
BERRY_DEEP = (0.52, 0.09, 0.13)
ORANGE = (0.93, 0.42, 0.17)
GOLD = (1.0, 0.78, 0.38)
SAND = (0.95, 0.86, 0.74)
CREAM = (0.98, 0.95, 0.89)
SNOW = (0.97, 0.98, 1.0)
PINE = (0.17, 0.38, 0.29)
SKIN = (0.99, 0.80, 0.68)
ROSY = (0.96, 0.52, 0.50)
LEATHER = (0.40, 0.24, 0.16)
DEER = (0.58, 0.38, 0.25)
DEER_BELLY = (0.90, 0.78, 0.62)
ANTLER = (0.90, 0.80, 0.64)
BURLAP = (0.80, 0.64, 0.44)
STEEL = (0.42, 0.44, 0.48)
FOX = (0.90, 0.45, 0.17)
FOX_DEEP = (0.70, 0.30, 0.11)
FOX_SOCK = (0.24, 0.15, 0.13)
PENGUIN = (0.13, 0.15, 0.21)
OWL = (0.97, 0.96, 0.93)
OWL_BAR = (0.42, 0.37, 0.34)
OWL_EYE = (1.0, 0.78, 0.18)

_materials = {}


def principled(m):
    nodes = m.node_tree.nodes
    for n in nodes:
        if n.type == "BSDF_PRINCIPLED":
            return n
    return nodes.new("ShaderNodeBsdfPrincipled")


def new_material(name):
    m = bpy.data.materials.new(name)
    if m.node_tree is None:
        m.use_nodes = True
    return m


def mat(name, color, rough=0.5, metal=0.0, coat=0.0, sheen=0.0, sss=0.0,
        emit=None, strength=0.0, bump=0.0, bump_scale=60.0):
    if name in _materials:
        return _materials[name]
    m = new_material(name)
    b = principled(m)
    b.inputs["Base Color"].default_value = lin(color)
    b.inputs["Roughness"].default_value = rough
    b.inputs["Metallic"].default_value = metal
    b.inputs["Coat Weight"].default_value = coat
    b.inputs["Coat Roughness"].default_value = 0.08
    b.inputs["Sheen Weight"].default_value = sheen
    b.inputs["Sheen Roughness"].default_value = 0.4
    if sss:
        b.inputs["Subsurface Weight"].default_value = sss
        b.inputs["Subsurface Radius"].default_value = (0.25, 0.08, 0.05)
        b.inputs["Subsurface Scale"].default_value = 0.08
    if emit:
        b.inputs["Emission Color"].default_value = lin(emit)
        b.inputs["Emission Strength"].default_value = strength
    if bump:
        add_bump(m, b, bump, bump_scale)
    _materials[name] = m
    return m


def add_bump(m, bsdf, strength, scale):
    nt = m.node_tree
    tex = nt.nodes.new("ShaderNodeTexNoise")
    tex.inputs["Scale"].default_value = scale
    tex.inputs["Detail"].default_value = 6.0
    coord = nt.nodes.new("ShaderNodeTexCoord")
    nt.links.new(coord.outputs["Object"], tex.inputs["Vector"])
    bump = nt.nodes.new("ShaderNodeBump")
    bump.inputs["Strength"].default_value = strength
    bump.inputs["Distance"].default_value = 0.02
    nt.links.new(tex.outputs["Fac"], bump.inputs["Height"])
    nt.links.new(bump.outputs["Normal"], bsdf.inputs["Normal"])


def two_tone(name, top, bottom, split, soft, bump=0.0, sheen=0.3):
    """Fur that fades to a paler belly below `split` (object space z)."""
    if name in _materials:
        return _materials[name]
    m = new_material(name)
    nt = m.node_tree
    b = principled(m)
    b.inputs["Roughness"].default_value = 0.7
    b.inputs["Sheen Weight"].default_value = sheen
    coord = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(coord.outputs["Object"], sep.inputs["Vector"])
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].position = 0.5 - soft
    ramp.color_ramp.elements[0].color = lin(bottom)
    ramp.color_ramp.elements[1].position = 0.5 + soft
    ramp.color_ramp.elements[1].color = lin(top)
    shift = nt.nodes.new("ShaderNodeMath")
    shift.operation = "ADD"
    shift.inputs[1].default_value = 0.5 - split
    nt.links.new(sep.outputs["Z"], shift.inputs[0])
    nt.links.new(shift.outputs[0], ramp.inputs["Fac"])
    nt.links.new(ramp.outputs["Color"], b.inputs["Base Color"])
    if bump:
        add_bump(m, b, bump, 70.0)
    _materials[name] = m
    return m


def stripes(name, a, b, freq, rough=0.45):
    """Diagonal hazard stripes across a plough blade."""
    if name in _materials:
        return _materials[name]
    m = new_material(name)
    nt = m.node_tree
    bsdf = principled(m)
    bsdf.inputs["Roughness"].default_value = rough
    bsdf.inputs["Coat Weight"].default_value = 0.4
    coord = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(coord.outputs["Object"], sep.inputs["Vector"])
    add = nt.nodes.new("ShaderNodeMath")
    add.operation = "ADD"
    nt.links.new(sep.outputs["Y"], add.inputs[0])
    nt.links.new(sep.outputs["Z"], add.inputs[1])
    mul = nt.nodes.new("ShaderNodeMath")
    mul.operation = "MULTIPLY"
    mul.inputs[1].default_value = freq
    nt.links.new(add.outputs[0], mul.inputs[0])
    frac = nt.nodes.new("ShaderNodeMath")
    frac.operation = "FRACT"
    nt.links.new(mul.outputs[0], frac.inputs[0])
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.interpolation = "CONSTANT"
    ramp.color_ramp.elements[0].position = 0.0
    ramp.color_ramp.elements[0].color = lin(a)
    ramp.color_ramp.elements[1].position = 0.5
    ramp.color_ramp.elements[1].color = lin(b)
    nt.links.new(frac.outputs[0], ramp.inputs["Fac"])
    nt.links.new(ramp.outputs["Color"], bsdf.inputs["Base Color"])
    _materials[name] = m
    return m


def barred(name, base, bar, scale=9.0, amount=0.55, spot=0.22):
    """Snowy owl plumage: soft white with short dark bars, densest on the
    back and wings and fading out toward the face and breast (object +x)."""
    if name in _materials:
        return _materials[name]
    m = new_material(name)
    nt = m.node_tree
    b = principled(m)
    b.inputs["Roughness"].default_value = 0.85
    b.inputs["Sheen Weight"].default_value = 0.8
    b.inputs["Sheen Roughness"].default_value = 0.35
    coord = nt.nodes.new("ShaderNodeTexCoord")
    voro = nt.nodes.new("ShaderNodeTexVoronoi")
    voro.feature = "F1"
    voro.inputs["Scale"].default_value = scale
    mapping = nt.nodes.new("ShaderNodeMapping")
    mapping.inputs["Scale"].default_value = (1.0, 1.0, 2.6)
    nt.links.new(coord.outputs["Object"], mapping.inputs["Vector"])
    nt.links.new(mapping.outputs["Vector"], voro.inputs["Vector"])
    spots = nt.nodes.new("ShaderNodeValToRGB")
    spots.color_ramp.elements[0].position = 0.10
    spots.color_ramp.elements[0].color = (1, 1, 1, 1)
    spots.color_ramp.elements[1].position = spot
    spots.color_ramp.elements[1].color = (0, 0, 0, 1)
    nt.links.new(voro.outputs["Distance"], spots.inputs["Fac"])
    # Fewer bars toward the front.
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(coord.outputs["Object"], sep.inputs["Vector"])
    front = nt.nodes.new("ShaderNodeMapRange")
    front.inputs["From Min"].default_value = 0.20
    front.inputs["From Max"].default_value = -0.10
    nt.links.new(sep.outputs["X"], front.inputs["Value"])
    mask = nt.nodes.new("ShaderNodeMath")
    mask.operation = "MULTIPLY"
    nt.links.new(spots.outputs["Color"], mask.inputs[0])
    nt.links.new(front.outputs["Result"], mask.inputs[1])
    amt = nt.nodes.new("ShaderNodeMath")
    amt.operation = "MULTIPLY"
    amt.inputs[1].default_value = amount
    nt.links.new(mask.outputs[0], amt.inputs[0])
    mix = nt.nodes.new("ShaderNodeMix")
    mix.data_type = "RGBA"
    mix.inputs["A"].default_value = lin(base)
    mix.inputs["B"].default_value = lin(bar)
    nt.links.new(amt.outputs[0], mix.inputs["Factor"])
    nt.links.new(mix.outputs["Result"], b.inputs["Base Color"])
    add_bump(m, b, 0.35, 90.0)
    _materials[name] = m
    return m


def knit_stripes(name, a, b, freq):
    """A knitted scarf in two colours, striped along its length (object x)."""
    if name in _materials:
        return _materials[name]
    m = new_material(name)
    nt = m.node_tree
    bsdf = principled(m)
    bsdf.inputs["Roughness"].default_value = 0.9
    bsdf.inputs["Sheen Weight"].default_value = 0.6
    coord = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(coord.outputs["Object"], sep.inputs["Vector"])
    add = nt.nodes.new("ShaderNodeMath")
    add.operation = "ADD"
    nt.links.new(sep.outputs["X"], add.inputs[0])
    nt.links.new(sep.outputs["Z"], add.inputs[1])
    mul = nt.nodes.new("ShaderNodeMath")
    mul.operation = "MULTIPLY"
    mul.inputs[1].default_value = freq
    nt.links.new(add.outputs[0], mul.inputs[0])
    frac = nt.nodes.new("ShaderNodeMath")
    frac.operation = "FRACT"
    nt.links.new(mul.outputs[0], frac.inputs[0])
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.interpolation = "CONSTANT"
    ramp.color_ramp.elements[0].position = 0.0
    ramp.color_ramp.elements[0].color = lin(a)
    ramp.color_ramp.elements[1].position = 0.6
    ramp.color_ramp.elements[1].color = lin(b)
    nt.links.new(frac.outputs[0], ramp.inputs["Fac"])
    nt.links.new(ramp.outputs["Color"], bsdf.inputs["Base Color"])
    add_bump(m, bsdf, 0.7, 200.0)
    _materials[name] = m
    return m


def glass(name, tint=(0.75, 0.86, 0.95), reflect=0.18):
    """Thin cab glass: mostly see-through, with a cool sheen."""
    if name in _materials:
        return _materials[name]
    m = new_material(name)
    nt = m.node_tree
    out = next(n for n in nt.nodes if n.type == "OUTPUT_MATERIAL")
    for n in list(nt.nodes):
        if n.type == "BSDF_PRINCIPLED":
            nt.nodes.remove(n)
    clear = nt.nodes.new("ShaderNodeBsdfTransparent")
    clear.inputs["Color"].default_value = lin(tint)
    gloss = nt.nodes.new("ShaderNodeBsdfGlossy")
    gloss.inputs["Roughness"].default_value = 0.08
    mix = nt.nodes.new("ShaderNodeMixShader")
    mix.inputs["Fac"].default_value = reflect
    nt.links.new(clear.outputs[0], mix.inputs[1])
    nt.links.new(gloss.outputs[0], mix.inputs[2])
    nt.links.new(mix.outputs[0], out.inputs["Surface"])
    _materials[name] = m
    return m


def materials():
    return {
        "lacquer": mat("Lacquer", BERRY, rough=0.28, coat=1.0),
        "deer_leg": mat("DeerLeg", (0.42, 0.27, 0.18), rough=0.7, sheen=0.3, bump=0.3),
        "muzzle": mat("Muzzle", DEER_BELLY, rough=0.7, sheen=0.4, bump=0.2),
        "deer_head": mat("DeerHead", DEER, rough=0.7, sheen=0.3, bump=0.35),
        "cloth": mat("Cloth", (0.66, 0.10, 0.14), rough=0.85, bump=0.2),
        "lacquer_deep": mat("LacquerDeep", BERRY_DEEP, rough=0.35, coat=0.8),
        "gold": mat("Gold", GOLD, rough=0.26, metal=1.0),
        "velvet": mat("Velvet", (0.80, 0.13, 0.16), rough=0.75, sheen=0.9, bump=0.15),
        "cushion": mat("Cushion", SAND, rough=0.8, sheen=0.6, bump=0.1),
        "fur": mat("Fur", CREAM, rough=0.9, sheen=1.0, bump=0.6, bump_scale=120.0),
        "skin": mat("Skin", SKIN, rough=0.55, sss=0.25),
        "rosy": mat("Rosy", ROSY, rough=0.5, sss=0.2),
        "ink": mat("Ink", INK, rough=0.35, coat=0.5),
        "spark": mat("Catchlight", (1, 1, 1), rough=0.2, emit=(1, 1, 1), strength=4.0),
        "leather": mat("Leather", LEATHER, rough=0.5, coat=0.3),
        "burlap": mat("Burlap", BURLAP, rough=0.95, bump=0.8, bump_scale=160.0),
        "pine": mat("PineWrap", PINE, rough=0.4, coat=0.5),
        "orange": mat("OrangePaint", ORANGE, rough=0.35, coat=0.7),
        "cream": mat("CreamPaint", CREAM, rough=0.4, coat=0.5),
        "sand": mat("SandPaint", SAND, rough=0.45, coat=0.4),
        "lamp": mat("Lamp", (1.0, 0.82, 0.5), rough=0.2, emit=(1.0, 0.76, 0.42), strength=8.0),
        "beacon": mat("Beacon", (1.0, 0.62, 0.15), rough=0.2, emit=(1.0, 0.55, 0.12), strength=6.0),
        "deer": two_tone("DeerFur", DEER, DEER_BELLY, split=0.62, soft=0.05, bump=0.35),
        "antler": mat("Antler", ANTLER, rough=0.6, bump=0.3),
        "snow": mat("Snow", SNOW, rough=0.6, sss=0.3, sheen=0.5, bump=0.4, bump_scale=40.0),
        "rubber": mat("Rubber", (0.17, 0.16, 0.18), rough=0.8),
        "steel": mat("Steel", STEEL, rough=0.35, metal=1.0),
        "chrome": mat("Chrome", (0.86, 0.86, 0.88), rough=0.12, metal=1.0),
        "glass": glass("CabGlass", tint=(0.97, 0.98, 1.0), reflect=0.025),
        "hazard": stripes("Hazard", ORANGE, CREAM, freq=3.2),
        "knit": mat("Knit", PINE, rough=0.9, sheen=0.5, bump=0.7, bump_scale=200.0),
        "jacket": mat("Jacket", (0.27, 0.22, 0.24), rough=0.7, sheen=0.3, bump=0.2),
        "scarf": mat("Scarf", ORANGE, rough=0.9, sheen=0.6, bump=0.6, bump_scale=180.0),
        "lens": mat("Lens", (0.62, 0.80, 0.92), rough=0.05, coat=1.0),
        "fairy_gold": mat("FairyGold", GOLD, rough=0.2, emit=(1.0, 0.80, 0.45), strength=9.0),
        "fairy_red": mat("FairyRed", BERRY, rough=0.2, emit=(1.0, 0.25, 0.25), strength=7.0),
        "fairy_green": mat("FairyGreen", PINE, rough=0.2, emit=(0.4, 1.0, 0.55), strength=6.0),
        "holly": mat("Holly", (0.12, 0.36, 0.22), rough=0.35, coat=0.6),
        "berry": mat("Berry", (0.80, 0.10, 0.14), rough=0.25, coat=0.8),
        # The fox: rust fur with a cream bib and dark socks.
        "fox": two_tone("FoxFur", FOX, CREAM, split=0.36, soft=0.035, bump=0.45, sheen=0.6),
        "fox_head": mat("FoxHead", FOX, rough=0.75, sheen=0.6, bump=0.4, bump_scale=110.0),
        "fox_deep": mat("FoxDeep", FOX_DEEP, rough=0.75, sheen=0.5, bump=0.4),
        "fox_white": mat("FoxWhite", CREAM, rough=0.85, sheen=0.9, bump=0.5, bump_scale=120.0),
        "fox_sock": mat("FoxSock", FOX_SOCK, rough=0.7, sheen=0.4, bump=0.3),
        # The penguin: a deep blue-black coat and a soft white front.
        "penguin": mat("PenguinCoat", PENGUIN, rough=0.32, coat=0.5, sheen=0.3),
        "penguin_white": mat("PenguinFront", SNOW, rough=0.6, sheen=0.6, bump=0.15),
        "beak": mat("Beak", (0.98, 0.58, 0.16), rough=0.35, coat=0.6),
        "scarf_stripe": knit_stripes("ScarfStripe", BERRY, CREAM, freq=9.0),
        # The owl: snowy plumage with short dark bars, and golden eyes.
        "owl": barred("OwlPlumage", OWL, OWL_BAR, scale=11.0, amount=0.8),
        # Wings carry bolder, denser bars than the body, as a snowy owl's do.
        "owl_wing": barred("OwlWing", OWL, OWL_BAR, scale=8.0, amount=0.9, spot=0.34),
        "owl_face": mat("OwlFace", OWL, rough=0.9, sheen=1.0, bump=0.5, bump_scale=140.0),
        "owl_eye": mat("OwlEye", OWL_EYE, rough=0.15, coat=1.0),
        "owl_beak": mat("OwlBeak", (0.30, 0.28, 0.30), rough=0.3, coat=0.6),
        "talon": mat("Talon", (0.20, 0.18, 0.20), rough=0.3, coat=0.4),
    }


# ---------------------------------------------------------------- geometry

class Kit:
    """Creates objects inside one collection, optionally parented."""

    def __init__(self, name, parent=None):
        self.col = bpy.data.collections.new(name)
        bpy.context.scene.collection.children.link(self.col)
        self.parent = parent

    def link(self, obj, material=None, smooth=True):
        self.col.objects.link(obj)
        if self.parent is not None:
            obj.parent = self.parent
        if material is not None and hasattr(obj.data, "materials"):
            obj.data.materials.append(material)
        if smooth and obj.type == "MESH":
            obj.data.shade_smooth()
        return obj

    def empty(self, name, loc=(0, 0, 0)):
        e = bpy.data.objects.new(name, None)
        e.location = loc
        self.col.objects.link(e)
        if self.parent is not None:
            e.parent = self.parent
        return e

    def mesh_from(self, name, build, material, loc=(0, 0, 0), rot=(0, 0, 0), scale=(1, 1, 1)):
        me = bpy.data.meshes.new(name)
        bm = bmesh.new()
        build(bm)
        bm.to_mesh(me)
        bm.free()
        obj = bpy.data.objects.new(name, me)
        obj.location = loc
        obj.rotation_euler = rot
        obj.scale = scale
        return self.link(obj, material)

    def sphere(self, name, loc, radius, material, scale=(1, 1, 1), rot=(0, 0, 0), seg=40):
        def build(bm):
            bmesh.ops.create_uvsphere(bm, u_segments=seg, v_segments=seg // 2, radius=radius)
        return self.mesh_from(name, build, material, loc, rot, scale)

    def box(self, name, loc, size, material, bevel=0.06, rot=(0, 0, 0), subdiv=2):
        def build(bm):
            bmesh.ops.create_cube(bm, size=1.0)
            for v in bm.verts:
                v.co.x *= size[0]
                v.co.y *= size[1]
                v.co.z *= size[2]
        obj = self.mesh_from(name, build, material, loc, rot)
        if bevel > 0:
            m = obj.modifiers.new("Bevel", "BEVEL")
            m.width = bevel
            m.segments = 3
            m.limit_method = "NONE"
        if subdiv:
            s = obj.modifiers.new("Soft", "SUBSURF")
            s.levels = s.render_levels = subdiv
        return obj

    def cylinder(self, name, loc, radius, depth, material, rot=(0, 0, 0), seg=40, bevel=0.0):
        def build(bm):
            bmesh.ops.create_cone(bm, cap_ends=True, cap_tris=False, segments=seg,
                                  radius1=radius, radius2=radius, depth=depth)
        obj = self.mesh_from(name, build, material, loc, rot)
        if bevel:
            m = obj.modifiers.new("Bevel", "BEVEL")
            m.width = bevel
            m.segments = 3
            m.limit_method = "ANGLE"
        return obj

    def torus(self, name, loc, major, minor, material, rot=(0, 0, 0), seg=48):
        def build(bm):
            ring = []
            for i in range(seg):
                a = 2 * math.pi * i / seg
                row = []
                for j in range(16):
                    b = 2 * math.pi * j / 16
                    r = major + minor * math.cos(b)
                    row.append(bm.verts.new((r * math.cos(a), r * math.sin(a), minor * math.sin(b))))
                ring.append(row)
            for i in range(seg):
                for j in range(16):
                    a, b = ring[i], ring[(i + 1) % seg]
                    bm.faces.new((a[j], b[j], b[(j + 1) % 16], a[(j + 1) % 16]))
        return self.mesh_from(name, build, material, loc, rot)

    def tube(self, name, points, radius, material, radii=None, cyclic=False, res=10,
             caps=True):
        """A smooth tube through `points`. `radii` scales the radius per point."""
        cu = bpy.data.curves.new(name, "CURVE")
        cu.dimensions = "3D"
        cu.bevel_depth = radius
        cu.bevel_resolution = 5
        cu.resolution_u = res
        cu.use_fill_caps = caps
        sp = cu.splines.new("BEZIER")
        sp.bezier_points.add(len(points) - 1)
        for i, p in enumerate(points):
            bp = sp.bezier_points[i]
            bp.co = p
            bp.handle_left_type = bp.handle_right_type = "AUTO"
            bp.radius = radii[i] if radii else 1.0
        sp.use_cyclic_u = cyclic
        obj = bpy.data.objects.new(name, cu)
        return self.link(obj, material)

    def ring(self, name, center, rx, ry, radius, material, axis="z", n=16, squash=1.0):
        """A closed tube around an ellipse, e.g. a belt or a fur brim."""
        pts = []
        for i in range(n):
            a = 2 * math.pi * i / n
            u, v = rx * math.cos(a), ry * math.sin(a)
            if axis == "z":
                pts.append((center[0] + u, center[1] + v, center[2]))
            elif axis == "x":
                pts.append((center[0], center[1] + u, center[2] + v * squash))
            else:
                pts.append((center[0] + u, center[1], center[2] + v * squash))
        return self.tube(name, pts, radius, material, cyclic=True)

    def profile(self, name, pts, half_depth, bevel, material, y=0.0):
        """A filled side profile in the XZ plane, extruded across Y."""
        cu = bpy.data.curves.new(name, "CURVE")
        cu.dimensions = "2D"
        cu.fill_mode = "BOTH"
        cu.extrude = half_depth
        cu.bevel_depth = bevel
        cu.bevel_resolution = 4
        cu.resolution_u = 16
        sp = cu.splines.new("BEZIER")
        sp.bezier_points.add(len(pts) - 1)
        for i, (x, z) in enumerate(pts):
            bp = sp.bezier_points[i]
            bp.co = (x, z, 0)
            bp.handle_left_type = bp.handle_right_type = "AUTO"
        sp.use_cyclic_u = True
        obj = bpy.data.objects.new(name, cu)
        obj.rotation_euler = (math.pi / 2, 0, 0)
        obj.location = (0, y, 0)
        return self.link(obj, material)

    def blobs(self, name, balls, material, resolution=0.025, threshold=0.6):
        """Metaballs, for fluffy and organic shapes. `balls` holds
        (location, radius) or (location, radius, (sx, sy, sz))."""
        mb = bpy.data.metaballs.new(name)
        mb.resolution = mb.render_resolution = resolution
        mb.threshold = threshold
        for b in balls:
            el = mb.elements.new(type="ELLIPSOID" if len(b) > 2 else "BALL")
            el.co = b[0]
            el.radius = b[1]
            if len(b) > 2:
                el.size_x, el.size_y, el.size_z = b[2]
        obj = bpy.data.objects.new(name, mb)
        return self.link(obj, material, smooth=False)


def eye(kit, name, loc, r, look, m, blink=False):
    """A glossy ink eye with a catchlight. `look` points out of the face."""
    look = Vector(look).normalized()
    scale = (1, 1, 0.18) if blink else (1, 1, 1.15)
    kit.sphere(name, loc, r, m["ink"], scale=scale, seg=24)
    if not blink:
        spark = Vector(loc) + look * r * 0.75 + Vector((0, -0.35, 0.45)) * r
        kit.sphere(name + "Spark", spark, r * 0.32, m["spark"], seg=12)


# ---------------------------------------------------------------- the sleigh

SLEIGH_SIDE = [
    (-1.02, 0.36), (-0.20, 0.30), (0.62, 0.34), (0.98, 0.46), (1.18, 0.70),
    (1.24, 0.92), (1.14, 1.04), (1.00, 1.00), (0.98, 0.88), (1.04, 0.80),
    (0.92, 0.72), (0.60, 0.66), (-0.40, 0.68), (-0.70, 0.86), (-0.78, 1.18),
    (-0.90, 1.38), (-1.10, 1.44), (-1.26, 1.34), (-1.24, 1.18), (-1.12, 1.16),
    (-1.08, 1.02), (-1.12, 0.66),
]


def build_sleigh(m, pose):
    root = bpy.data.objects.new("SleighRoot", None)
    kit = Kit("Sleigh")
    kit.col.objects.link(root)
    kit.parent = root
    half = 0.5
    kit.profile("SleighBody", SLEIGH_SIDE, half, 0.07, m["lacquer"])
    # A darker inner tub, seen over the near rim from above.
    kit.box("SleighWell", (-0.15, 0, 0.62), (1.05, 0.86, 0.12), m["lacquer_deep"], bevel=0.04)

    # Gold piping traced along the top edge on both sides, and a swirl.
    top = [(-1.18, 1.22), (-1.10, 1.40), (-0.90, 1.36), (-0.78, 1.14), (-0.68, 0.84),
           (-0.40, 0.70), (0.60, 0.68), (0.92, 0.74), (1.02, 0.84)]
    bottom = [(-1.06, 0.48), (-0.20, 0.42), (0.60, 0.46), (0.94, 0.56), (1.10, 0.74)]
    for side in (-1, 1):
        y = side * (half + 0.075)
        kit.tube(f"PipingTop{side}", [(x, y, z - 0.04) for x, z in top], 0.026, m["gold"])
        kit.tube(f"PipingLow{side}", [(x, y, z) for x, z in bottom], 0.022, m["gold"])
    swirl = []
    for i in range(30):
        t = i / 29
        a = t * 4.6 * math.pi
        r = 0.04 + 0.16 * t
        swirl.append((0.18 + r * math.cos(a) * 1.5, -(half + 0.08), 0.52 + r * math.sin(a) * 0.7))
    kit.tube("Swirl", swirl, 0.016, m["gold"])
    for i, x in enumerate((-0.62, -0.22, 0.42)):
        kit.sphere(f"Stud{i}", (x, -(half + 0.08), 0.55), 0.035, m["gold"], seg=16)

    # Runners with a big curl at the front, joined to the body by struts.
    for side in (-1, 1):
        y = side * 0.42
        pts = [(-1.20, y, 0.14), (-1.00, y, 0.04), (0.20, y, 0.02), (0.90, y, 0.06),
               (1.30, y, 0.22), (1.50, y, 0.52), (1.44, y, 0.76), (1.28, y, 0.78),
               (1.22, y, 0.62), (1.32, y, 0.56)]
        kit.tube(f"Runner{side}", pts, 0.042, m["gold"])
        for i, x in enumerate((-0.78, -0.10, 0.58)):
            kit.tube(f"Strut{side}{i}", [(x, y, 0.04), (x + 0.04, y, 0.20), (x + 0.02, y, 0.36)],
                     0.026, m["gold"])

    # Seat and backrest, plump and quilted.
    kit.box("Seat", (-0.10, 0, 0.74), (0.86, 0.86, 0.16), m["cushion"], bevel=0.07)
    kit.box("Backrest", (-0.62, 0, 1.00), (0.18, 0.84, 0.52), m["cushion"], bevel=0.08,
            rot=(0, math.radians(-12), 0))
    for i, y in enumerate((-0.22, 0.0, 0.22)):
        kit.sphere(f"Button{i}", (-0.52, y, 1.02), 0.022, m["gold"], seg=12)

    # The sack of presents behind the driver.
    sack = kit.sphere("Sack", (-0.95, 0.12, 1.30), 0.42, m["burlap"], scale=(0.9, 0.95, 1.0))
    disp = sack.modifiers.new("Lumpy", "DISPLACE")
    tex = bpy.data.textures.new("SackLumps", "CLOUDS")
    tex.noise_scale = 0.35
    disp.texture = tex
    disp.strength = 0.08
    kit.ring("SackTie", (-0.98, 0.12, 1.70), 0.13, 0.13, 0.03, m["gold"])
    kit.sphere("SackTop", (-1.00, 0.12, 1.78), 0.12, m["burlap"], scale=(1.1, 1.1, 0.7))
    present(kit, "GiftA", (-0.78, -0.12, 1.62), 0.20, m["pine"], m["gold"], tilt=0.3)
    present(kit, "GiftB", (-1.18, -0.02, 1.58), 0.16, m["orange"], m["cream"], tilt=-0.25)
    present(kit, "GiftC", (0.74, -0.08, 0.86), 0.22, m["orange"], m["cream"], tilt=0.12)

    # A lantern on a curled post at the front.
    kit.tube("LampPost", [(0.88, -0.30, 0.70), (0.92, -0.30, 1.10), (1.00, -0.30, 1.34),
                          (1.16, -0.30, 1.38), (1.22, -0.30, 1.30)], 0.022, m["gold"])
    kit.box("LampCage", (1.22, -0.30, 1.13), (0.15, 0.15, 0.20), m["gold"], bevel=0.03, subdiv=1)
    kit.box("LampGlass", (1.22, -0.30, 1.13), (0.16, 0.16, 0.15), m["lamp"], bevel=0.02, subdiv=1)
    kit.sphere("LampTop", (1.22, -0.30, 1.25), 0.06, m["gold"], scale=(1, 1, 0.6), seg=16)
    light = bpy.data.lights.new("LampLight", "POINT")
    light.energy = 18
    light.color = lin((1.0, 0.75, 0.45))[:3]
    light.shadow_soft_size = 0.1
    lo = bpy.data.objects.new("LampLight", light)
    lo.location = (1.22, -0.45, 1.13)
    kit.link(lo)

    # Bells along the rim.
    for i, x in enumerate((-0.30, 0.10, 0.50)):
        kit.sphere(f"Bell{i}", (x, -(half + 0.10), 0.66), 0.045, m["gold"], seg=16)

    driver = build_driver(kit, m, pose)
    anchors = {
        "hand": driver["hand"],
        "lantern": Vector((1.22, -0.30, 1.13)),
        "hitch": Vector((1.36, -0.42, 0.60)),
        "tail": Vector((-1.15, -0.42, 0.10)),
    }
    return kit, root, anchors


def present(kit, name, loc, size, wrap, ribbon, tilt=0.0):
    rot = (0, 0, tilt)
    kit.box(name, loc, (size, size, size * 0.86), wrap, bevel=size * 0.12, rot=rot, subdiv=1)
    c, s = math.cos(tilt), math.sin(tilt)
    kit.box(name + "R1", loc, (size * 1.02, size * 0.18, size * 0.88), ribbon,
            bevel=0.01, rot=rot, subdiv=1)
    kit.box(name + "R2", loc, (size * 0.18, size * 1.02, size * 0.88), ribbon,
            bevel=0.01, rot=rot, subdiv=1)
    top = Vector(loc) + Vector((0, 0, size * 0.48))
    for side in (-1, 1):
        off = Vector((c * side * size * 0.16, s * side * size * 0.16, 0.02))
        kit.sphere(name + f"Bow{side}", top + off, size * 0.14, ribbon,
                   scale=(1.3, 0.7, 0.8), rot=(0, 0, tilt))


class Face:
    """Head space for a character that turns `turn` radians from its travel
    direction (+X) toward the viewer (-Y). fwd, side and up are offsets."""

    def __init__(self, center, turn):
        self.c = Vector(center)
        self.f = Vector((math.cos(turn), -math.sin(turn), 0))
        self.s = Vector((math.sin(turn), math.cos(turn), 0))
        self.turn = turn

    def at(self, fwd, side=0.0, up=0.0):
        return self.c + self.f * fwd + self.s * side + Vector((0, 0, up))


def build_driver(kit, m, pose):
    """A round, cheerful winter courier: red velvet coat, cream fur, a big
    cloud of a beard, little gold spectacles and leather mittens. He turns
    his face toward the viewer as he passes."""
    c = Vector((0.06, -0.04, 1.02))
    kit.sphere("Coat", c, 0.40, m["velvet"], scale=(0.92, 0.96, 1.0))
    trim = []
    body = Face(c, math.radians(30))
    for i in range(9):
        a = math.radians(-70 + i * 17.5)
        trim.append(body.at(0.375 * math.cos(a) * 1.02, 0, 0.39 * math.sin(a)))
    kit.tube("CoatTrim", trim, 0.055, m["fur"])
    kit.ring("Hem", (c.x, c.y, 0.80), 0.34, 0.36, 0.06, m["fur"])
    kit.ring("Belt", (c.x, c.y, 0.95), 0.37, 0.385, 0.045, m["leather"])
    buckle = body.at(0.385, 0, -0.07)
    kit.box("Buckle", buckle, (0.05, 0.14, 0.12), m["gold"], bevel=0.015, subdiv=1,
            rot=(0, 0, -body.turn))

    face = Face((0.12, -0.05, 1.60), math.radians(48))
    r = 0.23
    kit.sphere("Head", face.c, r, m["skin"])
    kit.sphere("Nose", face.at(0.225, 0, -0.03), 0.066, m["rosy"])
    for side in (-1, 1):
        kit.sphere(f"Cheek{side}", face.at(0.17, side * 0.12, -0.07), 0.066, m["rosy"],
                   scale=(0.8, 0.8, 0.75), rot=(0, 0, -face.turn))
    blink = pose == "blink"
    for side in (-1, 1):
        e = face.at(0.19, side * 0.085, 0.05)
        eye(kit, f"Eye{side}", e, 0.035, face.f, m, blink)
        kit.torus(f"Specs{side}", face.at(0.215, side * 0.085, 0.05), 0.052, 0.008, m["gold"],
                  rot=(0, math.pi / 2, -face.turn), seg=24)
        kit.sphere(f"Brow{side}", face.at(0.18, side * 0.10, 0.135), 0.036, m["fur"],
                   scale=(0.8, 1.6, 0.7), rot=(side * math.radians(-12), 0, -face.turn))
    kit.tube("SpecsBridge", [face.at(0.225, -0.035, 0.055), face.at(0.235, 0, 0.065),
                             face.at(0.225, 0.035, 0.055)], 0.007, m["gold"])
    # Beard and moustache: one fluffy cloud below the cheeks.
    beard = []
    for i in range(7):
        a = math.radians(-80 + i * 26.7)
        beard.append((tuple(face.at(0.12 + 0.05 * math.cos(a), 0.17 * math.sin(a), -0.17)), 0.13))
    for i in range(5):
        a = math.radians(-60 + i * 30)
        beard.append((tuple(face.at(0.13, 0.12 * math.sin(a), -0.30 - 0.03 * (i % 2))), 0.12))
    beard.append((tuple(face.at(0.10, 0, -0.42)), 0.10))
    kit.blobs("Beard", beard, m["fur"], resolution=0.02)
    tash = []
    for side in (-1, 1):
        tash.append((tuple(face.at(0.235, side * 0.06, -0.085)), 0.05, (0.9, 1.4, 0.7)))
        tash.append((tuple(face.at(0.205, side * 0.14, -0.06)), 0.04, (0.9, 1.0, 0.8)))
    kit.blobs("Tash", tash, m["fur"], resolution=0.015)
    smile = [face.at(0.215, -0.045, -0.15), face.at(0.23, 0, -0.165), face.at(0.215, 0.045, -0.15)]
    kit.tube("Smile", smile, 0.012, m["rosy"])

    # A soft, floppy hat with a fur brim and a pompom.
    hat_base = face.c + Vector((-0.02, 0, 0.14))
    kit.ring("Brim", hat_base, 0.21, 0.22, 0.068, m["fur"])
    hat = [hat_base, hat_base + Vector((-0.04, 0, 0.20)), hat_base + Vector((-0.16, 0, 0.34)),
           hat_base + Vector((-0.34, 0, 0.30)), hat_base + Vector((-0.42, 0, 0.16))]
    kit.tube("Hat", hat, 0.20, m["velvet"], radii=[1.0, 0.78, 0.5, 0.28, 0.12])
    kit.sphere("Pompom", hat[-1] + Vector((-0.02, 0, -0.04)), 0.078, m["fur"])

    def arm(name, shoulder, elbow, hand, glove_rot=(0, 0, 0)):
        kit.tube(name, [shoulder, elbow, hand], 0.085, m["velvet"], radii=[1.0, 0.95, 0.85])
        d = (Vector(hand) - Vector(elbow)).normalized()
        kit.sphere(name + "Cuff", Vector(hand) - d * 0.05, 0.088, m["fur"])
        kit.sphere(name + "Mitt", Vector(hand) + d * 0.05, 0.08, m["leather"],
                   scale=(1.2, 0.9, 1.0), rot=glove_rot)
        kit.sphere(name + "Thumb", Vector(hand) + d * 0.05 + Vector((0.02, 0, 0.07)), 0.035,
                   m["leather"])
        return Vector(hand) + d * 0.08

    far = arm("ArmF", (0.04, 0.28, 1.24), (0.24, 0.34, 1.00), (0.44, 0.18, 1.04))
    if pose in ("wave_a", "wave_b"):
        tilt = 0.25 if pose == "wave_a" else -0.25
        arm("ArmN", (0.02, -0.34, 1.26), (-0.06, -0.52, 1.48), (-0.20 + tilt * 0.25, -0.46, 1.84),
            glove_rot=(0, tilt, 0))
        hand = far
    else:
        hand = arm("ArmN", (0.04, -0.34, 1.24), (0.24, -0.44, 1.00), (0.44, -0.30, 1.04))
    return {"hand": hand}


# ---------------------------------------------------------------- reindeer

DEER_FRAMES = 8
UPPER = 0.27
LOWER = 0.29


def leg_points(hip, thigh, bend):
    """Hip, knee and hoof for a leg swung `thigh` radians forward with the
    lower leg folded `bend` radians back."""
    knee = hip + Vector((math.sin(thigh) * UPPER, 0, -math.cos(thigh) * UPPER))
    a = thigh - bend
    hoof = knee + Vector((math.sin(a) * LOWER, 0, -math.cos(a) * LOWER))
    return knee, hoof


def deer_pose(frame):
    """A flying gallop: forelegs reach and tuck, hind legs push and trail."""
    p = 2 * math.pi * frame / DEER_FRAMES
    legs = {}
    for side, offset in ((-1, 0.0), (1, 0.45)):
        q = p + offset
        legs[("front", side)] = (0.55 + 0.55 * math.sin(q), 0.25 + 1.05 * max(0.0, math.cos(q)))
        legs[("hind", side)] = (-0.70 + 0.55 * math.sin(q + 2.6),
                                -0.20 - 0.95 * max(0.0, math.sin(q + 1.2)))
    return {
        "lift": 0.045 * math.sin(p + 0.4),
        "pitch": 0.07 * math.sin(p + 1.4),
        "head": 0.05 * math.sin(p + 2.2),
        "ears": 0.25 * math.sin(p + 2.8),
        "legs": legs,
    }


def build_deer(m, frame):
    pose = deer_pose(frame)
    root = bpy.data.objects.new("DeerRoot", None)
    kit = Kit("Deer")
    kit.col.objects.link(root)
    kit.parent = root
    root.location = (0, 0, pose["lift"])
    root.rotation_euler = (0, -pose["pitch"], 0)

    balls = [
        ((0.0, 0, 0.80), 0.34, (1.30, 0.80, 0.82)),
        ((0.28, 0, 0.84), 0.28, (1.0, 0.86, 1.0)),
        ((-0.28, 0, 0.83), 0.28, (1.0, 0.88, 0.95)),
    ]
    h = pose["head"]
    neck_base = Vector((0.36, 0, 0.94))
    neck_top = Vector((0.56, 0, 1.16 + h))
    for t in (0.0, 0.2, 0.4, 0.6, 0.8, 1.0):
        balls.append((tuple(neck_base.lerp(neck_top, t)), 0.15 - 0.03 * t))
    # Haunches and shoulders where the legs join.
    for kind, x in (("front", 0.27), ("hind", -0.28)):
        balls.append(((x, 0, 0.72), 0.17, (1.0, 1.25, 1.2)))
    kit.blobs("DeerBody", balls, m["deer"], resolution=0.016, threshold=0.6)

    face = Face(neck_top + Vector((0.07, 0, 0.07)), math.radians(32))
    kit.sphere("DeerHead", face.c, 0.165, m["deer_head"], scale=(1.12, 0.95, 0.95),
               rot=(0, 0, -face.turn))
    snout = face.at(0.15, 0, -0.06)
    kit.sphere("Snout", snout, 0.105, m["muzzle"], scale=(1.1, 0.95, 0.85), rot=(0, 0, -face.turn))
    kit.sphere("DeerNose", face.at(0.255, 0, -0.035), 0.052, m["ink"], scale=(0.9, 1.15, 0.85),
               rot=(0, 0, -face.turn), seg=20)
    kit.tube("DeerSmile", [face.at(0.22, -0.045, -0.105), face.at(0.24, 0, -0.12),
                           face.at(0.22, 0.045, -0.105)], 0.007, m["ink"])
    for side in (-1, 1):
        eye(kit, f"DeerEye{side}", face.at(0.11, side * 0.085, 0.05), 0.04, face.f, m)
        tilt = pose["ears"] * (1 if side < 0 else 0.7)
        kit.sphere(f"Ear{side}", face.at(-0.06, side * 0.15, 0.09), 0.085, m["deer_head"],
                   scale=(0.45, 0.25, 1.0), rot=(side * math.radians(55), -0.5 + tilt, -face.turn),
                   seg=20)
        kit.sphere(f"EarIn{side}", face.at(-0.05, side * 0.155, 0.09), 0.06, m["muzzle"],
                   scale=(0.3, 0.15, 0.8), rot=(side * math.radians(55), -0.5 + tilt, -face.turn),
                   seg=16)
        # Antlers: a beam sweeping back and up, with two forward tines.
        base = face.at(-0.03, side * 0.06, 0.13)
        out = face.s * side
        beam = [base, base + out * 0.04 + Vector((-0.02, 0, 0.12)),
                base + out * 0.09 + Vector((-0.10, 0, 0.25)),
                base + out * 0.11 + Vector((-0.20, 0, 0.32)),
                base + out * 0.10 + Vector((-0.26, 0, 0.42))]
        kit.tube(f"Antler{side}", beam, 0.03, m["antler"], radii=[1, 0.9, 0.75, 0.6, 0.4])
        for i, (t0, tip) in enumerate(((1, Vector((0.10, 0, 0.12))), (2, Vector((0.08, 0, 0.13))))):
            st = beam[t0]
            kit.tube(f"Tine{side}{i}", [st, st + tip * 0.55 + Vector((0, 0, 0.01)), st + tip],
                     0.02, m["antler"], radii=[1, 0.8, 0.5])

    # Legs: tapered tubes from inside the haunch to the hoof.
    for (kind, side), (thigh, bend) in pose["legs"].items():
        hip = Vector((0.27 if kind == "front" else -0.28, side * 0.12, 0.76))
        knee, hoof = leg_points(hip, thigh, bend)
        far = side > 0
        leg = m["deer_leg"]
        kit.tube(f"Leg{kind}{side}", [hip, knee, hoof], 0.075, leg, radii=[1.25, 0.72, 0.58])
        d = (hoof - knee).normalized()
        kit.sphere(f"Fetlock{kind}{side}", hoof - d * 0.03, 0.05, m["muzzle"] if not far else leg,
                   scale=(1, 1, 0.7), seg=16)
        kit.sphere(f"Hoof{kind}{side}", hoof + d * 0.02, 0.052, m["ink"], scale=(1.15, 0.9, 0.85),
                   rot=(0, math.atan2(d.x, -d.z), 0), seg=16)
    kit.sphere("Tail", (-0.52, 0, 0.94), 0.08, m["fur"], scale=(1.0, 0.8, 1.25))

    # Harness: a saddle cloth with gold trim, and a collar of bells.
    kit.box("Blanket", (-0.02, 0, 1.08), (0.40, 0.52, 0.06), m["cloth"], bevel=0.03,
            rot=(0, 0.04, 0))
    kit.ring("BlanketTrim", (-0.02, 0, 1.08), 0.20, 0.26, 0.016, m["gold"])
    kit.sphere("BlanketStud", (-0.02, -0.27, 1.06), 0.03, m["gold"], seg=12)
    axis = (neck_top - neck_base).normalized()
    collar = kit.ring("Collar", (0, 0, 0), 0.15, 0.15, 0.035, m["lacquer"], axis="x")
    collar.location = neck_base.lerp(neck_top, 0.3)
    collar.rotation_euler = (0, -math.atan2(axis.z, axis.x) + math.pi / 2, 0)
    for i, a in enumerate((-0.8, -0.2, 0.4)):
        p = neck_base.lerp(neck_top, 0.3) + Vector((0.10, math.sin(a) * 0.14, -0.09 - 0.03 * i))
        kit.sphere(f"DeerBell{i}", p, 0.036, m["gold"], seg=16)
    world = Matrix.Translation(root.location) @ root.rotation_euler.to_matrix().to_4x4()
    anchors = {"harness": world @ Vector((0.16, -0.20, 0.98))}
    return kit, root, anchors, pose


# ---------------------------------------------------------------- snowplough

REAR_WHEEL = (-0.62, 0.46)  # x, radius
FRONT_WHEEL = (0.56, 0.36)


def build_plough(m, pose):
    root = bpy.data.objects.new("PloughRoot", None)
    kit = Kit("Plough")
    kit.col.objects.link(root)
    kit.parent = root
    w = 0.50
    kit.box("Chassis", (-0.05, 0, 0.42), (2.05, 0.92, 0.22), m["ink"], bevel=0.06)
    # Bonnet and grille.
    kit.box("Bonnet", (0.50, 0, 0.78), (0.98, 0.86, 0.56), m["orange"], bevel=0.16)
    kit.box("Grille", (1.00, 0, 0.72), (0.06, 0.62, 0.40), m["cream"], bevel=0.03)
    for i in range(4):
        kit.box(f"Slot{i}", (1.035, 0, 0.60 + i * 0.08), (0.02, 0.50, 0.025), m["ink"], bevel=0.008,
                subdiv=0)
    for side in (-1, 1):
        kit.cylinder(f"Lamp{side}", (0.98, side * 0.30, 0.94), 0.075, 0.08, m["chrome"],
                     rot=(0, math.pi / 2, 0), bevel=0.02)
        kit.cylinder(f"LampGlass{side}", (1.025, side * 0.30, 0.94), 0.058, 0.02, m["lamp"],
                     rot=(0, math.pi / 2, 0))
    # Cab: lower panel, pillars, roof and back, with see-through windows.
    kit.box("CabLow", (-0.45, 0, 0.82), (1.05, 0.98, 0.62), m["orange"], bevel=0.12)
    kit.box("CabBack", (-0.92, 0, 1.32), (0.12, 0.98, 1.0), m["orange"], bevel=0.05)
    kit.box("PillarA", (0.02, -0.45, 1.36), (0.10, 0.08, 0.66), m["orange"], bevel=0.03,
            rot=(0, math.radians(-8), 0))
    kit.box("PillarA2", (0.02, 0.45, 1.36), (0.10, 0.08, 0.66), m["orange"], bevel=0.03,
            rot=(0, math.radians(-8), 0))
    kit.box("PillarB", (-0.86, -0.45, 1.36), (0.10, 0.08, 0.66), m["orange"], bevel=0.03)
    kit.box("Roof", (-0.43, 0, 1.73), (1.18, 1.04, 0.14), m["cream"], bevel=0.06)
    kit.box("WindowSide", (-0.42, -0.47, 1.36), (0.82, 0.01, 0.58), m["glass"], bevel=0.0,
            subdiv=0)
    kit.box("Windscreen", (0.05, 0, 1.36), (0.01, 0.84, 0.58), m["glass"], bevel=0.0, subdiv=0,
            rot=(0, math.radians(-8), 0))
    kit.box("DoorLine", (-0.10, -0.495, 0.82), (0.012, 0.01, 0.48), m["ink"], bevel=0.0, subdiv=0)
    kit.box("Handle", (-0.20, -0.51, 0.98), (0.10, 0.02, 0.025), m["chrome"], bevel=0.008)
    # Mudguards over the wheels.
    for name, (x, r) in (("Rear", REAR_WHEEL), ("Front", FRONT_WHEEL)):
        arc = []
        for i in range(9):
            a = math.radians(10 + i * 20)
            arc.append((x + math.cos(a) * (r + 0.07), -w - 0.02, 0.44 * 0 + r + math.sin(a) * (r + 0.07)
                        - 0.0))
        kit.tube(f"Guard{name}", arc, 0.06, m["orange"])
    # Roof beacon and exhaust stack.
    kit.cylinder("BeaconBase", (-0.40, 0, 1.83), 0.10, 0.06, m["ink"], bevel=0.01)
    kit.sphere("Beacon", (-0.40, 0, 1.88), 0.095, m["beacon"], scale=(1, 1, 1.1))
    kit.tube("Exhaust", [(-1.02, -0.30, 0.70), (-1.04, -0.30, 1.50), (-1.04, -0.30, 2.02)], 0.045,
             m["chrome"])
    kit.cylinder("ExhaustCap", (-1.04, -0.30, 2.04), 0.06, 0.05, m["ink"])
    # Snow resting on the roof and bonnet.
    snow = []
    for i in range(13):
        x = -0.98 + i * 0.095
        bump = 0.02 * math.sin(i * 1.7)
        snow.append(((x, -0.26, 1.82 + bump), 0.16, (1.0, 1.2, 0.5)))
        snow.append(((x + 0.04, 0.20, 1.82 - bump), 0.15, (1.0, 1.2, 0.5)))
    snow.append(((-1.03, -0.44, 1.78), 0.10, (1.0, 1.0, 0.7)))
    snow.append(((0.15, -0.44, 1.77), 0.09, (1.0, 1.0, 0.7)))
    for i in range(7):
        snow.append(((0.12 + i * 0.12, -0.12 + 0.06 * (i % 2), 1.08), 0.14, (1.1, 1.8, 0.4)))
    kit.blobs("RoofSnow", snow, m["snow"], resolution=0.018, threshold=0.45)

    # The blade: a curved, striped moldboard on hydraulic arms.
    def blade(bm):
        rows = []
        for j in range(13):
            y = -0.80 + j * (1.60 / 12)
            row = []
            for i in range(17):
                a = math.radians(-55 + i * (110 / 16))
                row.append(bm.verts.new((-math.cos(a) * 0.36, y, 0.36 * math.sin(a))))
            rows.append(row)
        for j in range(12):
            for i in range(16):
                bm.faces.new((rows[j][i], rows[j][i + 1], rows[j + 1][i + 1], rows[j + 1][i]))
    bl = kit.mesh_from("Blade", blade, m["hazard"], loc=(1.56, 0, 0.30), rot=(0, 0, 0))
    sol = bl.modifiers.new("Thick", "SOLIDIFY")
    sol.thickness = 0.05
    bev = bl.modifiers.new("Soft", "BEVEL")
    bev.width = 0.015
    bev.segments = 2
    kit.tube("BladeEdge", [(1.27, -0.82, 0.03), (1.27, 0.82, 0.03)], 0.035, m["steel"])
    kit.tube("BladeTop", [(1.28, -0.82, 0.57), (1.28, 0.82, 0.57)], 0.03, m["steel"])
    for side in (-1, 1):
        kit.tube(f"Arm{side}", [(0.96, side * 0.34, 0.42), (1.14, side * 0.34, 0.38),
                                (1.26, side * 0.34, 0.30)], 0.04, m["ink"])
    kit.tube("Ram", [(0.96, -0.20, 0.62), (1.24, -0.20, 0.50)], 0.035, m["steel"])
    kit.tube("RamRod", [(1.10, -0.20, 0.56), (1.27, -0.20, 0.48)], 0.02, m["chrome"])

    # Festive touches: a string of fairy lights along the roof edge, a holly
    # wreath on the grille and snow caught along the top of the blade.
    bulbs = ("fairy_gold", "fairy_red", "fairy_green")
    for i in range(11):
        x = -0.98 + i * 0.112
        sag = 0.035 * math.sin((i % 3) / 2 * math.pi)
        kit.sphere(f"Fairy{i}", (x, -0.545, 1.64 - sag), 0.028, m[bulbs[i % 3]], seg=12)
    kit.tube("FairyWire", [(-1.0 + i * 0.112, -0.54, 1.665 - 0.035 * math.sin((i % 3) / 2 * math.pi))
                           for i in range(11)], 0.006, m["ink"])
    wreath = []
    for i in range(14):
        a = 2 * math.pi * i / 14
        wreath.append(((1.06, 0.18 * math.cos(a), 0.74 + 0.18 * math.sin(a)), 0.075, (0.7, 1.0, 1.0)))
    kit.blobs("Wreath", wreath, m["holly"], resolution=0.014, threshold=0.5)
    for i, a in enumerate((0.6, 2.2, 3.9, 5.2)):
        kit.sphere(f"WreathBerry{i}", (1.10, 0.18 * math.cos(a), 0.74 + 0.18 * math.sin(a)), 0.03,
                   m["berry"], seg=12)
    kit.sphere("WreathBow", (1.11, 0.0, 0.57), 0.05, m["berry"], scale=(0.6, 1.6, 0.8), seg=16)
    blade_snow = []
    for i in range(12):
        y = -0.74 + i * 0.135
        blade_snow.append(((1.30, y, 0.60 + 0.015 * math.sin(i * 2.1)), 0.09, (1.2, 1.1, 0.55)))
    kit.blobs("BladeSnow", blade_snow, m["snow"], resolution=0.016, threshold=0.45)
    # A side mirror on a thin arm.
    kit.tube("MirrorArm", [(0.06, -0.50, 1.12), (0.16, -0.66, 1.18), (0.18, -0.70, 1.30)], 0.012,
             m["ink"])
    kit.box("Mirror", (0.18, -0.71, 1.36), (0.05, 0.03, 0.16), m["chrome"], bevel=0.015, subdiv=1)
    kit.box("Step", (-0.42, -0.52, 0.30), (0.34, 0.10, 0.03), m["steel"], bevel=0.01, subdiv=0)

    hand = build_operator(kit, m, pose)
    anchors = {
        "hand": hand,
        "rear_wheel": Vector((REAR_WHEEL[0], -w - 0.12, REAR_WHEEL[1])),
        "front_wheel": Vector((FRONT_WHEEL[0], -w - 0.12, FRONT_WHEEL[1])),
        "beacon": Vector((-0.40, 0, 1.90)),
        "exhaust": Vector((-1.04, -0.30, 2.08)),
        "blade": Vector((1.66, -0.40, 0.08)),
        "headlight": Vector((1.03, -0.30, 0.94)),
    }
    return kit, root, anchors


def build_operator(kit, m, pose):
    """The plough's driver: a round face with rosy cheeks, a knitted beanie
    with goggles pushed up, and a thick orange scarf. They sit close to the
    side window, turn to look out at the viewer and wave as they pass."""
    body = Vector((-0.44, -0.08, 1.04))
    kit.sphere("Jacket", body, 0.32, m["jacket"], scale=(0.9, 1.0, 0.85))
    kit.ring("Stripe", (body.x, body.y, body.z + 0.05), 0.29, 0.32, 0.025, m["cream"])
    face = Face((-0.38, -0.14, 1.43), math.radians(72))
    kit.sphere("OpHead", face.c, 0.215, m["skin"])
    kit.sphere("OpNose", face.at(0.215, 0, -0.02), 0.05, m["rosy"])
    for side in (-1, 1):
        kit.sphere(f"OpCheek{side}", face.at(0.16, side * 0.115, -0.065), 0.058, m["rosy"],
                   scale=(0.8, 0.8, 0.75), rot=(0, 0, -face.turn))
    blink = pose == "blink"
    for side in (-1, 1):
        eye(kit, f"OpEye{side}", face.at(0.175, side * 0.08, 0.045), 0.038, face.f, m, blink)
        kit.sphere(f"OpBrow{side}", face.at(0.17, side * 0.085, 0.12), 0.03, m["leather"],
                   scale=(0.6, 1.5, 0.45), rot=(side * math.radians(-10), 0, -face.turn))
    grin = 0.02 if pose.startswith("wave") else 0.0
    kit.tube("Smile", [face.at(0.185, -0.06, -0.085 + grin * 0.4), face.at(0.208, 0, -0.112 - grin),
                       face.at(0.185, 0.06, -0.085 + grin * 0.4)], 0.01, m["ink"])
    kit.sphere("Beanie", face.c + Vector((-0.01, 0, 0.06)), 0.225, m["knit"], scale=(1, 1, 0.9))
    kit.ring("BeanieCuff", face.c + Vector((0.0, 0, 0.075)), 0.22, 0.22, 0.05, m["cream"])
    kit.sphere("Bobble", face.c + Vector((-0.05, 0, 0.31)), 0.075, m["scarf"])
    for side in (-1, 1):
        g = face.at(0.15, side * 0.085, 0.18)
        kit.torus(f"Goggle{side}", g, 0.052, 0.015, m["gold"],
                  rot=(0, math.radians(60), -face.turn), seg=24)
        kit.sphere(f"GoggleLens{side}", g, 0.047, m["lens"], scale=(0.4, 1, 1),
                   rot=(0, math.radians(-30), -face.turn), seg=20)
    kit.ring("GoggleStrap", face.c + Vector((-0.01, 0, 0.16)), 0.225, 0.227, 0.012, m["ink"])
    kit.ring("Scarf", (body.x + 0.02, body.y, body.z + 0.27), 0.19, 0.2, 0.065, m["scarf"])
    kit.tube("ScarfEnd", [(body.x + 0.12, body.y - 0.17, body.z + 0.24),
                          (body.x + 0.18, body.y - 0.22, body.z + 0.10),
                          (body.x + 0.16, body.y - 0.24, body.z - 0.04)], 0.05, m["scarf"],
             radii=[1, 0.9, 0.8])
    kit.torus("SteeringWheel", (-0.08, 0.0, 1.14), 0.14, 0.018, m["ink"],
              rot=(0, math.radians(55), 0), seg=32)
    # The far hand stays on the wheel.
    kit.tube("OpArmFar", [(-0.40, 0.14, 1.16), (-0.22, 0.12, 1.04), (-0.08, 0.06, 1.14)], 0.06,
             m["jacket"])
    kit.sphere("OpGloveFar", (-0.07, 0.05, 1.16), 0.055, m["leather"])
    if pose in ("wave_a", "wave_b"):
        tilt = 0.3 if pose == "wave_a" else -0.3
        shoulder = Vector((-0.36, -0.32, 1.18))
        elbow = Vector((-0.24, -0.42, 1.36))
        hand = Vector((-0.16 + tilt * 0.12, -0.44, 1.62))
        kit.tube("OpArm", [shoulder, elbow, hand], 0.06, m["jacket"])
        kit.sphere("OpGlove", hand + Vector((0, 0, 0.04)), 0.062, m["leather"],
                   scale=(0.8, 0.9, 1.2), rot=(0, tilt, 0))
        kit.sphere("OpThumb", hand + Vector((0.05, 0, 0.02)), 0.026, m["leather"])
    else:
        hand = Vector((-0.06, -0.12, 1.14))
        kit.tube("OpArm", [(-0.40, -0.32, 1.16), (-0.22, -0.34, 1.04), hand], 0.06, m["jacket"])
        kit.sphere("OpGlove", hand, 0.055, m["leather"])
    # Warm cab lights, so the driver shows clearly through the glass.
    for name, loc, energy in (("CabLight", (-0.05, -0.36, 1.58), 26),
                              ("CabFill", (-0.70, -0.42, 1.30), 10)):
        light = bpy.data.lights.new(name, "POINT")
        light.energy = energy
        light.color = lin((1.0, 0.9, 0.78))[:3]
        light.shadow_soft_size = 0.3
        lo = bpy.data.objects.new(name, light)
        lo.location = loc
        kit.link(lo)
    return hand


def build_wheel(m, rear=True):
    kit = Kit("Wheel")
    r = 0.46

    def tyre(bm):
        seg, ring = 64, 20
        verts = []
        for i in range(seg):
            a = 2 * math.pi * i / seg
            row = []
            for j in range(ring):
                b = 2 * math.pi * j / ring
                rr = r - 0.12 + 0.12 * math.cos(b) * 1.0
                row.append(bm.verts.new((rr * math.cos(a), 0.15 * math.sin(b), rr * math.sin(a))))
            verts.append(row)
        for i in range(seg):
            for j in range(ring):
                a, b = verts[i], verts[(i + 1) % seg]
                bm.faces.new((a[j], b[j], b[(j + 1) % ring], a[(j + 1) % ring]))
    kit.mesh_from("Tyre", tyre, m["rubber"])
    for i in range(16):
        a = 2 * math.pi * i / 16
        kit.box(f"Tread{i}", (math.cos(a) * (r - 0.005), 0, math.sin(a) * (r - 0.005)),
                (0.07, 0.26, 0.06), m["rubber"], bevel=0.02, rot=(0, -a, 0), subdiv=1)
    kit.cylinder("Rim", (0, -0.06, 0), r * 0.58, 0.10, m["cream"], rot=(math.pi / 2, 0, 0), bevel=0.03)
    kit.cylinder("Hub", (0, -0.12, 0), r * 0.26, 0.06, m["orange"], rot=(math.pi / 2, 0, 0),
                 bevel=0.02)
    for i in range(5):
        a = 2 * math.pi * i / 5 + 0.3
        kit.sphere(f"Bolt{i}", (math.cos(a) * r * 0.40, -0.12, math.sin(a) * r * 0.40), 0.028,
                   m["gold"], seg=12)
    kit.sphere("Cap", (0, -0.16, 0), 0.06, m["chrome"], scale=(1, 0.5, 1), seg=20)
    return kit, {}


# ---------------------------------------------------------------- small visitors
#
# The penguin, the fox and the owl are about a third of the sleigh's size.
# Each turns its face a little toward the viewer, like the drivers do.

def cone(kit, name, base, tip, r0, r1, material, seg=24):
    """A cone from `base` to `tip`, for beaks, ears and claws."""
    base, tip = Vector(base), Vector(tip)
    d = tip - base

    def build(bm):
        bmesh.ops.create_cone(bm, cap_ends=True, cap_tris=False, segments=seg,
                              radius1=r0, radius2=r1, depth=d.length)
    obj = kit.mesh_from(name, build, material, loc=(base + tip) / 2)
    obj.rotation_mode = "QUATERNION"
    obj.rotation_quaternion = d.to_track_quat("Z", "Y")
    return obj


def visitor_root(name, loc=(0, 0, 0), rot=(0, 0, 0)):
    root = bpy.data.objects.new(name + "Root", None)
    kit = Kit(name)
    kit.col.objects.link(root)
    kit.parent = root
    root.location = loc
    root.rotation_euler = rot
    return kit, root


def world_of(root):
    bpy.context.view_layer.update()
    return root.matrix_world.copy()


PENGUIN_WALK = 4


def build_penguin(m, pose):
    """A round little penguin in a striped knitted scarf. It waddles in four
    steps, rocking from foot to foot, or slides along on its belly."""
    slide = pose == "slide"
    step = 0 if slide else int(pose.split("_")[1])
    p = 2 * math.pi * step / PENGUIN_WALK
    roll = 0.0 if slide else 0.12 * math.sin(p)
    if slide:
        kit, root = visitor_root("Penguin", loc=(-0.30, 0, 0.27), rot=(0, math.radians(84), -0.35))
    else:
        kit, root = visitor_root("Penguin", rot=(roll, 0, -0.55))

    coat = [((0, 0, 0.36), 0.30, (0.86, 0.84, 1.1)), ((0.0, 0, 0.56), 0.24, (0.9, 0.85, 0.9)),
            ((0.03, 0, 0.70), 0.22, (1.0, 1.0, 0.95))]
    kit.blobs("PenguinCoat", coat, m["penguin"], resolution=0.012)
    kit.sphere("PenguinFront", (0.13, 0, 0.35), 0.25, m["penguin_white"], scale=(0.62, 0.84, 1.05))
    face = Face((0.03, 0, 0.71), 0.0)
    kit.sphere("PenguinMask", face.at(0.12, 0, 0.0), 0.15, m["penguin_white"], scale=(0.62, 1.1, 0.9))
    for side in (-1, 1):
        eye(kit, f"PenguinEye{side}", face.at(0.18, side * 0.075, 0.035), 0.036, face.f, m,
            blink=pose == "blink")
        kit.sphere(f"PenguinCheek{side}", face.at(0.17, side * 0.115, -0.04), 0.032, m["rosy"],
                   scale=(0.6, 1.0, 0.8))
    cone(kit, "PenguinBeak", face.at(0.19, 0, -0.035), face.at(0.33, 0, -0.06), 0.045, 0.006,
         m["beak"])
    flap = 0.0 if slide else 0.18 * math.sin(p + 1.2)
    for side in (-1, 1):
        out = 0.55 if slide else 0.22 + flap * side
        kit.sphere(f"Flipper{side}", (-0.02 if not slide else -0.12, side * 0.27, 0.42), 0.17,
                   m["penguin"], scale=(0.42, 0.12, 1.0),
                   rot=(side * out, 0.6 if slide else 0.1, 0))
    for side in (-1, 1):
        lift = 0.0 if slide else 0.06 * max(0.0, math.sin(p) * side)
        x = -0.22 if slide else 0.10
        kit.sphere(f"PenguinFoot{side}", (x, side * 0.10, 0.025 + lift), 0.075, m["beak"],
                   scale=(1.4, 0.95, 0.42), rot=(0, -0.5 if slide else -lift * 2, 0))
    kit.ring("ScarfRing", (0.02, 0, 0.55), 0.225, 0.235, 0.05, m["scarf_stripe"])
    sway = 0.0 if slide else 0.05 * math.sin(p + 0.7)
    tail_x = -0.18 if slide else -0.06
    kit.tube("ScarfTail", [(0.05, -0.20, 0.54), (tail_x + sway, -0.25, 0.44),
                           (tail_x - 0.04 + sway * 1.6, -0.26, 0.31)], 0.045, m["scarf_stripe"],
             radii=[1, 0.95, 0.85])
    kit.sphere("ScarfKnot", (0.07, -0.21, 0.53), 0.055, m["scarf_stripe"])
    return kit, {}


FOX_TROT = 6
FOX_UPPER = 0.17
FOX_LOWER = 0.19


def fox_leg(kit, m, name, hip, thigh, bend, far):
    knee = hip + Vector((math.sin(thigh) * FOX_UPPER, 0, -math.cos(thigh) * FOX_UPPER))
    a = thigh - bend
    paw = knee + Vector((math.sin(a) * FOX_LOWER, 0, -math.cos(a) * FOX_LOWER))
    sock = knee.lerp(paw, 0.25)
    kit.tube(name, [hip, knee, sock], 0.045, m["fox_deep"] if far else m["fox_head"],
             radii=[1.3, 0.8, 0.7])
    kit.tube(name + "Sock", [sock, paw], 0.032, m["fox_sock"], radii=[1.0, 0.9])
    kit.sphere(name + "Paw", paw + Vector((0.02, 0, 0.0)), 0.04, m["fox_sock"],
               scale=(1.3, 1.0, 0.7), seg=16)
    return paw


def fox_pose(pose):
    """Leg angles (thigh, bend) for the near and far front and hind legs,
    with body height, pitch, head tilt and tail lift."""
    legs = {}
    out = {"lift": 0.0, "pitch": 0.0, "tilt": 0.0, "nod": 0.0, "tail": 0.0, "ears": 0.0}
    if pose.startswith("trot"):
        p = 2 * math.pi * int(pose.split("_")[1]) / FOX_TROT
        for side, offset in ((-1, 0.0), (1, math.pi)):
            q = p + offset
            legs[("front", side)] = (0.42 * math.sin(q), 0.15 + 0.9 * max(0.0, math.cos(q)))
            # Diagonal pairs move together in a trot.
            r = q + math.pi
            legs[("hind", side)] = (-0.42 * math.sin(r) - 0.1, -0.25 - 0.8 * max(0.0, math.cos(r)))
        out.update(lift=0.015 * math.cos(2 * p), nod=0.03 * math.sin(2 * p),
                   tail=0.08 * math.sin(p))
    elif pose == "listen":
        legs = {("front", -1): (0.55, 1.6), ("front", 1): (0.0, 0.05),
                ("hind", -1): (-0.15, -0.3), ("hind", 1): (-0.05, -0.2)}
        out.update(tilt=0.28, nod=0.02, tail=-0.05, ears=0.2)
    elif pose == "crouch":
        legs = {("front", -1): (0.55, 1.35), ("front", 1): (0.45, 1.2),
                ("hind", -1): (0.75, -1.9), ("hind", 1): (0.65, -1.8)}
        out.update(lift=-0.12, pitch=-0.12, nod=-0.06, tail=-0.12, ears=0.25)
    elif pose == "leap":
        legs = {("front", -1): (1.2, 1.9), ("front", 1): (1.05, 1.7),
                ("hind", -1): (-1.2, -0.4), ("hind", 1): (-1.05, -0.3)}
        out.update(pitch=-0.35, tail=0.15, ears=-0.3)
    elif pose == "dive":
        legs = {("front", -1): (0.3, 0.2), ("front", 1): (0.2, 0.2),
                ("hind", -1): (0.6, -1.6), ("hind", 1): (0.5, -1.5)}
        out.update(pitch=1.2, tail=0.18, ears=-0.3)
    return legs, out


def build_fox(m, pose):
    """A red fox with a big white-tipped tail and dark socks. It trots,
    stops to listen with one paw raised, crouches, leaps and dives head
    first into the snow after something it heard underneath."""
    legs, o = fox_pose(pose)
    shoulder = Vector((0.24, 0, 0.44))
    if pose == "dive":
        # Nose down into the snow, pivoting about the shoulders, which sit
        # at the snow line so the engine can bury everything in front.
        rot = Matrix.Rotation(o["pitch"], 4, "Y")
        loc = Vector((0, 0, -0.02)) - rot @ shoulder
        kit, root = visitor_root("Fox", loc=loc, rot=(0, o["pitch"], -0.25))
    else:
        kit, root = visitor_root("Fox", loc=(0, 0, o["lift"]), rot=(0, o["pitch"], -0.25))

    body = [((0.22, 0, 0.44), 0.21, (1.0, 0.88, 1.02)), ((0.0, 0, 0.43), 0.22, (1.35, 0.84, 0.88)),
            ((-0.25, 0, 0.44), 0.20, (1.0, 0.88, 0.98))]
    kit.blobs("FoxBody", body, m["fox"], resolution=0.011)
    kit.sphere("FoxBib", (0.34, -0.02, 0.41), 0.12, m["fox_white"], scale=(0.8, 0.95, 1.15))
    neck = Vector((0.33, 0, 0.50))
    head_c = Vector((0.47, 0, 0.64 + o["nod"]))
    kit.tube("FoxNeck", [neck, neck.lerp(head_c, 0.5) + Vector((0, 0, 0.02)), head_c], 0.10,
             m["fox"], radii=[1.15, 1.0, 0.85])
    face = Face(head_c, math.radians(38))
    head = kit.sphere("FoxHead", face.c, 0.13, m["fox_head"], scale=(1.05, 0.95, 0.88),
                      rot=(o["tilt"], 0, -face.turn))
    for side in (-1, 1):
        kit.sphere(f"FoxCheek{side}", face.at(0.02, side * 0.095, -0.05), 0.085, m["fox_white"],
                   scale=(0.9, 0.8, 0.75), rot=(0, 0, -face.turn))
    kit.sphere("FoxSnout", face.at(0.13, 0, -0.025), 0.07, m["fox_head"], scale=(1.7, 0.72, 0.6),
               rot=(0, 0.1, -face.turn))
    kit.sphere("FoxChin", face.at(0.12, 0, -0.06), 0.058, m["fox_white"], scale=(1.5, 0.7, 0.5),
               rot=(0, 0.1, -face.turn))
    kit.sphere("FoxNose", face.at(0.245, 0, -0.02), 0.03, m["ink"], scale=(0.9, 1.1, 0.85), seg=16)
    up = Vector((math.sin(o["tilt"]) * 0.0, math.sin(o["tilt"]) * 0.5, 1)).normalized()
    for side in (-1, 1):
        eye(kit, f"FoxEye{side}", face.at(0.085, side * 0.06, 0.035), 0.026, face.f, m,
            blink=pose == "blink")
        lean = o["ears"]
        base = face.at(-0.03, side * 0.075, 0.08)
        tip = base + up * 0.17 + face.f * (0.03 + 0.06 * lean) + face.s * side * 0.03
        cone(kit, f"FoxEar{side}", base, tip, 0.062, 0.006, m["fox_head"])
        cone(kit, f"FoxEarIn{side}", base + face.f * 0.02, base + face.f * 0.02 + (tip - base) * 0.82,
             0.04, 0.004, m["fox_white"])
        cone(kit, f"FoxEarTip{side}", base + (tip - base) * 0.62, tip + (tip - base) * 0.02, 0.026,
             0.004, m["fox_sock"])
    # Legs, far side first so the near legs overlap them.
    for (kind, side), (thigh, bend) in sorted(legs.items(), key=lambda k: -k[0][1]):
        hip = Vector((0.24 if kind == "front" else -0.26, side * 0.085, 0.38))
        fox_leg(kit, m, f"FoxLeg{kind}{side}", hip, thigh, bend, far=side > 0)
    # The tail: long, thick and bushy, with a white tip.
    t = o["tail"]
    pts = [Vector((-0.40, 0, 0.47)), Vector((-0.58, 0, 0.43 + t * 0.3)),
           Vector((-0.76, 0, 0.40 + t * 0.8)), Vector((-0.92, 0, 0.43 + t * 1.4)),
           Vector((-1.02, 0, 0.50 + t * 2.0))]
    kit.tube("FoxTail", pts[:4], 0.11, m["fox"], radii=[0.45, 0.95, 1.15, 1.05])
    kit.tube("FoxTailTip", [pts[2].lerp(pts[3], 0.6), pts[3], pts[4]], 0.11, m["fox_white"],
             radii=[1.0, 0.95, 0.35])
    w = world_of(root)
    return kit, {"nose": w @ face.at(0.25, 0, -0.02), "tail": w @ pts[4]}


OWL_FLAPS = 4


def owl_head(kit, m, center, turn, blink=False, tilt=0.0):
    face = Face(center, turn)
    kit.sphere("OwlHead", face.c, 0.27, m["owl"], scale=(1.0, 1.05, 0.92))
    kit.sphere("OwlDisc", face.at(0.16, 0, -0.01), 0.23, m["owl_face"], scale=(0.45, 1.05, 0.88),
               rot=(tilt, 0, -face.turn))
    for side in (-1, 1):
        e = face.at(0.245, side * 0.095, 0.025 + side * tilt * 0.05)
        if blink:
            kit.tube(f"OwlLid{side}", [e + face.s * side * -0.045 + Vector((0, 0, 0.005)),
                                       e + face.f * 0.012 + Vector((0, 0, -0.012)),
                                       e + face.s * side * 0.045 + Vector((0, 0, 0.005))],
                     0.008, m["ink"])
            continue
        kit.sphere(f"OwlRim{side}", e - face.f * 0.004, 0.084, m["ink"], scale=(0.3, 1.0, 1.0),
                   rot=(0, 0, -face.turn), seg=28)
        kit.sphere(f"OwlIris{side}", e, 0.074, m["owl_eye"], scale=(0.35, 1.0, 1.0),
                   rot=(0, 0, -face.turn), seg=28)
        kit.sphere(f"OwlPupil{side}", e + face.f * 0.018, 0.044, m["ink"], scale=(0.4, 1.0, 1.0),
                   rot=(0, 0, -face.turn), seg=20)
        kit.sphere(f"OwlSpark{side}", e + face.f * 0.036 + face.s * -0.016 + Vector((0, 0, 0.022)),
                   0.015, m["spark"], seg=10)
    cone(kit, "OwlBeak", face.at(0.25, 0, -0.05), face.at(0.29, 0, -0.11), 0.026, 0.003,
         m["owl_beak"])
    for side in (-1, 1):
        kit.sphere(f"OwlBrow{side}", face.at(0.235, side * 0.095, 0.11), 0.05, m["owl"],
                   scale=(0.35, 1.3, 0.45), rot=(side * -0.25, 0, -face.turn), seg=16)
    return face


def wing(kit, m, name, shoulder, side, angle, sweep=0.0):
    """A broad owl wing reaching out along ±y, raised `angle` radians, with
    scalloped flight feathers along its trailing edge."""
    def build(bm):
        span, n = 0.92, 24
        lead, trail = [], []
        for i in range(n + 1):
            t = i / n
            y = span * t
            # Broad and rounded, as owl wings are, with a soft tip.
            chord = 0.58 * math.sqrt(max(0.0, 1 - t ** 2.6)) + 0.06
            x0 = 0.12 * math.sin(t * math.pi * 0.9) - 0.04 * t
            # Rounded feathers along the trailing edge, longest at the tip.
            feather = abs(math.sin(t * 7 * math.pi))
            scallop = 0.06 * feather * (0.25 + 1.2 * t * t)
            lead.append(bm.verts.new((x0, y, 0.03 * math.sin(t * math.pi))))
            trail.append(bm.verts.new((x0 - chord - scallop, y, 0)))
        for i in range(n):
            bm.faces.new((lead[i], lead[i + 1], trail[i + 1], trail[i]))
        # Five primary feathers fan out past the tip like fingers.
        for k in range(5):
            a = math.radians(-8 - k * 13)
            base = (0.02 - 0.09 * k, span * (0.86 - 0.025 * k))
            length = 0.30 - 0.025 * abs(k - 1.5)
            d = (math.sin(a), math.cos(a))
            nrm = (d[1], -d[0])
            pts = []
            for j in range(7):
                t = j / 6
                w = 0.045 * math.sin(math.pi * min(1.0, 0.15 + t * 0.95)) + 0.008
                c = (base[0] + d[0] * length * t, base[1] + d[1] * length * t)
                pts.append((c, w))
            left = [bm.verts.new((c[0] + nrm[0] * w, c[1] + nrm[1] * w, 0.002 * k)) for c, w in pts]
            right = [bm.verts.new((c[0] - nrm[0] * w, c[1] - nrm[1] * w, 0.002 * k)) for c, w in pts]
            for j in range(6):
                bm.faces.new((left[j], left[j + 1], right[j + 1], right[j]))
    obj = kit.mesh_from(name, build, m["owl_wing"], loc=shoulder)
    obj.scale = (1, side, 1)
    obj.rotation_euler = (side * angle, 0, side * sweep)
    sol = obj.modifiers.new("Thick", "SOLIDIFY")
    sol.thickness = 0.035
    sol.offset = 0
    sub = obj.modifiers.new("Soft", "SUBSURF")
    sub.levels = sub.render_levels = 1
    return obj


def build_owl(m, pose):
    """A plump snowy owl. Perched, it blinks and turns its head to look
    straight at the viewer; in flight it beats its wings in four strokes
    and carries a tiny wrapped present in its talons."""
    if pose.startswith("fly"):
        f = int(pose.split("_")[1])
        # Up, half down, down, half up: a slow, powerful stroke.
        angle = (1.05, 0.25, -0.55, 0.15)[f]
        # It banks toward the viewer as it glides past, which shows the
        # tops of its wings; the head stays level, as a bird's does.
        kit, root = visitor_root("Owl", loc=(0, 0, 0.0), rot=(0.72, 0.08, -0.3))
        body = [((0.0, 0, 0.0), 0.30, (1.35, 0.82, 0.78)), ((0.30, 0, 0.06), 0.24, (1, 1, 1))]
        kit.blobs("OwlBody", body, m["owl"], resolution=0.012)
        neck = world_of(root) @ Vector((0.36, 0.0, 0.16))
        kit.parent = None
        owl_head(kit, m, neck, math.radians(50))
        kit.parent = root
        for side in (-1, 1):
            wing(kit, m, f"OwlWing{side}", (0.12, side * 0.16, 0.08), side, angle,
                 sweep=-0.15 if f == 2 else 0.05)
        kit.sphere("OwlTail", (-0.42, 0, -0.02), 0.16, m["owl"], scale=(1.2, 0.9, 0.3),
                   rot=(0, 0.15, 0))
        talons = Vector((0.08, 0, -0.30))
        for side in (-1, 1):
            leg = Vector((0.08, side * 0.06, -0.16))
            kit.sphere(f"OwlLeg{side}", leg, 0.07, m["owl_face"], scale=(0.9, 0.8, 1.3))
            for k in (-1, 0, 1):
                cone(kit, f"OwlClaw{side}{k}", leg + Vector((0.03 * k, 0, -0.08)),
                     leg + Vector((0.05 * k + 0.02, 0, -0.13)), 0.012, 0.002, m["talon"], seg=10)
        w = world_of(root)
        return kit, {"talons": w @ talons}
    kit, root = visitor_root("Owl")
    # One plump pear from the toes up to the head. Metaball surfaces sit
    # well inside their radii, so the lowest ball reaches right down.
    body = [((0.02, 0, 0.17), 0.30, (1.0, 0.95, 0.75)), ((0, 0, 0.38), 0.35, (0.95, 0.92, 1.05)),
            ((0.02, 0, 0.64), 0.27, (1, 1, 0.9))]
    kit.blobs("OwlBody", body, m["owl"], resolution=0.012)
    turn = math.radians(95 if pose == "turn" else 58)
    owl_head(kit, m, (0.04, 0, 0.80), turn, blink=pose == "blink",
             tilt=0.18 if pose == "turn" else 0.0)
    kit.sphere("OwlBreast", (0.11, -0.03, 0.36), 0.25, m["owl_face"], scale=(0.6, 0.92, 1.05))
    for side in (-1, 1):
        # Folded wings lie close along the flanks.
        kit.sphere(f"OwlWing{side}", (-0.07, side * 0.19, 0.40), 0.27, m["owl_wing"],
                   scale=(0.78, 0.3, 1.0), rot=(side * 0.1, 0.18, 0))
        kit.sphere(f"OwlToes{side}", (0.12, side * 0.09, 0.065), 0.075, m["owl_face"],
                   scale=(1.25, 0.9, 0.8))
        for k in (-1, 0, 1):
            base = Vector((0.19, side * 0.09 + k * 0.03, 0.035))
            cone(kit, f"OwlClaw{side}{k}", base, base + Vector((0.045, k * 0.01, -0.03)), 0.012,
                 0.002, m["talon"], seg=10)
    kit.sphere("OwlTail", (-0.24, 0, 0.16), 0.12, m["owl_wing"], scale=(0.9, 0.85, 0.32),
               rot=(0, -0.6, 0))
    return kit, {}


def build_gift(m):
    """The owl's tiny present: pine paper, a gold ribbon and a holly sprig."""
    kit, root = visitor_root("Gift")
    present(kit, "OwlGift", (0, 0, 0.15), 0.30, m["pine"], m["gold"], tilt=-0.35)
    for i, a in enumerate((0.5, 2.6)):
        kit.sphere(f"GiftLeaf{i}", (0.07 * math.cos(a), 0.07 * math.sin(a) - 0.03, 0.35), 0.06,
                   m["holly"], scale=(1.4, 0.6, 0.3), rot=(0, 0, a))
    kit.sphere("GiftBerry", (0.0, -0.04, 0.37), 0.03, m["berry"], seg=12)
    return kit, {}


# ---------------------------------------------------------------- staging

def clear_scene():
    for obj in list(bpy.data.objects):
        bpy.data.objects.remove(obj, do_unlink=True)
    for col in list(bpy.data.collections):
        bpy.data.collections.remove(col)


def setup_render(samples):
    s = bpy.context.scene
    s.render.engine = "CYCLES"
    s.cycles.device = "CPU"
    s.cycles.samples = samples
    s.cycles.use_denoising = True
    s.cycles.denoiser = "OPENIMAGEDENOISE"
    s.cycles.transparent_max_bounces = 16
    s.cycles.max_bounces = 6
    s.cycles.filter_width = 1.2
    s.render.film_transparent = True
    s.render.image_settings.file_format = "PNG"
    s.render.image_settings.color_mode = "RGBA"
    s.render.image_settings.color_depth = "8"
    s.view_settings.view_transform = "Standard"
    s.view_settings.look = "None"
    s.view_settings.exposure = 0.0
    s.render.resolution_percentage = 100
    world = bpy.data.worlds.new("Sky")
    s.world = world
    if world.node_tree is None:
        world.use_nodes = True
    bg = world.node_tree.nodes.get("Background")
    bg.inputs["Color"].default_value = lin((0.62, 0.68, 0.80))
    bg.inputs["Strength"].default_value = 0.55
    # Ink outlines keep the small sprites readable on light wallpapers.
    s.render.use_freestyle = True
    s.render.line_thickness_mode = "ABSOLUTE"
    s.render.line_thickness = 1.3
    vl = s.view_layers[0]
    vl.use_freestyle = True
    fs = vl.freestyle_settings
    ls = fs.linesets[0] if fs.linesets else fs.linesets.new("Lines")
    ls.select_by_visibility = True
    ls.select_by_edge_types = True
    ls.select_silhouette = True
    ls.select_border = True
    ls.select_crease = False
    ls.select_external_contour = True
    style = ls.linestyle
    style.color = lin((0.20, 0.12, 0.12))[:3]
    style.alpha = 0.85
    style.thickness = 1.3


def setup_lights():
    col = bpy.data.collections.new("Lights")
    bpy.context.scene.collection.children.link(col)

    def area(name, loc, energy, color, size, target=(0, 0, 0.9)):
        light = bpy.data.lights.new(name, "AREA")
        light.energy = energy
        light.color = lin(color)[:3]
        light.size = size
        obj = bpy.data.objects.new(name, light)
        obj.location = loc
        d = Vector(target) - Vector(loc)
        obj.rotation_euler = d.to_track_quat("-Z", "Y").to_euler()
        col.objects.link(obj)

    area("Key", (3.0, -4.2, 5.0), 780, (1.0, 0.95, 0.88), 3.0)
    area("Fill", (-3.5, -3.5, 1.6), 220, (0.80, 0.88, 1.0), 4.0)
    area("Rim", (-1.5, 4.5, 3.5), 700, (1.0, 0.96, 0.92), 2.5)


def camera(yaw=YAW, pitch=PITCH):
    cam_data = bpy.data.cameras.new("Cam")
    cam_data.type = "ORTHO"
    cam = bpy.data.objects.new("Cam", cam_data)
    bpy.context.scene.collection.objects.link(cam)
    bpy.context.scene.camera = cam
    d = 20.0
    cam.location = (d * math.sin(yaw) * math.cos(pitch), -d * math.cos(yaw) * math.cos(pitch),
                    d * math.sin(pitch))
    cam.rotation_euler = (Vector((0, 0, 0)) - cam.location).to_track_quat("-Z", "Y").to_euler()
    bpy.context.view_layer.update()
    return cam


def camera_points(cols, cam):
    """Every evaluated vertex of the visible objects, in camera space."""
    dg = bpy.context.evaluated_depsgraph_get()
    inv = cam.matrix_world.inverted()
    pts = []
    for col in cols:
        for obj in col.all_objects:
            if obj.type not in ("MESH", "CURVE", "META"):
                continue
            ev = obj.evaluated_get(dg)
            try:
                me = ev.to_mesh()
            except RuntimeError:
                continue
            if me is None:
                continue
            mw = ev.matrix_world
            for v in me.vertices:
                p = inv @ (mw @ v.co)
                pts.append((p.x, p.y))
            ev.to_mesh_clear()
    return pts


def frame_box(pts):
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return min(xs), max(xs), min(ys), max(ys)


def aim(cam, box, origin_world):
    """Moves the camera so `box` (camera space) is centred, and sets the
    resolution. Returns the image position of `origin_world`."""
    x0, x1, y0, y1 = box
    w = math.ceil((x1 - x0) * PPU) + 2 * MARGIN
    h = math.ceil((y1 - y0) * PPU) + 2 * MARGIN
    w += (-w) % 2
    h += (-h) % 2
    s = bpy.context.scene
    s.render.resolution_x = w
    s.render.resolution_y = h
    cam.data.ortho_scale = max(w, h) / PPU
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    right = cam.matrix_world.to_3x3() @ Vector((1, 0, 0))
    up = cam.matrix_world.to_3x3() @ Vector((0, 1, 0))
    cam.location = cam.location + right * cx + up * cy
    bpy.context.view_layer.update()
    return w, h


def project(cam, p):
    """Image pixel of world point `p` (y grows downward)."""
    s = bpy.context.scene
    q = cam.matrix_world.inverted() @ Vector(p)
    return (s.render.resolution_x / 2 + q.x * PPU, s.render.resolution_y / 2 - q.y * PPU)


def render_to(path):
    s = bpy.context.scene
    s.render.filepath = path
    bpy.ops.render.render(write_still=True)


def only_visible(col):
    for c in bpy.context.scene.collection.children:
        c.hide_render = c is not col and c.name != "Lights"


# ---------------------------------------------------------------- atlas

def load_rgba(path):
    img = bpy.data.images.load(path)
    w, h = img.size
    import numpy as np
    px = np.array(img.pixels[:], dtype=np.float32).reshape(h, w, 4)[::-1]
    bpy.data.images.remove(img)
    return px


def pack(sizes, width):
    """Shelf packing, tallest first. Returns positions and the atlas height."""
    order = sorted(range(len(sizes)), key=lambda i: -sizes[i][1])
    pos = [None] * len(sizes)
    x = y = shelf = 0
    for i in order:
        w, h = sizes[i]
        w += (-w) % ALIGN + ALIGN
        h += (-h) % ALIGN + ALIGN
        if x + w > width:
            x, y, shelf = 0, y + shelf, 0
        pos[i] = (x, y)
        x += w
        shelf = max(shelf, h)
    height = y + shelf
    height += (-height) % ALIGN
    return pos, height


def write_atlas(frames, out_png, atlas_width=1024):
    import numpy as np
    images = [load_rgba(f["file"]) for f in frames]
    sizes = [(im.shape[1], im.shape[0]) for im in images]
    pos, height = pack(sizes, atlas_width)
    atlas = np.zeros((height, atlas_width, 4), dtype=np.float32)
    for f, im, (x, y) in zip(frames, images, pos):
        h, w = im.shape[:2]
        atlas[y:y + h, x:x + w] = im
        f["rect"] = (x, y, w, h)
    img = bpy.data.images.new("Atlas", atlas_width, height, alpha=True)
    img.colorspace_settings.name = "sRGB"
    img.pixels[:] = atlas[::-1].ravel()
    img.filepath_raw = out_png
    img.file_format = "PNG"
    img.save()
    return atlas_width, height


def rust_table(frames, groups, size, path):
    lines = [
        "// Generated by art/visitors/build.py from the Blender models. Do not edit.",
        "",
        "use super::Frame;",
        "",
        f"pub const ATLAS_WIDTH: u32 = {size[0]};",
        f"pub const ATLAS_HEIGHT: u32 = {size[1]};",
        "/// Atlas pixels per drawn pixel.",
        f"pub const SCALE: f32 = {SCALE:.1f};",
        "",
        f"pub const FRAMES: [Frame; {len(frames)}] = [",
    ]
    for f in frames:
        x, y, w, h = f["rect"]
        px, py = f["pivot"]
        lines.append(f"    // {f['name']}")
        lines.append(f"    Frame {{ rect: [{x}, {y}, {w}, {h}], pivot: [{px:.1f}, {py:.1f}] }},")
    lines.append("];")
    lines.append("")
    for name, (start, count) in groups.items():
        lines.append(f"pub const {name.upper()}: usize = {start};")
        if count > 1:
            lines.append(f"pub const {name.upper()}_FRAMES: usize = {count};")
    lines.append("")
    lines.append("/// Points on the models, in drawn pixels from the frame's pivot.")
    lines.append("pub mod anchor {")
    seen = set()
    for f in frames:
        for key, (ax, ay) in f.get("anchors", {}).items():
            const = f"{f['group']}_{key}".upper()
            if f.get("per_frame"):
                continue
            if const in seen:
                continue
            seen.add(const)
            lines.append(f"    pub const {const}: [f32; 2] = [{ax:.1f}, {ay:.1f}];")
    per = {}
    for f in frames:
        if f.get("per_frame"):
            for key, v in f["anchors"].items():
                per.setdefault(f"{f['group']}_{key}".upper(), []).append(v)
    for const, values in per.items():
        body = ", ".join(f"[{x:.1f}, {y:.1f}]" for x, y in values)
        lines.append(f"    pub const {const}: [[f32; 2]; {len(values)}] = [{body}];")
    lines.append("}")
    with open(path, "w") as fh:
        fh.write("\n".join(lines) + "\n")


# ---------------------------------------------------------------- main

def main():
    opts = parse_args()
    frames_dir = opts["frames"] or os.path.join(bpy.app.tempdir or "/tmp", "larkaura-visitors")
    os.makedirs(frames_dir, exist_ok=True)
    os.makedirs(opts["out"], exist_ok=True)
    only = opts["only"]

    jobs = []  # (group, name, builder, per_frame)
    for pose in ("idle", "blink", "wave_a", "wave_b"):
        jobs.append(("sleigh", f"sleigh_{pose}", lambda m, p=pose: build_sleigh(m, p)[::2], False))
    for f in range(DEER_FRAMES):
        jobs.append(("deer", f"deer_{f}", lambda m, f=f: build_deer(m, f)[:3:2], True))
    for pose in ("idle", "blink", "wave_a", "wave_b"):
        jobs.append(("plough", f"plough_{pose}", lambda m, p=pose: build_plough(m, p)[::2], False))
    jobs.append(("wheel", "wheel", lambda m: build_wheel(m), False))
    for pose in [f"walk_{i}" for i in range(PENGUIN_WALK)] + ["slide"]:
        jobs.append(("penguin", f"penguin_{pose}", lambda m, p=pose: build_penguin(m, p), False))
    for pose in [f"trot_{i}" for i in range(FOX_TROT)] + ["listen", "crouch"]:
        jobs.append(("fox", f"fox_{pose}", lambda m, p=pose: build_fox(m, p), True))
    for pose in ("leap", "dive"):
        jobs.append(("fox_air", f"fox_{pose}", lambda m, p=pose: build_fox(m, p), True))
    for pose in ("perch", "blink", "turn"):
        jobs.append(("owl", f"owl_{pose}", lambda m, p=pose: build_owl(m, p), False))
    for i in range(OWL_FLAPS):
        jobs.append(("owl_fly", f"owl_fly_{i}", lambda m, i=i: build_owl(m, f"fly_{i}"), True))
    jobs.append(("gift", "gift", lambda m: build_gift(m), False))

    if only:
        jobs = [j for j in jobs if j[0] in only]

    # Groups that animate share one camera box so their frames line up.
    boxes = {}
    for group in dict.fromkeys(j[0] for j in jobs):
        clear_scene()
        _materials.clear()
        m = materials()
        cam = camera(pitch=0 if group == "wheel" else PITCH, yaw=0 if group == "wheel" else YAW)
        pts = []
        for j in (j for j in jobs if j[0] == group):
            clear_scene()
            cam = camera(pitch=0 if group == "wheel" else PITCH, yaw=0 if group == "wheel" else YAW)
            kit, _ = j[2](m)
            bpy.context.view_layer.update()
            pts += camera_points([kit.col], cam)
        boxes[group] = frame_box(pts)

    setup_render(opts["samples"])
    frames = []
    groups = {}
    for group, name, builder, per_frame in jobs:
        clear_scene()
        setup_lights()
        cam = camera(pitch=0 if group == "wheel" else PITCH, yaw=0 if group == "wheel" else YAW)
        m = materials()
        kit, anchors = builder(m)
        only_visible(kit.col)
        aim(cam, boxes[group], (0, 0, 0))
        path = os.path.join(frames_dir, f"{name}.png")
        render_to(path)
        pivot = project(cam, (0, 0, 0))
        f = {"name": name, "group": group, "file": path, "pivot": pivot, "per_frame": per_frame}
        f["anchors"] = {k: ((px - pivot[0]) / SCALE, (py - pivot[1]) / SCALE)
                        for k, (px, py) in ((k, project(cam, v)) for k, v in anchors.items())}
        start, count = groups.get(group, (len(frames), 0))
        groups[group] = (start, count + 1)
        frames.append(f)
        print(f"rendered {name}", flush=True)

    if only:
        print("partial build: atlas and table not written")
        return
    size = write_atlas(frames, os.path.join(opts["out"], "visitors.png"))
    table = os.path.join(REPO, "crates/aura-effects/src/sprites/table.rs")
    os.makedirs(os.path.dirname(table), exist_ok=True)
    rust_table(frames, groups, size, table)
    with open(os.path.join(frames_dir, "frames.json"), "w") as fh:
        json.dump([{k: v for k, v in f.items() if k != "file"} for f in frames], fh, indent=1)
    if opts["blend"]:
        # Keep an editable scene with the sleigh, a reindeer and the plough.
        clear_scene()
        setup_lights()
        camera()
        m = materials()
        build_sleigh(m, "idle")
        k, root, _, _ = build_deer(m, 0)
        root.location.x += 2.6
        k2, root2, _ = build_plough(m, "idle")
        root2.location.x -= 4.2
        for i, (builder, pose) in enumerate(((build_penguin, "walk_0"), (build_fox, "listen"),
                                             (build_owl, "perch"))):
            kit, _ = builder(m, pose)
            for obj in kit.col.objects:
                if obj.parent is None:
                    obj.location.x += 4.0 + i * 1.6
        bpy.ops.wm.save_as_mainfile(filepath=opts["blend"], compress=True)
    print(f"atlas {size[0]}x{size[1]}, {len(frames)} frames")


main()
