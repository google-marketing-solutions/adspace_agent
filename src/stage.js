(() => {
"use strict";
// @abc-canvas: gemini-4-argon-gradient
var Ae = Object.defineProperty;
var Te = (t, e, o) => e in t ? Ae(t, e, { enumerable: !0, configurable: !0, writable: !0, value: o }) : t[e] = o;
var v = (t, e, o) => Te(t, typeof e != "symbol" ? e + "" : e, o);
const Ce = /^#(?:[\da-f]{3}){1,2}$/i, xe = /^\d{4}-\d\d-\d\d$/, Ee = /^([01]\d|2[0-3]):([0-5]\d)(?::([0-5]\d))?$/;
function se(t) {
  if (typeof t != "string") return null;
  const e = t.trim();
  if (!e) return null;
  const o = Date.parse(
    xe.test(e) ? `${e}T00:00` : e
  );
  return Number.isNaN(o) ? null : o;
}
function Re(t) {
  if (typeof t != "string") return null;
  const e = Ee.exec(t.trim());
  return e ? +e[1] * 3600 + +e[2] * 60 + +(e[3] ?? 0) : null;
}
function le(t) {
  if (typeof t != "string") return null;
  const e = t.trim();
  if (!e) return null;
  try {
    const { protocol: o } = new URL(e, "https://relative.invalid");
    return /^(?:https?|blob):$/.test(o) ? e : null;
  } catch {
    return null;
  }
}
function Pe(t, e) {
  var a;
  const o = typeof e == "string" ? e.trim() : "";
  switch (t.type) {
    case "string":
    case "text":
    case "richtext":
      return o || null;
    case "number":
    case "integer": {
      if (typeof e != "number" || !Number.isFinite(e)) return null;
      const s = t.type === "integer" ? Math.round(e) : e;
      return Math.min(
        t.max ?? 1 / 0,
        Math.max(t.min ?? -1 / 0, s)
      );
    }
    case "boolean":
      return typeof e == "boolean" ? e : null;
    case "color":
      return Ce.test(o) ? o : null;
    case "choice":
      return (a = t.choices.find(([s]) => s === e)) == null ? void 0 : a[0];
    case "multichoice": {
      if (!Array.isArray(e)) return null;
      const s = t.choices.map(([n]) => n).filter((n) => e.includes(n));
      return s.length ? s : null;
    }
    case "date":
      return xe.test(o) && se(o) !== null ? o : null;
    case "time":
      return Re(o) !== null ? o : null;
    case "datetime":
      return se(o) !== null ? o : null;
    case "url":
    case "file":
    case "document":
    case "page":
      return le(e);
    case "image":
    case "media": {
      const s = le(e == null ? void 0 : e.url);
      return s ? { ...e, url: s } : null;
    }
  }
}
function De(t, e) {
  const o = e && typeof e == "object" ? e : {}, a = {};
  for (const [s, n] of Object.entries(t)) {
    const i = Pe(n, o[s]) ?? n.default ?? (n.type === "boolean" ? !1 : n.type === "multichoice" ? [] : null);
    a[s] = n.transform ? n.transform(i) : i;
  }
  return a;
}
function Fe(t) {
  const e = t.config;
  return {
    ...t,
    mount(o) {
      return t.mount({
        ...o,
        colorMode: o.colorMode === "dark" ? "dark" : "light",
        config: e ? De(e, o.config) : o.config
      });
    }
  };
}
const x = (t, e, o) => [
  t / 255,
  e / 255,
  o / 255
], W = 10, j = [
  {
    id: "argon-dark",
    label: "Argon Nocturnal",
    colorMode: "dark",
    background: "#01081e",
    gradient: {
      softScale: 1 / 1.34,
      beamRange: [1, 1.22],
      corners: {
        topLeft: x(32.1, 69, 161.1),
        topRight: x(129, 217.7, 252.2),
        bottomLeft: x(9.8, 32.7, 61.2),
        bottomRight: x(0, 32.5, 135.9)
      },
      lobes: [
        {
          color: x(10.4, 67.5, 162),
          center: [0.681, 0.203],
          radii: [0.329, 0.349],
          angle: 1.117,
          weight: 0.662,
          beamModulated: !0,
          maxAlpha: 0.96
        },
        {
          color: x(115.9, 206.6, 255),
          center: [0.945, 0.144],
          radii: [0.388, 0.42],
          angle: 0.033,
          weight: 0.712,
          beamModulated: !0,
          maxAlpha: 0.96
        },
        {
          color: x(0, 66.2, 252.1),
          center: [0.853, 0.693],
          radii: [0.55, 0.729],
          angle: -0.437,
          weight: 0.334,
          beamModulated: !0,
          maxAlpha: 0.96
        },
        {
          color: x(0, 31.9, 112),
          center: [0.308, 0.57],
          radii: [0.444, 0.414],
          angle: -0.062,
          weight: 0.86,
          beamModulated: !1,
          maxAlpha: 0.96
        },
        {
          color: x(0.2, 0, 6.1),
          center: [0.21, 0.825],
          radii: [0.393, 0.619],
          angle: -0.689,
          weight: 0.79,
          beamModulated: !1,
          maxAlpha: 0.96
        },
        {
          color: x(0, 8.2, 31.5),
          center: [0.658, 0.941],
          radii: [0.491, 0.395],
          angle: -0.359,
          weight: 0.698,
          beamModulated: !1,
          maxAlpha: 0.96
        },
        {
          color: x(83.5, 208.7, 255),
          center: [1.173, 0.898],
          radii: [0.242, 0.808],
          angle: 0.033,
          weight: 0.96,
          beamModulated: !0,
          maxAlpha: 0.96
        }
      ]
    },
    particles: {
      coreColor: x(242, 253, 255),
      haloColorA: x(152, 222, 255),
      haloColorB: x(48, 148, 252)
    }
  },
  {
    id: "argon-light",
    label: "Argon Daylight",
    colorMode: "light",
    background: "#d6edff",
    gradient: {
      softScale: 1 / 1.34,
      beamRange: [1, 1.18],
      corners: {
        topLeft: x(255, 255, 255),
        topRight: x(105.8, 176, 234.3),
        bottomLeft: x(164.4, 203.8, 252.2),
        bottomRight: x(139.6, 205.2, 255)
      },
      lobes: [
        {
          color: x(156.2, 211.4, 253.5),
          center: [0.309, 0.159],
          radii: [0.495, 0.379],
          angle: -0.265,
          weight: 0.96,
          beamModulated: !1,
          maxAlpha: 0.96
        },
        {
          color: x(89.8, 185.2, 255),
          center: [0.816, 0.291],
          radii: [0.395, 0.282],
          angle: 0.191,
          weight: 0.859,
          beamModulated: !1,
          maxAlpha: 0.96
        },
        {
          color: x(164.9, 227.6, 255),
          center: [1.032, 0.492],
          radii: [0.691, 0.342],
          angle: -0.514,
          weight: 0.96,
          beamModulated: !1,
          maxAlpha: 0.96
        },
        {
          color: x(238.3, 249.9, 255),
          center: [0.182, 0.654],
          radii: [0.666, 0.371],
          angle: 0.531,
          weight: 0.833,
          beamModulated: !0,
          maxAlpha: 0.96
        },
        {
          color: x(250.5, 253, 255),
          center: [0.356, 0.452],
          radii: [0.446, 0.243],
          angle: -0.444,
          weight: 0.801,
          beamModulated: !0,
          maxAlpha: 0.96
        },
        {
          color: x(227.2, 243.4, 254.8),
          center: [0.783, 0.816],
          radii: [0.534, 0.231],
          angle: -0.374,
          weight: 0.878,
          beamModulated: !0,
          maxAlpha: 0.96
        },
        {
          color: x(97.3, 188.9, 249.2),
          center: [4e-3, 1.114],
          radii: [0.627, 0.197],
          angle: -0.873,
          weight: 0.456,
          beamModulated: !1,
          maxAlpha: 0.96
        }
      ]
    },
    particles: {
      coreColor: x(255, 255, 255),
      haloColorA: x(168, 216, 255),
      haloColorB: x(116, 182, 252)
    }
  },
  {
    id: "argon-neutral",
    label: "Argon Neutral",
    colorMode: "light dark",
    background: "#021d68",
    gradient: {
      softScale: 1 / 1.34,
      beamRange: [1, 1.2],
      corners: {
        topLeft: x(53.8, 90.2, 209.3),
        topRight: x(174.3, 246.4, 255),
        bottomLeft: x(0, 77, 137.1),
        bottomRight: x(0, 88.6, 255)
      },
      lobes: [
        {
          color: x(21.2, 158.5, 255),
          center: [0.409, 0.022],
          radii: [0.324, 0.499],
          angle: 0.579,
          weight: 0.96,
          beamModulated: !0,
          maxAlpha: 0.96
        },
        {
          color: x(150.7, 205.5, 250.8),
          center: [0.483, -0.023],
          radii: [0.451, 0.233],
          angle: -0.856,
          weight: 0.96,
          beamModulated: !0,
          maxAlpha: 0.96
        },
        {
          color: x(209.1, 249.4, 255),
          center: [0.949, 0.25],
          radii: [0.349, 0.547],
          angle: -0.072,
          weight: 0.897,
          beamModulated: !0,
          maxAlpha: 0.96
        },
        {
          color: x(69.8, 189.2, 255),
          center: [0.618, 0.501],
          radii: [0.22, 0.599],
          angle: 0.103,
          weight: 0.834,
          beamModulated: !0,
          maxAlpha: 0.96
        },
        {
          color: x(0, 52.5, 165.6),
          center: [0.321, 0.639],
          radii: [0.409, 0.318],
          angle: 0.696,
          weight: 0.958,
          beamModulated: !1,
          maxAlpha: 0.96
        },
        {
          color: x(1.1, 12.9, 51.6),
          center: [0.19, 0.845],
          radii: [0.291, 0.489],
          angle: -0.377,
          weight: 0.935,
          beamModulated: !1,
          maxAlpha: 0.96
        },
        {
          color: x(0, 21.3, 87.8),
          center: [0.665, 0.939],
          radii: [0.484, 0.308],
          angle: -0.271,
          weight: 0.934,
          beamModulated: !1,
          maxAlpha: 0.96
        },
        {
          color: x(13.4, 182.2, 238.3),
          center: [-0.069, 1.128],
          radii: [0.85, 0.22],
          angle: 0.175,
          weight: 0.801,
          beamModulated: !1,
          maxAlpha: 0.96
        },
        {
          color: x(61.6, 212.7, 255),
          center: [1.055, 0.843],
          radii: [0.22, 0.584],
          angle: -0.047,
          weight: 0.833,
          beamModulated: !0,
          maxAlpha: 0.96
        }
      ]
    },
    particles: {
      coreColor: x(242, 253, 255),
      haloColorA: x(152, 222, 255),
      haloColorB: x(48, 148, 252)
    }
  }
], Ie = j.filter(
  (t) => t.colorMode === "dark" || t.colorMode === "light dark"
).map((t) => [t.id, t.label]), ke = j.filter(
  (t) => t.colorMode === "light" || t.colorMode === "light dark"
).map((t) => [t.id, t.label]);
function ce(t, e, o) {
  const a = t === "dark" ? e : o, s = t === "dark" ? "argon-dark" : "argon-light";
  return j.find(
    (n) => n.id === a && (n.colorMode === t || n.colorMode === "light dark")
  ) ?? j.find((n) => n.id === s);
}
const g = {
  // === 1. Volumetric Light & Composition Framing ===
  /** Default internal particle/variation seed when CMS `seed` is 0. */
  baseSeed: 12,
  /** Canonical reference anchor points `[center, curve]` used to compute rotation & zoom deltas. */
  anchor: [
    [0.5, 0.26],
    [0.75, 0.68]
  ],
  /** Default composition focus handle `[x, y]` in 0..1 scene coordinates. */
  center: [0.5, 0.26],
  /** Default beam angle & spread handle `[x, y]` in 0..1 scene coordinates. */
  curve: [0.75, 0.68],
  /** Beam spread / zoom factor (20..80). */
  spread: 80,
  /** Atmospheric softness of the volumetric Gaussian lobes (15..80). */
  softness: 80,
  /** Light ray intensity / beam gain (20..100). */
  beamIntensity: 20,
  // === 2. Hero Motif ("4" or Google Symbol) ===
  /** Translucent evaporating centerpiece motif ("4" or Google Symbol). */
  motif: {
    /** Whether the hero motif layer is shown by default. */
    showFour: !1,
    /** Motif scale percentage (40..200). */
    size: 120,
    /** Horizontal offset in scene space (-50..50). */
    posX: 0,
    /** Vertical offset in scene space (-50..50). */
    posY: 0,
    /** Motif opacity percentage (5..100). */
    opacity: 30,
    /** Idle gaseous evaporation strength (0..100). */
    idleEvap: 100,
    /** Cursor hover gaseous evaporation strength (0..100). */
    hoverEvap: 75
  },
  // === 3. 3D Sparkticle Meshes ===
  /** Shared sparkticle prominence (size & alpha boost, 15..85). */
  prominence: 70,
  /** Primary 3D Sparkticle Mesh (Mesh 1 — lower-right / mid-field by default). */
  mesh1: {
    enabled: !0,
    density: 20,
    scale: 143,
    sizeFalloff: 47,
    opacityDark: 60,
    opacityLight: 35,
    meshDepth: 41,
    posX: 18,
    posY: 33,
    seedOffset: 0,
    phaseOffset: 0
  },
  /** Secondary 3D Sparkticle Mesh (Mesh 2 — upper-left canopy by default). */
  mesh2: {
    enabled: !0,
    density: 14,
    scale: 129,
    sizeFalloff: 51,
    opacityDark: 73,
    opacityLight: 35,
    meshDepth: 80,
    posX: -50,
    posY: -43,
    seedOffset: 517,
    phaseOffset: 0.37
  },
  // === 3. Film Grain & Autonomous Motion ===
  /** Default screen-space TPDF film grain intensity (0..65). */
  grain: 35,
  /** Whether the film grain pattern animates by default. */
  animateGrain: !1,
  /** Frame rate (Hz) at which animated film grain steps to a new pattern. */
  grainFps: 30,
  /** Autonomous flow & volumetric breathing speed (0..75). */
  energy: 75,
  /** Seamless animation loop duration in seconds (5, 10, or 15). */
  duration: 15,
  // === 4. Pointer Parallax, Hover & Tap Reactivity ===
  /** Enables 3D multi-plane pointer parallax and volumetric light shift. */
  parallax: !0,
  /** Enables click/tap radial surface wave ripples. */
  ripple: !0,
  /** Overall cursor hover reactivity on the 3D mesh & depth-of-field blur (0..100). */
  hoverReactivity: 30,
  /** Individual sparkticle size bloom under the cursor (0..100). */
  hoverSize: 100,
  /** 3D mesh radial repulsion & tangential swirl strength under the cursor (0..100). */
  repulsion: 20,
  /** Cursor reach radius across the 3D sparkticle mesh (0..100). */
  reach: 100
}, $ = (t, e, o) => Math.min(o, Math.max(e, t));
function Be(t) {
  let e = Math.imul(t ^ 1831565813, 2654435769) | 0;
  return () => {
    e = e + 1831565813 | 0;
    let o = e;
    return o = Math.imul(o ^ o >>> 15, o | 1), o ^= o + Math.imul(o ^ o >>> 7, o | 61), ((o ^ o >>> 14) >>> 0) / 4294967296;
  };
}
function Le(t) {
  if (!t || t <= 0)
    return {
      seed: g.baseSeed,
      center: g.center,
      curve: g.curve,
      spread: g.spread,
      beamIntensity: g.beamIntensity
    };
  const e = Be(Math.round(t)), [o, a] = g.anchor, s = Math.round(e() * 100), n = [
    $(o[0] + (e() - 0.5) * 0.22, 0.14, 0.86),
    $(o[1] + (e() - 0.5) * 0.22, 0.14, 0.86)
  ], i = [
    $(a[0] + (e() - 0.5) * 0.26, 0.12, 0.88),
    $(a[1] + (e() - 0.5) * 0.26, 0.12, 0.88)
  ], r = 40 + Math.floor(e() * 32), l = 50 + Math.floor(e() * 35);
  return {
    seed: s,
    center: n,
    curve: i,
    spread: r,
    beamIntensity: l
  };
}
const ze = {
  dark_mode_preset: {
    type: "choice",
    label: "Dark Mode Preset",
    description: "Gradient and particle theme activated when the page is in dark mode.",
    required: !0,
    choices: Ie,
    default: "argon-dark"
  },
  light_mode_preset: {
    type: "choice",
    label: "Light Mode Preset",
    description: "Gradient and particle theme activated when the page is in light mode.",
    required: !0,
    choices: ke,
    default: "argon-light"
  },
  mesh_1_text: {
    type: "string",
    label: "Primary Mesh Text / Symbol",
    description: 'Character/word string (e.g. "4", "G4") rendered in Google Sans Flex, or a Google Symbol ligature prefixed with ":" (e.g. ":spark", ":auto_awesome", ":spark:outline").',
    default: "4"
  },
  mesh_1_depth: {
    type: "integer",
    label: "Primary Mesh Wave Depth",
    description: "3D surface wave amplitude for the primary mesh (10 to 100).",
    default: g.mesh1.meshDepth,
    min: 10,
    max: 100
  },
  mesh_1_pos_x: {
    type: "integer",
    label: "Primary Mesh Position X",
    description: "Horizontal offset of the primary sparkticle mesh (-50 to 50).",
    default: g.mesh1.posX,
    min: -50,
    max: 50
  },
  mesh_1_pos_y: {
    type: "integer",
    label: "Primary Mesh Position Y",
    description: "Vertical offset of the primary sparkticle mesh (-50 to 50).",
    default: g.mesh1.posY,
    min: -50,
    max: 50
  },
  mesh_2_text: {
    type: "string",
    label: "Secondary Mesh Text / Symbol",
    description: 'Second mesh character/word string or ":symbol" ligature. Clear or set to ":none" to hide the second mesh.',
    default: "4"
  },
  mesh_2_depth: {
    type: "integer",
    label: "Secondary Mesh Wave Depth",
    description: "3D surface wave amplitude for the secondary mesh (10 to 100).",
    default: g.mesh2.meshDepth,
    min: 10,
    max: 100
  },
  mesh_2_pos_x: {
    type: "integer",
    label: "Secondary Mesh Position X",
    description: "Horizontal offset of the secondary sparkticle mesh (-50 to 50).",
    default: g.mesh2.posX,
    min: -50,
    max: 50
  },
  mesh_2_pos_y: {
    type: "integer",
    label: "Secondary Mesh Position Y",
    description: "Vertical offset of the secondary sparkticle mesh (-50 to 50).",
    default: g.mesh2.posY,
    min: -50,
    max: 50
  },
  spark_prominence: {
    type: "integer",
    label: "Spark Prominence",
    description: "Shared sparkticle size and brightness prominence across both meshes (15 to 85).",
    default: g.prominence,
    min: 15,
    max: 85
  },
  show_hero_motif: {
    type: "boolean",
    label: "Hero Motif",
    description: 'Displays the interactive translucent evaporating centerpiece motif ("4" or Google Symbol) and its micro-sparkticles.',
    default: g.motif.showFour
  },
  hero_motif_symbol: {
    type: "string",
    label: "Hero Motif Symbol",
    description: 'Optional Google Symbol ligature for the hero motif (e.g. "shield", "spark", "auto_awesome", or "shield:fill"). Leave blank or set to "4" to render the geometric "4" motif.',
    default: ""
  },
  motif_scale: {
    type: "integer",
    label: "Motif Scale",
    description: "Hero motif scale percentage (40 to 200).",
    default: g.motif.size,
    min: 40,
    max: 200
  },
  motif_opacity: {
    type: "integer",
    label: "Motif Opacity",
    description: "Hero motif opacity percentage (5 to 100).",
    default: g.motif.opacity,
    min: 5,
    max: 100
  },
  motif_pos_x: {
    type: "integer",
    label: "Motif Position X",
    description: "Horizontal offset of the hero motif (-50 to 50).",
    default: g.motif.posX,
    min: -50,
    max: 50
  },
  motif_pos_y: {
    type: "integer",
    label: "Motif Position Y",
    description: "Vertical offset of the hero motif (-50 to 50).",
    default: g.motif.posY,
    min: -50,
    max: 50
  },
  motif_idle_evap: {
    type: "integer",
    label: "Motif Idle Evaporation",
    description: "Idle gaseous diffusion and micro-sparkticle evaporation on the hero motif (0 to 100).",
    default: g.motif.idleEvap,
    min: 0,
    max: 100
  },
  motif_hover_evap: {
    type: "integer",
    label: "Motif Hover Evaporation",
    description: "Cursor-reactive gaseous plume and evaporation strength on the hero motif (0 to 100).",
    default: g.motif.hoverEvap,
    min: 0,
    max: 100
  },
  seed: {
    type: "integer",
    label: "Variation Seed",
    description: "Deterministic seed for light beam placement, spread, and intensity (0 = canonical Argon default).",
    default: 0,
    min: 0,
    max: 999
  },
  grain: {
    type: "integer",
    label: "Film Grain",
    description: "Fine screen-space film grain intensity (0 to 65).",
    default: g.grain,
    min: 0,
    max: 65
  },
  animate_grain: {
    type: "boolean",
    label: "Animate Film Grain",
    description: "Steps the film grain pattern at 30 fps for an animated texture.",
    default: g.animateGrain
  }
};
function Oe(t, e, o = {}) {
  const a = {
    size: o.size ?? g.motif.size,
    opacity: o.opacity ?? g.motif.opacity,
    posX: o.posX ?? g.motif.posX,
    posY: o.posY ?? g.motif.posY,
    idleEvap: o.idleEvap ?? g.motif.idleEvap,
    hoverEvap: o.hoverEvap ?? g.motif.hoverEvap
  };
  if (!t)
    return {
      enabled: !1,
      motifType: "four",
      symbolName: "shield",
      symbolFill: !1,
      ...a
    };
  const s = (e ?? "").trim(), n = (s.startsWith(":") ? s.slice(1) : s).trim().toLowerCase();
  if (!n || n === "4" || n === "none")
    return {
      enabled: !0,
      motifType: "four",
      symbolName: "shield",
      symbolFill: !1,
      ...a
    };
  const i = n.split(":").filter(Boolean), r = i[0] || "shield";
  if (r === "4")
    return {
      enabled: !0,
      motifType: "four",
      symbolName: "shield",
      symbolFill: !1,
      ...a
    };
  const l = i[1];
  return {
    enabled: !0,
    motifType: "symbol",
    symbolName: r,
    symbolFill: l === "fill" || l === "filled" || l === "1",
    ...a
  };
}
function fe(t) {
  const e = (t ?? "").trim();
  if (!e || e.toLowerCase() === ":none")
    return {
      enabled: !1,
      shapeType: "text",
      text: "4",
      symbol: "spark",
      symbolFill: !0
    };
  if (e.startsWith(":")) {
    const a = e.slice(1).trim().toLowerCase().split(":").filter(Boolean), s = a[0] || "spark", n = a[1];
    return {
      enabled: !0,
      shapeType: "symbol",
      text: "4",
      symbol: s,
      symbolFill: n !== "outline" && n !== "unfilled" && n !== "0"
    };
  }
  return {
    enabled: !0,
    shapeType: "text",
    text: e.slice(0, 16),
    symbol: "spark",
    symbolFill: !0
  };
}
const ye = "M480 80Q474 80 469.0 84.0Q464 88 462 94Q445 161 411.0 220.0Q377 279 328 328Q279 377 220.0 411.0Q161 445 94 462Q88 464 84.0 469.0Q80 474 80.0 480.0Q80 486 84.0 491.0Q88 496 94 498Q161 515 220.0 549.0Q279 583 328 632Q377 681 411.0 740.0Q445 799 462 866Q464 872 469.0 876.0Q474 880 480.0 880.0Q486 880 490.5 876.0Q495 872 497 866Q515 799 549.0 740.0Q583 681 632.0 632.0Q681 583 740.0 549.0Q799 515 866 498Q872 496 876.0 491.0Q880 486 880.0 480.0Q880 474 876.0 469.0Q872 464 866 462Q799 445 740.0 411.0Q681 377 632 328Q583 279 549.0 220.0Q515 161 498 94Q496 88 491.0 84.0Q486 80 480 80Z", k = 256, be = 1024, Se = [0, 2.5, 6.5, 14], Ge = 16, H = /* @__PURE__ */ new Map(), te = /* @__PURE__ */ new Map();
function Y(t) {
  t.width = 0, t.height = 0;
}
function we(t) {
  const e = t.width, o = t.height, a = t.getContext("2d", { willReadFrequently: !0 }), s = a ? new Uint8Array(a.getImageData(0, 0, e, o).data.buffer) : new Uint8Array(e * o * 4);
  return Y(t), { width: e, height: o, pixels: s };
}
function re(t, e) {
  if (H.has(t))
    H.delete(t);
  else if (H.size >= Ge) {
    const o = H.keys().next().value;
    o !== void 0 && H.delete(o);
  }
  H.set(t, e);
}
function Xe(t) {
  if (t.shapeType === "text")
    return `text:${t.text.trim() || "4"}`;
  const e = t.symbol.trim().toLowerCase() || "spark", o = t.symbolFill ? 1 : 0;
  return `symbol:${e}:${o}`;
}
function Ue() {
  const t = document.createElement("canvas");
  t.width = k, t.height = k;
  const e = t.getContext("2d");
  return e.clearRect(0, 0, k, k), e.save(), e.translate(48, 48), e.scale(160 / 820, 160 / 820), e.translate(-70, 890), e.scale(1, -1), e.fillStyle = "#ffffff", e.fill(new Path2D(ye)), e.restore(), t;
}
function He(t) {
  const e = document.createElement("canvas");
  e.width = be, e.height = k;
  const o = e.getContext("2d", { willReadFrequently: !0 });
  return o.clearRect(0, 0, e.width, e.height), Se.forEach((a, s) => {
    o.filter = `blur(${a}px)`, o.drawImage(t, s * k, 0, k, k);
  }), o.filter = "none", Y(t), e;
}
function Ve(t) {
  const e = t.width, o = t.height, s = t.getContext("2d", { willReadFrequently: !0 }).getImageData(0, 0, e, o).data;
  let n = e, i = o, r = -1, l = -1;
  for (let f = 0; f < o; f++)
    for (let m = 0; m < e; m++)
      s[(f * e + m) * 4 + 3] > 16 && (m < n && (n = m), m > r && (r = m), f < i && (i = f), f > l && (l = f));
  const h = document.createElement("canvas");
  h.width = k, h.height = k;
  const p = h.getContext("2d");
  if (p.clearRect(0, 0, k, k), r >= n && l >= i) {
    const f = Math.max(1, r - n + 1), m = Math.max(1, l - i + 1), y = 160 / Math.max(f, m), M = f * y, E = m * y, A = (k - M) * 0.5, S = (k - E) * 0.5;
    return p.imageSmoothingEnabled = !0, p.imageSmoothingQuality = "high", p.drawImage(t, n, i, f, m, A, S, M, E), { tile: h, hasInk: !0 };
  }
  return Y(h), { tile: h, hasInk: !1 };
}
async function Ne(t, e) {
  const o = t.trim().length > 0 ? t : "4";
  typeof document < "u" && document.fonts && await document.fonts.load('700 220px "Google Sans Flex"', o).catch(() => {
  });
  const a = Array.from(o).slice(0, 16), s = Math.max(1, a.length);
  let n = 1;
  for (; n < s; ) n <<= 1;
  const i = 384, r = [];
  let l = i, h = -1, p = 1;
  for (let T = 0; T < s; T++) {
    const u = a[T];
    if (!u || u.trim() === "") {
      r.push({ isSpace: !0 });
      continue;
    }
    const c = document.createElement("canvas");
    c.width = i, c.height = i;
    const _ = c.getContext("2d", { willReadFrequently: !0 });
    _.clearRect(0, 0, i, i), _.fillStyle = "#ffffff", _.textAlign = "center", _.textBaseline = "alphabetic", _.font = '700 220px "Google Sans Flex", Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif', _.fillText(u, i * 0.5, 268);
    const I = _.getImageData(0, 0, i, i).data;
    let C = i, b = i, P = -1, F = -1;
    for (let w = 0; w < i; w++)
      for (let D = 0; D < i; D++)
        I[(w * i + D) * 4 + 3] > 16 && (D < C && (C = D), D > P && (P = D), w < b && (b = w), w > F && (F = w));
    if (P >= C && F >= b) {
      const w = Math.max(1, P - C + 1), D = Math.max(1, F - b + 1);
      b < l && (l = b), F > h && (h = F), w > p && (p = w), r.push({ isSpace: !1, canvas: c, minX: C, minY: b, bw: w, bh: D });
    } else
      Y(c), r.push({ isSpace: !0 });
  }
  const f = h >= l ? Math.max(1, h - l + 1) : 160, m = h >= l ? (l + h) * 0.5 : i * 0.5, y = 160 / Math.max(1, p, f), M = document.createElement("canvas");
  M.width = be, M.height = k * n;
  const E = M.getContext("2d", { willReadFrequently: !0 });
  E.clearRect(0, 0, M.width, M.height);
  for (let T = 0; T < s; T++) {
    const u = r[T];
    if (!u.isSpace && u.canvas !== void 0) {
      const c = document.createElement("canvas");
      c.width = k, c.height = k;
      const _ = c.getContext("2d");
      _.clearRect(0, 0, k, k);
      const I = u.bw * y, C = u.bh * y, b = (k - I) * 0.5, P = 128 + (u.minY - m) * y;
      _.imageSmoothingEnabled = !0, _.imageSmoothingQuality = "high", _.drawImage(
        u.canvas,
        u.minX,
        u.minY,
        u.bw,
        u.bh,
        b,
        P,
        I,
        C
      ), Y(u.canvas), Se.forEach((F, w) => {
        E.filter = `blur(${F}px)`, E.drawImage(
          c,
          w * k,
          T * k,
          k,
          k
        );
      }), E.filter = "none", Y(c);
    }
  }
  const { width: A, height: S, pixels: L } = we(M), R = {
    key: e,
    width: A,
    height: S,
    pixels: L,
    charCount: s,
    atlasRows: n,
    spaceMask: r.map((T) => T.isSpace)
  };
  return re(e, R), R;
}
async function Ye(t, e, o) {
  const a = t.trim().toLowerCase() || "spark", s = e ? 1 : 0;
  typeof document < "u" && document.fonts && await document.fonts.load('600 320px "Google Symbols"', a).catch(() => {
  });
  const n = 512, i = document.createElement("canvas");
  i.width = n, i.height = n;
  const r = `'FILL' ${s}, 'wght' 600, 'GRAD' 0, 'ROND' 100, 'opsz' 48`;
  i.style.cssText = "position:fixed;left:-9999px;top:-9999px;width:512px;height:512px;opacity:0;pointer-events:none;z-index:-1;", i.style.fontOpticalSizing = "none", i.style.fontFeatureSettings = '"liga" 1', i.style.fontVariationSettings = r, typeof document < "u" && document.body && (document.body.appendChild(i), i.offsetWidth, window.getComputedStyle(i).fontVariationSettings);
  let l;
  try {
    const y = i.getContext("2d", { willReadFrequently: !0 });
    if (y.clearRect(0, 0, n, n), y.fillStyle = "#ffffff", y.textAlign = "center", y.textBaseline = "middle", y.font = "10px sans-serif", y.font = '600 320px "Google Symbols"', "fontVariationSettings" in y)
      try {
        y.fontVariationSettings = r;
      } catch {
      }
    y.fillText(a, n * 0.5, n * 0.5);
    const M = Ve(i);
    l = M.hasInk ? M.tile : Ue();
  } finally {
    i.remove(), Y(i);
  }
  const h = He(l), { width: p, height: f, pixels: m } = we(h), d = {
    key: o,
    width: p,
    height: f,
    pixels: m,
    charCount: 1,
    atlasRows: 1,
    spaceMask: [!1]
  };
  return re(o, d), d;
}
function ue(t) {
  const e = Xe(t), o = H.get(e);
  if (o)
    return re(e, o), Promise.resolve(o);
  const a = te.get(e);
  if (a) return a;
  const s = (t.shapeType === "text" ? Ne(t.text, e) : Ye(t.symbol, t.symbolFill, e)).finally(() => {
    te.delete(e);
  });
  return te.set(e, s), s;
}
const We = "canvas{display:block;width:100%;height:100%}", oe = {
  scale(t, e) {
    const o = t / Math.max(1, e) / 1.7777777777777777;
    return o < 1 ? [o, 1] : [1, 1 / o];
  },
  toScene(t, e, o) {
    const a = this.scale(e, o);
    return [(t[0] - 0.5) * a[0] + 0.5, (t[1] - 0.5) * a[1] + 0.5];
  }
};
class Qe {
  constructor(e, listenEl = e) {
    v(this, "pointer", [0, 0]);
    v(this, "target", [0.54, 0.5]);
    v(this, "hover", [0.54, 0.5]);
    v(this, "inside", !1);
    v(this, "amount", 0);
    v(this, "tap", [0.54, 0.5]);
    v(this, "tapTime", -1e4);
    v(this, "outPointer", [0, 0]);
    v(this, "outHover", [0.54, 0.5]);
    v(this, "outTap", [0.54, 0.5]);
    v(this, "sampled");
    v(this, "controller", new AbortController());
    this.sampled = {
      pointer: this.outPointer,
      hover: this.outHover,
      hoverAmount: 0,
      tap: this.outTap,
      tapAge: 10
    };
    const { signal: o } = this.controller, a = (r) => {
      if (r.pointerType === "touch") return;
      const l = e.getBoundingClientRect();
      !l.width || !l.height || (this.target[0] = (r.clientX - l.left) / l.width, this.target[1] = (r.clientY - l.top) / l.height, !this.inside && this.amount < 0.01 && (this.hover[0] = this.target[0], this.hover[1] = this.target[1]), this.inside = !0);
    }, s = () => {
      this.inside = !1;
    }, n = (r) => {
      const l = e.getBoundingClientRect();
      !l.width || !l.height || (this.tap[0] = (r.clientX - l.left) / l.width, this.tap[1] = (r.clientY - l.top) / l.height, this.tapTime = performance.now());
    }, i = () => {
      this.reset();
    };
    listenEl.addEventListener("pointermove", a, { signal: o, passive: !0 }), listenEl.addEventListener("pointerenter", a, { signal: o, passive: !0 }), listenEl.addEventListener("pointerleave", s, { signal: o, passive: !0 }), listenEl.addEventListener("pointercancel", s, { signal: o, passive: !0 }), listenEl.addEventListener("pointerdown", n, { signal: o, passive: !0 }), window.addEventListener("blur", i, { signal: o });
  }
  /**
   * Returns true while the pointer is inside the canvas, hover/parallax is
   * still settling back to rest, or a tap ripple is propagating (< 4s).
   */
  isActive(e) {
    return this.inside || this.amount > 1e-3 || Math.abs(this.pointer[0]) > 1e-3 || Math.abs(this.pointer[1]) > 1e-3 || e - this.tapTime < 4e3;
  }
  reset() {
    this.pointer[0] = 0, this.pointer[1] = 0, this.target[0] = 0.54, this.target[1] = 0.5, this.hover[0] = 0.54, this.hover[1] = 0.5, this.inside = !1, this.amount = 0, this.tap[0] = 0.54, this.tap[1] = 0.5, this.tapTime = -1e4, this.outPointer[0] = 0, this.outPointer[1] = 0, this.outHover[0] = 0.54, this.outHover[1] = 0.5, this.outTap[0] = 0.54, this.outTap[1] = 0.5, this.sampled.hoverAmount = 0, this.sampled.tapAge = 10;
  }
  sample(e, o, a, s) {
    const n = Math.max(0, e), i = 1 - Math.exp(-n * 6.2), r = 1 - Math.exp(-n * (this.inside ? 5.5 : 2));
    this.hover[0] += (this.target[0] - this.hover[0]) * i, this.hover[1] += (this.target[1] - this.hover[1]) * i, this.amount += ((this.inside ? 1 : 0) - this.amount) * r, this.amount < 1e-4 && (this.amount = 0);
    const l = this.inside ? this.target[0] - 0.5 : 0, h = this.inside ? this.target[1] - 0.5 : 0;
    return this.pointer[0] += (l - this.pointer[0]) * i, this.pointer[1] += (h - this.pointer[1]) * i, this.outPointer[0] = a ? this.pointer[0] : 0, this.outPointer[1] = a ? this.pointer[1] : 0, this.outHover[0] = this.hover[0], this.outHover[1] = this.hover[1], this.outTap[0] = this.tap[0], this.outTap[1] = this.tap[1], this.sampled.hoverAmount = this.amount, this.sampled.tapAge = s ? (o - this.tapTime) / 1e3 : 10, this.sampled;
  }
  destroy() {
    this.controller.abort();
  }
}
const J = 512, K = 328, Q = 1e12, qe = 280, $e = 8, V = /* @__PURE__ */ new Map(), ne = /* @__PURE__ */ new Map();
function Me(t, e) {
  if (V.has(t))
    V.delete(t);
  else if (V.size >= $e) {
    const o = V.keys().next().value;
    o !== void 0 && V.delete(o);
  }
  V.set(t, e);
}
const Z = (t, e, o) => {
  const a = Math.max(0, Math.min(1, (o - t) / (e - t)));
  return a * a * (3 - 2 * a);
};
function me(t, e, o, a, s) {
  o[0] = 0, a[0] = -Q, a[1] = Q;
  let n = 0;
  for (let i = 1; i < s; i++) {
    let r = (t[i] + i * i - (t[o[n]] + o[n] * o[n])) / (2 * i - 2 * o[n]);
    for (; r <= a[n]; )
      n--, r = (t[i] + i * i - (t[o[n]] + o[n] * o[n])) / (2 * i - 2 * o[n]);
    n++, o[n] = i, a[n] = r, a[n + 1] = Q;
  }
  n = 0;
  for (let i = 0; i < s; i++) {
    for (; a[n + 1] < i; ) n++;
    const r = i - o[n];
    e[i] = r * r + t[o[n]];
  }
}
function he(t, e, o) {
  const a = Math.max(e, o), s = new Float64Array(a), n = new Float64Array(a), i = new Int32Array(a), r = new Float64Array(a + 1);
  for (let l = 0; l < e; l++) {
    for (let h = 0; h < o; h++) s[h] = t[h * e + l];
    me(s, n, i, r, o);
    for (let h = 0; h < o; h++) t[h * e + l] = n[h];
  }
  for (let l = 0; l < o; l++) {
    const h = l * e;
    for (let p = 0; p < e; p++) s[p] = t[h + p];
    me(s, n, i, r, e);
    for (let p = 0; p < e; p++) t[h + p] = Math.sqrt(n[p]);
  }
}
function Ze(t) {
  const e = document.createElement("canvas");
  e.width = t, e.height = t;
  const o = e.getContext("2d", { willReadFrequently: !0 });
  o.clearRect(0, 0, t, t), o.save();
  const a = (t - K) * 0.5;
  return o.translate(a, a), o.scale(K / 820, K / 820), o.translate(-70, 890), o.scale(1, -1), o.fillStyle = "#ffffff", o.fill(new Path2D(ye)), o.restore(), e;
}
function Ke(t, e) {
  const o = J, s = t.getContext("2d", { willReadFrequently: !0 }).getImageData(0, 0, o, o).data;
  t.width = 0, t.height = 0;
  const n = new Float64Array(o * o), i = new Float64Array(o * o);
  for (let f = 0; f < o * o; f++) {
    const m = s[f * 4 + 3] / 255;
    if (m <= 4e-3)
      n[f] = Q, i[f] = 0;
    else if (m >= 0.996)
      n[f] = 0, i[f] = Q;
    else {
      const d = 0.5 - m, y = Math.max(0, d), M = Math.max(0, -d);
      n[f] = y * y, i[f] = M * M;
    }
  }
  he(n, o, o), he(i, o, o);
  const r = new Uint8Array(o * o * 2), l = [];
  for (let f = 0; f < o; f++)
    for (let m = 0; m < o; m++) {
      const d = f * o + m, y = n[d] - i[d], M = Math.max(0, Math.min(1, 0.5 + y / 256)), E = Math.round(M * 65535), A = d * 2;
      if (r[A] = E >> 8 & 255, r[A + 1] = E & 255, Math.abs(y) <= 1.35 && (m + f & 1) === 0) {
        const S = (m + 0.5) / o, L = (f + 0.5) / o, R = (S - 0.5) * 0.96, T = (L - 0.5) * 0.96, u = 0.54 + R / 1.7778, c = 0.5 + T, _ = Math.min(
          1,
          Math.max(
            0.25,
            Z(-0.02, -0.22, R) * Z(-0.08, 0.25, T) + Z(0.02, 0.22, R) * Z(0.12, -0.22, T) * 0.85
          )
        );
        l.push([u, c, _]);
      }
    }
  const h = [];
  if (l.length > 0) {
    const f = Math.max(
      1,
      Math.floor(l.length / qe)
    );
    for (let m = 0; m < l.length; m += f)
      h.push(l[m]);
  }
  const p = { key: e, rgBytes: r, contourPoints: h };
  return Me(e, p), p;
}
async function je(t, e, o) {
  const a = t.trim().toLowerCase() || "shield", s = e ? 1 : 0, n = 600, i = 100;
  typeof document < "u" && document.fonts && await document.fonts.load(`${n} 256px "Google Symbols"`, a).then(() => document.fonts.ready).catch(() => {
  });
  const r = J, l = document.createElement("canvas");
  l.width = r, l.height = r;
  const h = `'FILL' ${s}, 'wght' ${n}, 'GRAD' 0, 'ROND' ${i}, 'opsz' 48`;
  l.style.cssText = "position:fixed;left:-9999px;top:-9999px;width:512px;height:512px;opacity:0;pointer-events:none;z-index:-1;", l.style.fontOpticalSizing = "none", l.style.fontFeatureSettings = '"liga" 1', l.style.fontVariationSettings = h, typeof document < "u" && document.body && (document.body.appendChild(l), l.offsetWidth, window.getComputedStyle(l).fontVariationSettings);
  let p;
  try {
    const f = l.getContext("2d", { willReadFrequently: !0 });
    if (f.clearRect(0, 0, r, r), f.fillStyle = "#ffffff", f.textAlign = "center", f.textBaseline = "middle", f.font = "10px sans-serif", f.font = `${n} 340px "Google Symbols"`, "fontVariationSettings" in f)
      try {
        f.fontVariationSettings = h;
      } catch {
      }
    f.fillText(a, r * 0.5, r * 0.5);
    const m = f.getImageData(0, 0, r, r).data;
    let d = r, y = r, M = -1, E = -1;
    for (let A = 0; A < r; A++)
      for (let S = 0; S < r; S++)
        m[(A * r + S) * 4 + 3] > 16 && (S < d && (d = S), S > M && (M = S), A < y && (y = A), A > E && (E = A));
    if (M >= d && E >= y) {
      p = document.createElement("canvas"), p.width = r, p.height = r;
      const A = p.getContext("2d", { willReadFrequently: !0 });
      A.clearRect(0, 0, r, r);
      const S = Math.max(1, M - d + 1), L = Math.max(1, E - y + 1), R = K / Math.max(S, L), T = S * R, u = L * R, c = (r - T) * 0.5, _ = (r - u) * 0.5;
      A.imageSmoothingEnabled = !0, A.imageSmoothingQuality = "high", A.drawImage(l, d, y, S, L, c, _, T, u);
    } else
      p = Ze(r);
  } finally {
    l.remove(), l.width = 0, l.height = 0;
  }
  return Ke(p, o);
}
function Je(t) {
  if (!t.enabled || t.motifType !== "symbol")
    return Promise.resolve(null);
  const e = t.symbolName.trim().toLowerCase() || "shield", o = t.symbolFill ? 1 : 0, a = `sym:${e}:${o}:600:100`, s = V.get(a);
  if (s)
    return Me(a, s), Promise.resolve(s);
  const n = ne.get(a);
  if (n) return n;
  const i = je(e, t.symbolFill, a).finally(() => {
    ne.delete(a);
  });
  return ne.set(a, i), i;
}
const et = `#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform vec2 u_resolution;
uniform vec2 u_viewScale;
uniform vec2 u_shift;
uniform vec2 u_pointer;
uniform vec2 u_tap;
uniform vec2 u_focusPoint;
uniform float u_focusAmount;
uniform float u_zoom;
uniform vec2 u_rotCosSin;
uniform float u_beamIntensity;
uniform float u_grain;
uniform vec4 u_grainOffset;
uniform float u_energy;
uniform float u_phase;
uniform float u_tapAge;

// Theme preset uniforms
uniform vec2 u_beamRange;
uniform vec3 u_corners[4]; // 0: topLeft, 1: topRight, 2: bottomLeft, 3: bottomRight
uniform int u_lobeCount;
uniform vec3 u_lobeColors[10];
uniform vec4 u_lobeCentersRadii[10]; // xy = center, zw = 1.0 / (radii * effSoft)
uniform vec2 u_lobeCosSin[10]; // x = cos(angle), y = sin(angle)
uniform vec3 u_lobeParams[10]; // x = weight, y = beamModulated (0 or 1), z = maxAlpha

uniform sampler2D u_noiseMap;

float smooth01(float a, float b, float x) {
  float t = clamp((x - a) / (b - a), 0.0, 1.0);
  return t * t * (3.0 - 2.0 * t);
}

float rotBell(vec2 p, vec2 c, vec2 invR, vec2 cs) {
  vec2 d = p - c;
  vec2 q = vec2(cs.x * d.x + cs.y * d.y, -cs.y * d.x + cs.x * d.y) * invR;
  return exp(-dot(q, q) * 2.0);
}

void main() {
  vec2 screen = vec2(v_uv.x, 1.0 - v_uv.y);
  vec2 scene = (screen - 0.5) * u_viewScale + 0.5;
  float a = u_phase * 6.2831853;
  float effBeam = mix(
    u_beamRange.x,
    u_beamRange.y,
    clamp((u_beamIntensity - 20.0) / 80.0, 0.0, 1.0)
  );

  // Sub-bit TPDF Interleaved Gradient Noise dither to eliminate 8-bit Mach banding in smooth gradients
  float ign1 = fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(0.06711056, 0.00583715))));
  float ign2 = fract(52.9829189 * fract(dot(gl_FragCoord.xy + vec2(17.3, 31.7), vec2(0.06711056, 0.00583715))));
  vec3 dither = vec3(
    ign1 - ign2,
    (ign2 - 0.5) + (ign1 - 0.5),
    fract(ign1 + 0.37) - fract(ign2 + 0.73)
  ) * (1.35 / 255.0);

  // Transform scene coordinates by composition handles, subtle autonomous breathing, pointer parallax, and tap ripple
  vec2 p = scene;
  if (u_tapAge < 4.0) {
    p += normalize(p - u_tap + vec2(0.00001)) *
         sin(length(p - u_tap) * 35.0 - u_tapAge * 7.0) *
         exp(-u_tapAge * 2.4) * 0.0028;
  }
  p -= u_shift * 0.55;
  p += u_pointer * 0.018;
  float motionAmp = u_energy * 0.00032;
  vec2 flowOffset = vec2(
    sin(a) * 1.15 + 0.35 * cos(a * 2.0),
    cos(a) * 0.95 - 0.30 * sin(a * 2.0)
  ) * motionAmp;
  p += flowOffset;
  mat2 R = mat2(u_rotCosSin.x, -u_rotCosSin.y, u_rotCosSin.y, u_rotCosSin.x);
  p = R * (p - 0.5) / u_zoom + 0.5;

  vec2 pSky = p - (u_focusPoint - 0.5) * u_focusAmount * 0.026;

  // Gentle autonomous volumetric breathing wave when flow speed (energy) > 0
  float diagCoord = pSky.x * 0.78 + pSky.y * 0.62;
  float rayDrift = sin(a + diagCoord * 2.6) * (u_energy * 0.00045);
  vec2 pWave = pSky + vec2(rayDrift, -rayDrift * 0.75);

  // 1. Unified 2D atmospheric 4-corner base canvas
  vec3 topRow = mix(u_corners[0], u_corners[1], clamp(pWave.x, 0.0, 1.0));
  vec3 botRow = mix(u_corners[2], u_corners[3], clamp(pWave.x, 0.0, 1.0));
  vec3 col = mix(topRow, botRow, clamp(pWave.y, 0.0, 1.0));

  // 2. Multi-lobe rotated Gaussian volumetric field
  for (int i = 0; i < 10; i++) {
    if (i >= u_lobeCount) break;
    vec4 cr = u_lobeCentersRadii[i];
    vec3 pr = u_lobeParams[i];
    float gain = pr.x * mix(1.0, effBeam, pr.y);
    float lobeAlpha = clamp(
      rotBell(pWave, cr.xy, cr.zw, u_lobeCosSin[i]) * gain,
      0.0,
      pr.z
    );
    col = mix(col, u_lobeColors[i], lobeAlpha);
  }

  // Screen-space pixel-locked fine film grain (never stretches with container aspect ratio or magnifies into blotches)
  if (u_grain > 0.001) {
    float pxScale = max(1.0, max(u_resolution.x, u_resolution.y) / 2560.0);
    vec2 grainPx = floor(gl_FragCoord.xy / pxScale);
    vec2 tileId = floor(grainPx / 512.0);
    vec2 noiseUV1 = (grainPx + u_grainOffset.xy + 0.5) / 512.0;
    vec2 noiseUV2 = (
      grainPx +
      u_grainOffset.zw +
      floor(vec2(tileId.x * 173.0 + tileId.y * 311.0, tileId.x * 257.0 - tileId.y * 139.0)) +
      0.5
    ) / 512.0;
    float n1 = texture(u_noiseMap, noiseUV1).r - 0.5;
    float n2 = texture(u_noiseMap, noiseUV2).g - 0.5;
    // Sum of two independent uniform random channels forms a smooth, zero-mean Gaussian-like (TPDF) fine 1px grain
    float grainSignal = (n1 + n2) * 0.65;
    float grainAmp = u_grain * 0.0024;
    // Scalar luminance headroom shift only at extreme >92% white or <8% black
    float lum = dot(col, vec3(0.2126, 0.7152, 0.0722));
    float lumShift = -0.30 * smooth01(0.92, 0.995, lum) + 0.30 * smooth01(0.08, 0.005, lum);
    col = clamp(col + vec3((grainSignal + lumShift) * grainAmp), 0.0, 1.0);
  }

  fragColor = vec4(clamp(col + dither, 0.0, 1.0), 1.0);
}
`, tt = `#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform vec2 u_viewScale;
uniform vec2 u_pointer;
uniform vec2 u_tap;
uniform vec2 u_focusPoint;
uniform float u_focusAmount;
uniform float u_softness;
uniform float u_energy;
uniform float u_phase;
uniform float u_tapAge;
uniform float u_isLight;
uniform float u_fourSize;
uniform vec2 u_fourOffset;
uniform float u_fourOpacity;
uniform float u_idleEvap;
uniform float u_fourEvap;
uniform float u_motifMode;
uniform sampler2D u_motifSdfMap;

float bell(vec2 p, vec2 c, vec2 r) {
  vec2 q = (p - c) / r;
  return exp(-dot(q, q) * 2.0);
}

float rotBell(vec2 p, vec2 c, vec2 r, float ang) {
  vec2 d = p - c;
  float ca = cos(ang);
  float sa = sin(ang);
  vec2 q = vec2(ca * d.x + sa * d.y, -sa * d.x + ca * d.y) / r;
  return exp(-dot(q, q) * 2.0);
}

void blendOver(inout vec3 baseC, inout float baseA, vec3 srcC, float srcA) {
  float a = clamp(srcA, 0.0, 1.0);
  float outA = a + baseA * (1.0 - a);
  baseC = (srcC * a + baseC * baseA * (1.0 - a)) / max(outA, 0.0001);
  baseA = outA;
}

float sdRect(vec2 p, vec2 a, vec2 b) {
  vec2 d = abs(p - (a + b) * 0.5) - (b - a) * 0.5;
  return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0);
}

// Exact architectural SDF for the geometric "4" in aspect-corrected scene coordinates (Figma 12390:612 / 24300:3221)
float sdFour(vec2 p) {
  vec2 g = vec2((p.x - 0.54) * 1.7778, p.y - 0.50);
  float stem = sdRect(g, vec2(0.052, -0.335), vec2(0.156, 0.335));
  float bar = sdRect(g, vec2(-0.245, 0.084), vec2(0.240, 0.174));
  float leftKnee = sdRect(g, vec2(-0.245, 0.058), vec2(-0.122, 0.174));
  float dOuter = dot(g - vec2(-0.245, 0.058), vec2(-0.8185, -0.5745));
  float dInner = dot(g - vec2(-0.122, 0.084), vec2(0.8348, 0.5506));
  float dSlab = max(dOuter, dInner);
  float dBounds = max(max(-0.335 - g.y, g.y - 0.128), max(-0.245 - g.x, g.x - 0.095));
  float diag = length(max(vec2(dSlab, dBounds), 0.0)) + min(max(dSlab, dBounds), 0.0);
  return min(min(stem, bar), min(diag, max(leftKnee, dOuter)));
}

// 16-bit unpacked bilinear SDF sampler for Google Symbols (512x512 sub-pixel Euclidean Distance Transform)
float unpackSdf(vec2 tc) {
  vec4 c = texture(u_motifSdfMap, clamp(tc, 0.001, 0.999));
  return ((c.r * 255.0 * 256.0 + c.g * 255.0) / 65535.0 - 0.5) * 0.48;
}

float sdSymbol(vec2 p) {
  vec2 g = vec2((p.x - 0.54) * 1.7778, p.y - 0.50);
  vec2 tc = g / 0.96 + 0.5;
  vec2 boxD = abs(tc - 0.5) - 0.494;
  float outsideBox = length(max(boxD, 0.0)) * 0.96;
  if (outsideBox > 0.015) {
    return 0.24 + outsideBox;
  }
  vec2 pix = tc * 512.0 - 0.5;
  vec2 i = floor(pix);
  vec2 f = fract(pix);
  vec2 i00 = (i + vec2(0.5, 0.5)) / 512.0;
  vec2 i10 = (i + vec2(1.5, 0.5)) / 512.0;
  vec2 i01 = (i + vec2(0.5, 1.5)) / 512.0;
  vec2 i11 = (i + vec2(1.5, 1.5)) / 512.0;
  float d = mix(
    mix(unpackSdf(i00), unpackSdf(i10), f.x),
    mix(unpackSdf(i01), unpackSdf(i11), f.x),
    f.y
  );
  return d + outsideBox;
}

float sdMotif(vec2 p) {
  if (u_motifMode > 0.5) {
    return sdSymbol(p);
  }
  return sdFour(p);
}

// Quintic C2-continuous 2D gradient/value noise (zero grid artifacts)
vec2 hash22(vec2 p) {
  vec3 p3 = fract(vec3(p.xyx) * vec3(0.1031, 0.1030, 0.0973));
  p3 += dot(p3, p3.yzx + 33.33);
  return fract((p3.xx + p3.yz) * p3.zy) * 2.0 - 1.0;
}

float gnoise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  vec2 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  return mix(
    mix(dot(hash22(i + vec2(0.0, 0.0)), f - vec2(0.0, 0.0)),
        dot(hash22(i + vec2(1.0, 0.0)), f - vec2(1.0, 0.0)), u.x),
    mix(dot(hash22(i + vec2(0.0, 1.0)), f - vec2(0.0, 1.0)),
        dot(hash22(i + vec2(1.0, 1.0)), f - vec2(1.0, 1.0)), u.x),
    u.y
  ) * 0.5 + 0.5;
}

// Rotated-octave Fractional Brownian Motion (3 octaves normalized to match 4-octave energy)
float fbm(vec2 p) {
  float v = 0.0;
  float amp = 0.5;
  mat2 rot = mat2(0.80, -0.60, 0.60, 0.80);
  for (int i = 0; i < 3; i++) {
    v += amp * gnoise(p);
    p = rot * p * 2.03 + vec2(13.7, 29.3);
    amp *= 0.5;
  }
  return v * 1.0714286;
}

void main() {
  vec2 screen = vec2(v_uv.x, 1.0 - v_uv.y);
  vec2 scene = (screen - 0.5) * u_viewScale + 0.5;
  float a = u_phase * 6.2831853;
  float soft = mix(0.76, 1.34, clamp((u_softness - 15.0) / 65.0, 0.0, 1.0));

  // Calm, poised parallax & breathing so the architectural lines of the motif never wobble
  float s4 = clamp(u_fourSize / 100.0, 0.4, 2.0);
  vec2 fourCenter = vec2(0.50, 0.50) + u_fourOffset;
  vec2 c4 = scene - fourCenter;
  vec2 tilt4 = vec2(
    c4.x * u_pointer.x * 0.022 + c4.y * u_pointer.x * 0.010,
    c4.y * u_pointer.y * 0.020 + c4.x * u_pointer.y * 0.008
  );
  vec2 p4World =
    scene +
    u_pointer * 0.028 +
    tilt4 +
    vec2(sin(a), cos(a) - 1.0) * u_energy * 0.00010;
  vec2 p4 = (p4World - fourCenter) / s4 + vec2(0.54, 0.50);

  // Aspect-corrected coordinate for isotropic fluid/gas noise
  vec2 isoP = vec2((p4.x - 0.54) * 1.7778, p4.y - 0.50);

  // Fast bounding-box and exact SDF early-out: skips ~82% of full-screen pixels outside the motif & outer halo
  if (abs(isoP.x) > 0.72 || abs(isoP.y) > 0.72) {
    fragColor = vec4(0.0);
    return;
  }
  float dExact = sdMotif(p4);
  if (dExact > 0.28 * max(1.0, soft * 0.85)) {
    fragColor = vec4(0.0);
    return;
  }

  // Sub-bit TPDF Interleaved Gradient Noise dither
  float ign1 = fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(0.06711056, 0.00583715))));
  float ign2 = fract(52.9829189 * fract(dot(gl_FragCoord.xy + vec2(17.3, 31.7), vec2(0.06711056, 0.00583715))));
  vec3 dither = vec3(
    ign1 - ign2,
    (ign2 - 0.5) + (ign1 - 0.5),
    fract(ign1 + 0.37) - fract(ign2 + 0.73)
  ) * (1.35 / 255.0);

  // Seamless circular time vector for 100% loop-safe FBM & curl flow
  float flowRate = 0.28 + (u_energy / 36.0) * 0.42;
  vec2 loopVec = vec2(cos(a), sin(a)) * flowRate;
  vec2 loopVec2 = vec2(sin(a * 2.0), -cos(a * 2.0)) * (flowRate * 0.55);

  // Shared 2-stage domain-warped FBM & analytical curl turbulence (drives both idle & hover states)
  vec2 gasCoord = isoP * 5.8 + loopVec * 1.35;
  vec2 warpQ = vec2(
    fbm(gasCoord + vec2(0.0, 0.0)),
    fbm(gasCoord + vec2(5.2, 1.3))
  ) - 0.5;
  vec2 warpR = vec2(
    fbm(gasCoord * 1.85 + 1.6 * warpQ + vec2(1.7, 9.2) + loopVec2),
    fbm(gasCoord * 1.85 + 1.6 * warpQ + vec2(8.3, 2.8) - loopVec2)
  ) - 0.5;

  float gasBillows = fbm(gasCoord * 1.4 + 1.25 * warpR);
  float gasTendrils = smoothstep(0.28, 0.78, fbm(gasCoord * 2.9 - 1.4 * warpQ + loopVec2));
  vec2 curlV = vec2(warpR.y - warpQ.y, -(warpR.x - warpQ.x)) * 2.2;

  // 1. ARCHITECTURAL ANATOMY OF THE MOTIF ("4" OR GOOGLE SYMBOL)
  float idleScale = clamp(u_idleEvap / 58.0, 0.0, 1.65);
  float structuralLock;
  float zoneSum;
  if (u_motifMode > 0.5) {
    float symCrispCore = bell(p4, vec2(0.565, 0.405), vec2(0.095, 0.155));
    structuralLock = clamp(symCrispCore * 0.85, 0.0, 0.92);
    float symLowerLeft = rotBell(p4, vec2(0.445, 0.585), vec2(0.168, 0.208), -0.58);
    float symLowerRight = bell(p4, vec2(0.615, 0.635), vec2(0.128, 0.148)) * 0.80;
    float symTopRim = smoothstep(0.32, 0.16, p4.y) * bell(p4, vec2(0.535, 0.215), vec2(0.135, 0.095)) * 0.45;
    zoneSum = symLowerLeft * 1.02 + symLowerRight + symTopRim;
  } else {
    float innerCounter = bell(p4, vec2(0.525, 0.475), vec2(0.068, 0.125));
    float crispSpine = bell(p4, vec2(0.585, 0.360), vec2(0.055, 0.165));
    structuralLock = clamp(innerCounter * 0.88 + crispSpine * 0.65, 0.0, 0.94);
    float leftKneeZone = rotBell(p4, vec2(0.395, 0.575), vec2(0.128, 0.168), -0.58);
    float diagMistZone = rotBell(p4, vec2(0.445, 0.465), vec2(0.088, 0.158), -0.60) * 0.68;
    float rightTipZone = bell(p4, vec2(0.655, 0.610), vec2(0.075, 0.085)) * 0.76;
    float stemFootZone = smoothstep(0.66, 0.85, p4.y) * bell(p4, vec2(0.595, 0.785), vec2(0.085, 0.120)) * 0.70;
    float topBevelVeil = smoothstep(0.28, 0.15, p4.y) * bell(p4, vec2(0.555, 0.195), vec2(0.095, 0.075)) * 0.40;
    zoneSum = leftKneeZone * 1.02 + diagMistZone + rightTipZone + stemFootZone + topBevelVeil;
  }

  // Laminar FBM silk modulation + domain-warped gaseous pocket texture for the idle state
  vec2 idleFlowCoord = isoP * 3.6 + loopVec * 0.85;
  float silkFbm1 = fbm(idleFlowCoord);
  float silkFbm2 = clamp(silkFbm1 * 0.62 + (warpQ.x + 0.5) * 0.38, 0.0, 1.0);
  float silkMod = mix(0.76, 1.26, silkFbm1 * 0.55 + silkFbm2 * 0.45);
  float idleGasTexture = 0.52 + 0.52 * gasBillows + 0.38 * gasTendrils;

  float idleDiffusion = clamp(
    zoneSum *
      silkMod *
      idleGasTexture *
      (1.0 - structuralLock) *
      idleScale,
    0.0,
    1.0
  );

  // 2. COMPLEX, RESTRAINED GASSY HOVER STATE
  float hoverGasField = 0.0;
  vec2 fromCursor = vec2(0.0);
  if (u_focusAmount > 0.002 && u_fourEvap > 0.5) {
    float hoverScale = clamp(u_fourEvap / 54.0, 0.0, 1.6);
    vec2 cursorPWorld = u_focusPoint + u_pointer * 0.028;
    vec2 cursorP = (cursorPWorld - fourCenter) / s4 + vec2(0.54, 0.50);
    float dCursorTo4 = sdMotif(cursorP) * s4;
    float cursorProximityTo4 = exp(-pow(max(dCursorTo4, 0.0) / 0.18, 2.0));
    float activeHover = u_focusAmount * cursorProximityTo4 * hoverScale;

    fromCursor = vec2((p4World.x - cursorPWorld.x) * 1.7778, p4World.y - cursorPWorld.y);
    float distCursor = length(fromCursor);
    vec2 warpedFromCursor = fromCursor + warpQ * 0.055 + warpR * 0.032;
    float gasEnvelope =
      exp(-dot(warpedFromCursor, warpedFromCursor) / (0.145 * 0.145)) *
      (1.0 - smoothstep(0.11, 0.25, distCursor));
    hoverGasField = clamp(
      gasEnvelope * (0.45 + 0.55 * gasBillows + 0.45 * gasTendrils) * activeHover,
      0.0,
      1.0
    );
  }

  // Tap-triggered surface ripple wave
  float tapWisp = 0.0;
  if (u_tapAge < 3.5) {
    vec2 tapPWorld = u_tap + u_pointer * 0.028;
    vec2 tapP = (tapPWorld - fourCenter) / s4 + vec2(0.54, 0.50);
    float dTapTo4 = sdMotif(tapP) * s4;
    float tapOnFour = 1.0 - smoothstep(0.01, 0.24, max(dTapTo4, 0.0));
    vec2 fromTap = vec2((p4World.x - tapPWorld.x) * 1.7778, p4World.y - tapPWorld.y);
    float tapDist = length(fromTap + warpQ * 0.035);
    float waveRadius = 0.02 + 0.55 * (1.0 - exp(-u_tapAge * 1.4));
    float ringWidth = 0.05 + u_tapAge * 0.028;
    float tapRings = exp(-pow((tapDist - waveRadius) / ringWidth, 2.0));
    tapWisp = clamp(
      tapRings * exp(-u_tapAge * 1.25) * smoothstep(0.0, 0.03, u_tapAge) * tapOnFour,
      0.0,
      1.0
    );
  }

  // 3. MULTI-STRATA VOLUMETRIC & CURL-ADVECTED SDF EVALUATION
  float totalDiffusion = clamp(idleDiffusion + hoverGasField * 0.82 + tapWisp * 0.65, 0.0, 1.0);
  float diffSq = totalDiffusion * totalDiffusion;

  float dCore = dExact;
  float dStratum1 = dExact;
  float dStratum2 = dExact;
  float dStratum3 = dExact;
  if (totalDiffusion >= 0.012) {
    vec2 idleFlowDir = mix(vec2(-0.56, -0.36), vec2(0.46, -0.44), smoothstep(0.46, 0.62, p4.x));
    vec2 idleBuoyantCurl = normalize(curlV * 0.88 + idleFlowDir * 0.92 + vec2(0.0001));
    vec2 idleAdvect =
      (idleBuoyantCurl * (0.46 + 0.54 * gasTendrils) + warpQ * 0.52 + warpR * 0.56) *
      0.034 *
      idleDiffusion;

    vec2 hoverAdvect = vec2(0.0);
    if (hoverGasField > 0.001) {
      vec2 buoyantCurl = normalize(curlV + vec2(fromCursor.x * 1.2, -0.55) + vec2(0.0001));
      hoverAdvect =
        (buoyantCurl * (0.45 + 0.55 * gasTendrils) + warpR * 0.65) *
        0.028 *
        hoverGasField;
    }

    vec2 totalAdvect = idleAdvect + hoverAdvect;
    float warpScale = idleDiffusion * 0.84 + hoverGasField;
    dCore = sdMotif(p4 + totalAdvect * 0.24);
    dStratum1 = sdMotif(p4 + totalAdvect * 0.68 + warpQ * (0.009 * warpScale));
    dStratum2 = sdMotif(p4 + totalAdvect * 1.22 + vec2(-warpQ.y, warpQ.x) * (0.015 * warpScale));
    dStratum3 = sdMotif(p4 + totalAdvect * 1.85 + warpR * (0.022 * warpScale));
  }

  // Crisp architectural edge where totalDiffusion is 0; silky progressive optical caustic where it diffuses
  float baseFeather = mix(0.0022, 0.00075, structuralLock) / s4;
  float sigmaCore = (baseFeather + diffSq * 0.030) * soft;
  float sigmaS1 = (0.0035 + totalDiffusion * 0.048) * soft;
  float sigmaS2 = (0.0080 + totalDiffusion * 0.088) * soft;
  float sigmaS3 = (0.0160 + totalDiffusion * 0.138) * soft;

  float densityCore =
    1.0 - smoothstep(-sigmaCore, sigmaCore, mix(dExact, dCore, smoothstep(0.04, 0.38, totalDiffusion)));
  float densityS1 =
    (1.0 - smoothstep(-sigmaS1, sigmaS1, dStratum1)) * smoothstep(0.02, 0.32, totalDiffusion);
  float densityS2 =
    (1.0 - smoothstep(-sigmaS2, sigmaS2, dStratum2)) *
    smoothstep(0.06, 0.50, totalDiffusion) *
    (0.68 + 0.32 * silkFbm2);
  float densityS3 =
    (1.0 - smoothstep(-sigmaS3, sigmaS3, dStratum3)) *
    smoothstep(0.12, 0.70, totalDiffusion) *
    (0.55 + 0.45 * gasBillows);

  // Gaseous internal marbling for both idle and hover states
  float pocketMask = smoothstep(0.22, 0.76, 1.0 - gasBillows * 0.65 - gasTendrils * 0.35);
  float gasPocketErosion = hoverGasField * pocketMask * 0.46;
  float idlePocketErosion =
    idleDiffusion * pocketMask * 0.34 + idleDiffusion * idleDiffusion * 0.14;
  float coreRetention = clamp(1.0 - gasPocketErosion - idlePocketErosion, 0.30, 1.0);

  // Specular glass rim & internal optical depth
  float innerDepth = max(-dCore, 0.0);
  float rimWidth = 0.016 + totalDiffusion * 0.026;
  float rimHighlight = exp(-(innerDepth * innerDepth) / (rimWidth * rimWidth));
  float gasCaustic =
    (idleDiffusion * 0.60 + hoverGasField) *
    gasTendrils *
    exp(-pow(abs(dStratum1) / 0.030, 2.0)) *
    0.55;

  float bodyLuminance =
    mix(0.82, 1.0, rimHighlight) *
      (1.0 - 0.18 * (idleDiffusion * 0.65 + hoverGasField) * (1.0 - gasBillows)) +
    gasCaustic;

  // Volumetric outer atmospheric halo around the diffusing/evaporating zones
  float posOut = max(dStratum2, 0.0);
  float haloWidth = 0.018 + totalDiffusion * 0.105;
  float outerHalo =
    exp(-(posOut * posOut) / (haloWidth * haloWidth)) *
    (0.18 + 0.82 * totalDiffusion) *
    (0.72 + 0.28 * silkFbm1);

  // Diagonal atmospheric cobalt shadow across the lower-right stem/crossbar intersection
  vec2 shadeP = p4 - vec2(0.595, 0.595);
  vec2 shadeRot = vec2(shadeP.x * 0.78 + shadeP.y * 0.62, -shadeP.x * 0.62 + shadeP.y * 0.78);
  float stemShade = exp(
    -(shadeRot.x * shadeRot.x) / (0.062 * 0.062) -
      (shadeRot.y * shadeRot.y) / (0.155 * 0.155)
  );
  float shadeAtten = 1.0 - 0.42 * stemShade;

  // Composite the multi-strata volumetric body and gaseous plumes
  float compositeBody =
    mix(
      densityCore * bodyLuminance,
      densityCore * bodyLuminance * 0.52 +
        densityS1 * 0.34 +
        densityS2 * 0.22 +
        densityS3 * 0.14,
      smoothstep(0.0, 0.65, totalDiffusion)
    ) *
    coreRetention *
    shadeAtten;

  float compositeGlow =
    (outerHalo * 0.48 + densityS2 * 0.24 + densityS3 * 0.28 + gasCaustic * 0.35) *
    shadeAtten;

  float opacityScale = clamp(u_fourOpacity / 100.0, 0.0, 1.0);
  vec3 fgCol = vec3(0.0);
  float fgAlpha = 0.0;

  if (u_isLight < 0.5) {
    // Dark & Neutral modes: icy-white glass core -> celestial cyan vapour -> deep sapphire mist
    vec3 coreTint = mix(
      vec3(186.0, 224.0, 255.0) / 255.0,
      vec3(248.0, 252.0, 255.0) / 255.0,
      0.50 + 0.50 * rimHighlight * (1.0 - totalDiffusion * 0.35)
    );
    coreTint = mix(coreTint, vec3(102.0, 164.0, 245.0) / 255.0, stemShade * 0.58);
    vec3 gasTint = mix(
      vec3(114.0, 184.0, 255.0) / 255.0,
      vec3(208.0, 238.0, 255.0) / 255.0,
      0.45 + 0.55 * gasTendrils
    );
    vec3 fourCol = mix(coreTint, gasTint, hoverGasField * 0.55);
    vec3 fourGlow = mix(
      vec3(96.0, 168.0, 252.0) / 255.0,
      vec3(198.0, 234.0, 255.0) / 255.0,
      smoothstep(0.68, 0.18, p4.y)
    );
    blendOver(fgCol, fgAlpha, fourGlow, compositeGlow * 0.40 * opacityScale);
    blendOver(fgCol, fgAlpha, fourCol, compositeBody * 0.88 * opacityScale);
  } else {
    // Light mode: pure optical white core & pearlescent sky-ice vapour (Figma 12390:612)
    vec3 fourColLight = mix(
      vec3(235.0, 246.0, 255.0) / 255.0,
      vec3(255.0, 255.0, 255.0) / 255.0,
      0.46 + 0.54 * rimHighlight
    );
    vec3 fourGlowLight = vec3(1.0);
    blendOver(fgCol, fgAlpha, fourGlowLight, compositeGlow * 0.50 * opacityScale);
    blendOver(fgCol, fgAlpha, fourColLight, compositeBody * 0.96 * opacityScale);
  }

  fragColor = vec4(clamp(fgCol + dither, 0.0, 1.0), fgAlpha);
}
`, ot = `#version 300 es
precision highp float;

in vec4 v_style;
in vec2 v_pos;
in vec2 v_pointCoord;

out vec4 fragColor;

uniform sampler2D u_glyph;
uniform float u_darkMode;
uniform float u_glyphRows;
uniform vec3 u_coreColor;
uniform vec3 u_haloColorA;
uniform vec3 u_haloColorB;

float smooth01(float a, float b, float x) {
  float t = clamp((x - a) / (b - a), 0.0, 1.0);
  return t * t * (3.0 - 2.0 * t);
}

void main() {
  vec2 p = v_pointCoord - 0.5;
  float c = cos(v_style.z), s = sin(v_style.z);
  p = mat2(c, -s, s, c) * p + 0.5;
  float inside = step(0.0, p.x) * step(p.x, 1.0) * step(0.0, p.y) * step(p.y, 1.0);
  if (inside < 0.5) {
    discard;
  }

  float px = clamp(p.x, 0.002, 0.998);
  float rowIdx = floor(v_style.w + 0.0001);
  float blurVal = max(0.0, v_style.w - rowIdx);
  float py = (clamp(p.y, 0.002, 0.998) + rowIdx) / max(1.0, u_glyphRows);
  float level = clamp(blurVal * 10.0, 0.0, 3.0);
  float lo = floor(level), hi = min(3.0, lo + 1.0);
  float a0 = texture(u_glyph, vec2(px / 4.0, py)).a;
  float a = texture(u_glyph, vec2((px + lo) / 4.0, py)).a;
  float b = texture(u_glyph, vec2((px + hi) / 4.0, py)).a;
  float blurComp = 1.0 + level * 0.58;
  float mipAlpha = clamp(mix(a, b, fract(level)) * blurComp, 0.0, 1.0);

  if (u_darkMode > 0.5) {
    vec3 haloTint = mix(u_haloColorA, u_haloColorB, smooth01(0.20, 0.68, v_pos.y));
    float darkShape = clamp(max(mipAlpha * 1.15, a0 * (0.74 - 0.08 * level)), 0.0, 1.0);
    float coreMask = smooth01(0.20, 0.72, a0) * (1.0 - clamp(blurVal * 2.0, 0.0, 0.78));
    vec3 darkSpark = mix(haloTint, u_coreColor, coreMask);
    float darkAlpha = clamp(darkShape * min(1.0, v_style.y * 1.32), 0.0, 0.98);
    fragColor = vec4(darkSpark, darkAlpha);
  } else {
    float lightShape = clamp(max(mipAlpha * 1.22, a0 * (0.78 - 0.10 * level)), 0.0, 1.0);
    float coreMask = smooth01(0.18, 0.72, a0);
    vec3 edgeTint = mix(
      u_haloColorA,
      u_haloColorB,
      smooth01(0.55, 0.05, v_pos.y) * smooth01(0.45, 0.95, v_pos.x)
    );
    vec3 lightSpark = mix(edgeTint, u_coreColor, coreMask * 0.92 + 0.08);
    float lightAlpha = clamp(lightShape * min(1.0, v_style.y * 1.45), 0.0, 0.98);
    fragColor = vec4(lightSpark, lightAlpha);
  }
}
`, nt = `#version 300 es
precision highp float;

// Per-instance attributes:
// a_grid: (u / spawnX, vBase / spawnY, layerId, charIdx)
// a_rand: (rSize / r1, rHarmonic, r3)
layout(location = 0) in vec4 a_grid;
layout(location = 1) in vec3 a_rand;

out vec4 v_style;
out vec2 v_pos;
out vec2 v_pointCoord;

uniform vec2 u_resolution;
uniform vec2 u_invViewScale;
uniform vec2 u_metric;
uniform vec2 u_invMetric;
uniform float u_a;
uniform float u_phase;
uniform float u_phaseSpeed;
uniform float u_waveAmp;
uniform float u_isLight;
uniform float u_opacityGain;
uniform float u_promAlpha;
uniform float u_lightBlurScale;
uniform float u_blurAtten;
uniform float u_uniformBaseSize;
uniform float u_falloffT;
uniform vec2 u_sparkOffset;
uniform vec2 u_focusScene;
uniform float u_effectiveHoverAmt;
uniform float u_rawHoverAmt;
uniform float u_hasHover;
uniform vec2 u_hover;
uniform float u_reach;
uniform float u_reachSq;
uniform float u_force;
uniform float u_hoverSizeBoost;
uniform float u_reactCoupling;
uniform vec2 u_rotCosSin;
uniform float u_zoom;
uniform vec2 u_shiftOff;
uniform vec2 u_evapFlowOffset;
uniform float u_evapIdleStrength;
uniform float u_evapHoverStrength;
uniform vec2 u_ptr;
uniform float u_hasTap;
uniform vec2 u_tap;
uniform float u_tapAge;
uniform float u_tapDecay;
uniform float u_spinScale;
uniform float u_radiusFactor;

float smooth01(float a, float b, float x) {
  float t = clamp((x - a) / (b - a), 0.0, 1.0);
  return t * t * (3.0 - 2.0 * t);
}

float expFast(float x) {
  return x < -12.0 ? 0.0 : exp(x);
}

float pow145(float u) {
  return u <= 0.0 ? 0.0 : u * (0.12 * u + 0.88 * sqrt(u));
}

float perspectiveSize(float camProx) {
  return 1.08 / (2.42 - 1.55 * clamp(camProx, 0.0, 1.0));
}

float computeParticleSize(
  float layerBase,
  float camProx,
  float u,
  float v,
  float z,
  float rVal
) {
  if (u_falloffT <= 0.001) return u_uniformBaseSize;
  float edgeTaper =
    smooth01(0.0, 0.32, u) *
    (1.0 - 0.65 * smooth01(0.78, 1.0, u)) *
    (1.0 - 0.60 * smooth01(0.62, 0.98, v));
  float crestBoost = 0.55 + 0.75 * clamp(0.5 + z * 0.65, 0.0, 1.0);
  float depthFactor = perspectiveSize(camProx);
  float rCurve = pow(max(rVal, 0.0), 1.6);
  float particleSpread = 0.38 + 1.45 * rCurve;
  float contrastExp = 0.55 + 1.35 * u_falloffT;
  float rawRatio = max(
    0.16,
    (layerBase / 2.15) * depthFactor * (0.45 + 0.75 * edgeTaper) * crestBoost * particleSpread
  );
  float contrastedRatio = pow(rawRatio, contrastExp);
  return u_uniformBaseSize * (1.0 + (contrastedRatio - 1.0) * min(1.0, u_falloffT * 1.35));
}

float cursorBlurShift(float x, float y, float baseBlur) {
  if (u_effectiveHoverAmt < 0.001) return baseBlur;
  float sx = x + u_sparkOffset.x;
  float sy = y + u_sparkOffset.y;
  float w = sin(sy * 14.0 + sx * 9.0 + u_a * 2.0) * 0.045;
  float dx = (sx - u_focusScene.x) * 1.35 + w;
  float dy = sy - u_focusScene.y - w * 0.7;
  float u1 = dx * 0.82 - dy * 0.57;
  float v1 = dx * 0.57 + dy * 0.82;
  float plume = expFast(-(u1 * u1) / 0.055 - (v1 * v1) / 0.016);
  return clamp(
    baseBlur * (1.0 - 0.35 * min(1.0, u_effectiveHoverAmt) * plume) +
      0.08 * u_effectiveHoverAmt * plume,
    0.002,
    0.28
  );
}

void main() {
  float u = a_grid.x;
  float vBase = a_grid.y;
  float layerId = a_grid.z;
  float charIdx = a_grid.w;
  float rSize = a_rand.x;
  float rHarmonic = a_rand.y;
  float r3 = a_rand.z;

  float x, y, z, size, alpha, blur, spin, parallaxDepth;
  bool isMeshLayer = layerId < 3.5;

  if (layerId < 1.5) {
    // === LAYER 1: Upper-Right 3D Warped Surface Canopy Mesh ===
    float v = fract(vBase + u_phaseSpeed * 0.35);
    float wavePhase1 = u * 5.2 + v * 3.8 - u_a;
    float wavePhase2 = u * 3.4 - v * 5.6 + u_a;
    z = (0.58 * sin(wavePhase1) + 0.42 * cos(wavePhase2)) * u_waveAmp;
    float dZdu = (0.58 * 5.2 * cos(wavePhase1) - 0.42 * 3.4 * sin(wavePhase2)) * u_waveAmp * 0.0045;
    float dZdv = (0.58 * 3.8 * cos(wavePhase1) + 0.42 * 5.6 * sin(wavePhase2)) * u_waveAmp * 0.0045;

    float baseX = 0.46 + u * 0.54 - v * 0.14;
    float baseY = -0.04 + v * 0.62 - (1.0 - u) * 0.10;
    x = baseX + dZdu + 0.018 * sin(v * 4.4 + u * 2.6 - u_a) * u_waveAmp;
    y = baseY + dZdv - z * 0.028 * u_waveAmp;

    float edgeFade =
      smooth01(0.02, 0.22, u) *
      (1.0 - smooth01(0.88, 0.99, u)) *
      smooth01(0.02, 0.18, v) *
      (1.0 - smooth01(0.74, 0.97, v));
    float rightBoost =
      mix(0.42, 0.58, u_isLight) +
      mix(0.58, 0.42, u_isLight) * smooth01(0.54, 0.92, x) * smooth01(0.58, 0.06, y);
    float camProx = clamp(0.18 + 0.24 * smooth01(0.08, 0.90, v) + 0.18 * z, 0.0, 1.0);
    float baseBlur =
      min(0.26, 0.004 + 0.13 * smooth01(0.25, 0.85, 1.0 - u) + 0.07 * abs(z - 0.15)) *
      u_lightBlurScale;
    blur = cursorBlurShift(x, y, baseBlur);
    size = computeParticleSize(1.9, camProx, u, v, z, rSize);
    float ridgeHighlight = 0.78 + 0.32 * max(0.0, z);
    alpha = mix(0.38, 0.46, u_isLight) * edgeFade * rightBoost * ridgeHighlight * u_promAlpha;
    spin = 0.22 + u * 0.48 - v * 0.18;
    parallaxDepth = 0.010 + 0.012 * camProx;
  } else if (layerId < 2.5) {
    // === LAYER 2: Mid-Depth 3D Undulating Wave Mesh ===
    float v = fract(vBase + u_phaseSpeed * 0.55 + 0.33);
    z = (sin(u * 4.6 - v * 3.2 - u_a) + 0.45 * sin(v * 5.2 + u * 2.8 + u_a)) * u_waveAmp;
    float arch = sin(u * 4.2 - 0.7) + 0.28 * sin(v * 4.4 - u * 2.4 - u_a);
    x = 0.28 + u * 0.66 + 0.026 * sin(v * 4.8 + u * 2.3 - u_a) * u_waveAmp;
    y = 0.06 + v * 0.68 + 0.052 * arch * u_waveAmp - z * 0.024;
    float camProx = clamp(0.26 + 0.30 * smooth01(0.06, 0.92, v) + 0.22 * z, 0.0, 1.0);
    float focalDist = abs(v - 0.44 + 0.32 * (u - 0.58) - z * 0.15);
    float baseBlur = min(0.28, 0.01 + 0.21 * smooth01(0.04, 0.44, focalDist)) * u_lightBlurScale;
    blur = cursorBlurShift(x, y, baseBlur);
    float mask =
      smooth01(0.04, 0.24, u) *
      (1.0 - smooth01(0.85, 0.99, u)) *
      smooth01(0.04, 0.22, v) *
      (1.0 - smooth01(0.80, 0.97, v));
    size = computeParticleSize(2.2, camProx, u, v, z, rSize);
    alpha =
      mix(0.34, 0.44, u_isLight) *
      mask *
      (1.0 - blur * u_blurAtten) *
      u_promAlpha *
      (0.85 + 0.15 * rSize);
    spin = 0.28 + u * 0.52 - v * 0.22;
    parallaxDepth = 0.016 + 0.018 * camProx;
  } else if (layerId < 3.5) {
    // === LAYER 3: Foreground 3D Ribbon Mesh ===
    float v = fract(vBase + u_phaseSpeed * 0.68 + 0.67);
    z = (0.65 * cos(u * 4.0 + v * 3.6 - u_a) + 0.35 * sin(v * 6.0 - u_a)) * u_waveAmp;
    x = 0.38 + u * 0.58 + 0.028 * cos(v * 4.8 + u * 2.2 - u_a) * u_waveAmp;
    y = 0.04 + v * 0.64 + 0.046 * sin(u * 3.8 - 0.6) * u_waveAmp - z * 0.032;
    float wingMask =
      smooth01(0.04, 0.22, u) *
      (1.0 - smooth01(0.86, 0.99, u)) *
      smooth01(0.04, 0.20, v) *
      (1.0 - smooth01(0.78, 0.96, v));
    float camProx = clamp(0.38 + 0.34 * smooth01(0.08, 0.92, v) + 0.24 * z, 0.0, 1.0);
    float focal = abs(v - 0.38 - 0.26 * (u - 0.58) + z * 0.12);
    float baseBlur = min(0.26, 0.004 + 0.22 * smooth01(0.06, 0.45, focal)) * u_lightBlurScale;
    blur = cursorBlurShift(x, y, baseBlur);
    size = computeParticleSize(2.45, camProx, u, v, z, rSize);
    alpha = mix(0.38, 0.48, u_isLight) * wingMask * (1.0 - blur * u_blurAtten) * u_promAlpha;
    spin = 0.32 + u * 0.40 - v * 0.15;
    parallaxDepth = 0.024 + 0.026 * camProx;
  } else {
    // === LAYER 4: Large "4" Motif Idle & Hover Evaporation Micro-Sparkticles ===
    float spawnX = u;
    float spawnY = vBase;
    float isIdleZone = layerId - 4.0;
    float r1 = rSize;
    float life = fract(r1 + u_phase);
    float curlAngle = r1 * 6.283 + life * 2.85 + u_a;
    float driftRadius = life * (0.056 + 0.048 * r3);
    x = spawnX + (-0.48 * driftRadius + cos(curlAngle) * 0.024 * life);
    y = spawnY + (-0.68 * driftRadius + sin(curlAngle) * 0.022 * life);
    z = 0.45 + life * 0.4;

    float cursorTrigger = 0.0;
    if (u_evapHoverStrength > 0.002) {
      float cdx = (spawnX - u_focusScene.x) * 1.6;
      float cdy = spawnY - u_focusScene.y;
      cursorTrigger = expFast(-(cdx * cdx + cdy * cdy) / (0.16 * 0.16)) * u_evapHoverStrength;
    }
    float idleTrigger = isIdleZone * u_evapIdleStrength * mix(0.52, 0.62, u_isLight);
    float activeTrigger = min(1.0, idleTrigger + cursorTrigger * 0.85);
    float env = sin(life * 3.14159265) * (1.0 - life * 0.35) * activeTrigger;
    if (env <= 0.012) {
      gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
      return;
    }
    blur = (0.012 + life * 0.16) * u_lightBlurScale;
    size = (1.35 + 1.45 * (1.0 - life) * (0.7 + 0.6 * r3)) * (u_uniformBaseSize / 2.15);
    alpha = env * mix(0.58, 0.78, u_isLight) * u_opacityGain;
    spin = r1 * 3.14 + life * 0.8;
    parallaxDepth = 0.028;
  }

  if (alpha < 0.005) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }

  if (isMeshLayer) {
    // Scene zoom, rotation, and shift
    float rx = (x - 0.5) * u_zoom;
    float ry = (y - 0.5) * u_zoom;
    x = u_rotCosSin.x * rx - u_rotCosSin.y * ry + u_shiftOff.x;
    y = u_rotCosSin.y * rx + u_rotCosSin.x * ry + u_shiftOff.y;
  } else {
    x -= u_evapFlowOffset.x;
    y -= u_evapFlowOffset.y;
  }

  // Multi-plane 3D pointer parallax modulated by vertex 3D height
  float effParallax = parallaxDepth * (1.0 + z * 0.65);
  x -= u_ptr.x * effParallax;
  y -= u_ptr.y * effParallax;

  float localScreenX = (x - 0.5) * u_invViewScale.x + 0.5;
  float localScreenY = (y - 0.5) * u_invViewScale.y + 0.5;
  float fadeStart = 0.54 + 0.26 * smooth01(0.32, 0.95, localScreenX);
  alpha *= pow145(1.0 - smooth01(fadeStart, 0.90, localScreenY));

  if (isMeshLayer) {
    x += u_sparkOffset.x;
    y += u_sparkOffset.y;
  }

  // Tap-triggered 3D surface ripple wave
  float rippleBoost = 0.0;
  if (u_hasTap > 0.5) {
    float dxTap = x - u_tap.x;
    float dyTap = y - u_tap.y;
    float dTap = sqrt(dxTap * dxTap + dyTap * dyTap) + 0.00001;
    float wave = sin(dTap * 34.0 - u_tapAge * 7.5) * u_tapDecay;
    x -= (dxTap / dTap) * wave * 0.012;
    y -= (dyTap / dTap) * wave * 0.012;
    rippleBoost = max(0.0, wave * 0.25);
  }

  x = (x - 0.5) * u_invViewScale.x + 0.5;
  y = (y - 0.5) * u_invViewScale.y + 0.5;

  // 3D Mesh Cursor Reactivity & Individual Sparkticle Hover Size Scaling
  float cursorLift = 0.0;
  float sizeMultiplier = 1.0;
  if (u_hasHover > 0.5) {
    float dx = (x - u_hover.x) * u_metric.x;
    float dy = (y - u_hover.y) * u_metric.y;
    float distSq = dx * dx + dy * dy;
    if (distSq < u_reachSq) {
      float distance = sqrt(distSq);
      float falloff = 1.0 - smooth01(0.0, u_reach, distance);
      float smoothBell = pow145(falloff);

      if (u_effectiveHoverAmt > 0.005) {
        float push = u_reach * 0.85 * u_force * smoothBell * (0.85 + 0.3 * z);
        float wave3D = sin(distance * 22.0 - u_a * 2.0) * 0.007 * u_effectiveHoverAmt * smoothBell;
        vec2 dir = distance > 0.00001
          ? vec2(dx, dy) / distance
          : vec2(cos(rHarmonic), sin(rHarmonic));
        float swirl = 0.0045 * u_effectiveHoverAmt * smoothBell * (z * 0.5 + 0.5);
        x += (dir.x * (push + wave3D) - dir.y * swirl) * u_invMetric.x;
        y += (dir.y * (push + wave3D) + dir.x * swirl) * u_invMetric.y;
        cursorLift = smoothBell * u_effectiveHoverAmt * 0.34;
      }

      if (u_hoverSizeBoost > 0.005) {
        float individualHarmonic = 0.78 + 0.44 * sin(u_a * 2.0 + rHarmonic);
        sizeMultiplier =
          1.0 +
          u_hoverSizeBoost * u_rawHoverAmt * u_reactCoupling * smoothBell * individualHarmonic * 1.15;
        blur = max(0.002, blur * (1.0 - 0.55 * smoothBell * min(1.0, u_hoverSizeBoost * u_rawHoverAmt)));
      }
    }
  }

  float finalSize = size * sizeMultiplier * (1.0 + cursorLift * 0.22 + rippleBoost);
  float finalAlpha = min(0.98, alpha * (1.0 + rippleBoost));
  float safeBlur = clamp(blur, 0.001, 0.48);
  float spriteSize = finalSize * (1.0 + safeBlur * 1.5) * u_invViewScale.y;
  float radius = spriteSize * u_radiusFactor;
  float rxBound = radius / u_resolution.x;
  float ryBound = radius / u_resolution.y;

  if (
    x < -rxBound ||
    x > 1.0 + rxBound ||
    y < -ryBound ||
    y > 1.0 + ryBound ||
    finalAlpha < 0.005
  ) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    return;
  }

  // Instanced 4-vertex quad matching gl.POINTS footprint & gl_PointCoord orientation
  vec2 corner = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1));
  float pointSizePx = spriteSize * 1.6 * u_resolution.y / 1080.0;
  vec2 screenPos = vec2(x, y) + (corner - 0.5) * (pointSizePx / u_resolution);

  v_pos = vec2(x, y);
  v_pointCoord = corner;
  v_style = vec4(spriteSize, finalAlpha, spin * u_spinScale, charIdx + safeBlur);
  gl_Position = vec4(screenPos.x * 2.0 - 1.0, 1.0 - screenPos.y * 2.0, 0.0, 1.0);
}
`, de = `#version 300 es
out vec2 v_uv;
void main() {
  vec2 v = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  v_uv = v;
  gl_Position = vec4(v * 2.0 - 1.0, 0.0, 1.0);
}`, at = [
  "u_resolution",
  "u_viewScale",
  "u_shift",
  "u_pointer",
  "u_tap",
  "u_focusPoint",
  "u_focusAmount",
  "u_zoom",
  "u_rotCosSin",
  "u_beamIntensity",
  "u_grain",
  "u_grainOffset",
  "u_energy",
  "u_phase",
  "u_tapAge",
  "u_beamRange",
  "u_corners[0]",
  "u_lobeCount",
  "u_lobeColors[0]",
  "u_lobeCentersRadii[0]",
  "u_lobeCosSin[0]",
  "u_lobeParams[0]",
  "u_noiseMap"
], it = [
  "u_viewScale",
  "u_pointer",
  "u_tap",
  "u_focusPoint",
  "u_focusAmount",
  "u_softness",
  "u_energy",
  "u_phase",
  "u_tapAge",
  "u_isLight",
  "u_fourSize",
  "u_fourOffset",
  "u_fourOpacity",
  "u_idleEvap",
  "u_fourEvap",
  "u_motifMode",
  "u_motifSdfMap"
], rt = [
  "u_resolution",
  "u_invViewScale",
  "u_metric",
  "u_invMetric",
  "u_a",
  "u_phase",
  "u_phaseSpeed",
  "u_waveAmp",
  "u_isLight",
  "u_opacityGain",
  "u_promAlpha",
  "u_lightBlurScale",
  "u_blurAtten",
  "u_uniformBaseSize",
  "u_falloffT",
  "u_sparkOffset",
  "u_focusScene",
  "u_effectiveHoverAmt",
  "u_rawHoverAmt",
  "u_hasHover",
  "u_hover",
  "u_reach",
  "u_reachSq",
  "u_force",
  "u_hoverSizeBoost",
  "u_reactCoupling",
  "u_rotCosSin",
  "u_zoom",
  "u_shiftOff",
  "u_evapFlowOffset",
  "u_evapIdleStrength",
  "u_evapHoverStrength",
  "u_ptr",
  "u_hasTap",
  "u_tap",
  "u_tapAge",
  "u_tapDecay",
  "u_spinScale",
  "u_radiusFactor",
  "u_glyph",
  "u_darkMode",
  "u_glyphRows",
  "u_coreColor",
  "u_haloColorA",
  "u_haloColorB"
];
function ae(t, e, o) {
  const a = t.createProgram();
  if (!a) throw new Error("Failed to create WebGLProgram.");
  const s = [];
  for (const [n, i] of [
    [t.VERTEX_SHADER, e],
    [t.FRAGMENT_SHADER, o]
  ]) {
    const r = t.createShader(n);
    if (!r) {
      for (const l of s) t.deleteShader(l);
      throw t.deleteProgram(a), new Error("Failed to create WebGLShader.");
    }
    t.shaderSource(r, i), t.compileShader(r), t.attachShader(a, r), s.push(r);
  }
  if (t.linkProgram(a), !t.getProgramParameter(a, t.LINK_STATUS)) {
    const n = t.getProgramInfoLog(a) ?? "", i = s.map((r) => t.getShaderInfoLog(r) ?? "").filter(Boolean).join(`
`);
    for (const r of s) t.deleteShader(r);
    throw t.deleteProgram(a), new Error(i || n || "Program link failed");
  }
  for (const n of s) t.deleteShader(n);
  return a;
}
const O = (t) => {
  const e = Math.sin(t * 127.1 + 311.7) * 43758.5453;
  return e - Math.floor(e);
}, pe = (t, e, o) => {
  const a = Math.max(0, Math.min(1, (o - t) / (e - t)));
  return a * a * (3 - 2 * a);
};
function ve(t, e) {
  const o = t.createVertexArray(), a = t.createBuffer();
  return t.bindVertexArray(o), t.bindBuffer(t.ARRAY_BUFFER, a), t.bufferData(t.ARRAY_BUFFER, e, t.STATIC_DRAW), t.enableVertexAttribArray(0), t.vertexAttribPointer(0, 4, t.FLOAT, !1, 28, 0), t.vertexAttribDivisor(0, 1), t.enableVertexAttribArray(1), t.vertexAttribPointer(1, 3, t.FLOAT, !1, 28, 16), t.vertexAttribDivisor(1, 1), t.bindVertexArray(null), t.bindBuffer(t.ARRAY_BUFFER, null), { vao: o, buffer: a };
}
function ge(t, e, o, a, s, n) {
  if (!t.enabled || !e.enabled || t.density <= 0 || !o)
    return null;
  const i = a + t.seedOffset, r = 0.52 + t.density * 85e-4, l = Math.max(1, o.charCount), h = o.spaceMask, p = (u) => l > 1 ? Math.max(l, Math.round(u / l) * l) : u, f = l === 2 ? 1 : 2, m = [], d = [], y = Math.max(8, Math.round(52 * r)), M = p(Math.max(6, Math.round(32 * r)));
  for (let u = 0; u < M; u++)
    for (let c = 0; c < y; c++) {
      const _ = l > 1 ? (c + u * f) % l : 0;
      if (h[_]) continue;
      const I = c / (y - 1), C = u / M, b = O(c * 3.1 + u * 7.7 + i), P = c * 17 + u * 31, F = O(P * 1.7 + i) * 6.283;
      m.push(I, C, 1, _, b, F, 0);
    }
  const E = Math.max(8, Math.round(46 * r)), A = p(Math.max(6, Math.round(28 * r)));
  for (let u = 0; u < A; u++)
    for (let c = 0; c < E; c++) {
      const _ = l > 1 ? (c + u * f) % l : 0;
      if (h[_]) continue;
      const I = c / (E - 1), C = u / A, b = O(c * 5.3 + u * 9.1 + i), P = c * 23 + u * 41 + 1e3, F = O(P * 1.7 + i) * 6.283;
      m.push(I, C, 2, _, b, F, 0);
    }
  const S = Math.max(8, Math.round(44 * r)), L = p(Math.max(6, Math.round(26 * r)));
  for (let u = 0; u < L; u++)
    for (let c = 0; c < S; c++) {
      const _ = l > 1 ? (c + u * f) % l : 0;
      if (h[_]) continue;
      const I = c / (S - 1), C = u / L, b = O(c * 7.9 + u * 11.3 + i), P = c * 29 + u * 53 + 3e3, F = O(P * 1.7 + i) * 6.283;
      d.push(I, C, 3, _, b, F, 0);
    }
  if (s != null && s.enabled) {
    const u = Math.max(0.4, Math.min(2, s.size / 100)), c = s.posX * 75e-4, _ = s.posY * 75e-4, I = Math.round(115 * r), C = s.motifType === "symbol" && (n != null && n.contourPoints.length) ? n.contourPoints : null;
    for (let b = 0; b < I; b++) {
      const P = b % l;
      if (h[P]) continue;
      const F = O(b * 13.7 + a), w = O(b * 29.3 + a), D = O(b * 47.1 + a);
      let B, z, X;
      if (C) {
        const U = C[Math.floor(w * C.length) % C.length];
        B = U[0] + (O(b * 5.3) - 0.5) * 0.026, z = U[1] + (O(b * 9.7) - 0.5) * 0.026, X = U[2];
      } else if (w < 0.52) {
        const U = D * 0.65;
        B = 0.395 + U * 0.14 + (O(b * 5.3) - 0.5) * 0.038, z = 0.585 - U * 0.24 + (O(b * 9.7) - 0.5) * 0.038, X = 1;
      } else if (w < 0.78) {
        const U = D;
        B = 0.4 + U * 0.25, z = 0.575 + (O(b * 11.3) - 0.5) * 0.035, X = pe(0.55, 0.95, U) * 0.8;
      } else {
        const U = D;
        B = 0.585 + (O(b * 7.1) - 0.5) * 0.04, z = 0.32 + U * 0.44, X = pe(0.65, 0.95, U) * 0.72;
      }
      B = (B - 0.54) * u + 0.5 + c, z = (z - 0.5) * u + 0.5 + _;
      const q = b + 8e3, G = O(q * 1.7 + i) * 6.283;
      d.push(B, z, 4 + X, P, F, G, D);
    }
  }
  const R = new Float32Array(m), T = new Float32Array(d);
  return {
    settings: t,
    glyphCfg: e,
    atlasEntry: o,
    backData: R,
    backCount: R.length / 7,
    frontData: T,
    frontCount: T.length / 7
  };
}
function st(t, e) {
  const o = t.createTexture();
  return t.bindTexture(t.TEXTURE_2D, o), t.texParameteri(t.TEXTURE_2D, t.TEXTURE_WRAP_S, t.CLAMP_TO_EDGE), t.texParameteri(t.TEXTURE_2D, t.TEXTURE_WRAP_T, t.CLAMP_TO_EDGE), t.texParameteri(t.TEXTURE_2D, t.TEXTURE_MAG_FILTER, t.LINEAR), t.texImage2D(
    t.TEXTURE_2D,
    0,
    t.RGBA,
    e.width,
    e.height,
    0,
    t.RGBA,
    t.UNSIGNED_BYTE,
    e.pixels
  ), e.atlasRows === 1 ? (t.generateMipmap(t.TEXTURE_2D), t.texParameteri(
    t.TEXTURE_2D,
    t.TEXTURE_MIN_FILTER,
    t.LINEAR_MIPMAP_LINEAR
  )) : t.texParameteri(t.TEXTURE_2D, t.TEXTURE_MIN_FILTER, t.LINEAR), o;
}
const ee = 512;
let ie = null;
function lt() {
  if (ie) return ie;
  const t = new Uint8Array(ee * ee * 2);
  let e = 1831565813;
  const o = () => {
    e = e + 2654435769 | 0;
    let a = e;
    return a = Math.imul(a ^ a >>> 16, 569420461), a = Math.imul(a ^ a >>> 15, 1935289751), (a ^ a >>> 15) >>> 24 & 255;
  };
  for (let a = 0; a < t.length; a += 2)
    t[a] = o(), t[a + 1] = o(), o();
  return ie = t, t;
}
function ct(t) {
  const e = t.createTexture();
  return t.bindTexture(t.TEXTURE_2D, e), t.texParameteri(t.TEXTURE_2D, t.TEXTURE_MIN_FILTER, t.NEAREST), t.texParameteri(t.TEXTURE_2D, t.TEXTURE_MAG_FILTER, t.NEAREST), t.texParameteri(t.TEXTURE_2D, t.TEXTURE_WRAP_S, t.REPEAT), t.texParameteri(t.TEXTURE_2D, t.TEXTURE_WRAP_T, t.REPEAT), t.texImage2D(
    t.TEXTURE_2D,
    0,
    t.RG8,
    ee,
    ee,
    0,
    t.RG,
    t.UNSIGNED_BYTE,
    lt()
  ), e;
}
function ft(t, e) {
  const o = t.createTexture();
  return t.bindTexture(t.TEXTURE_2D, o), t.texParameteri(t.TEXTURE_2D, t.TEXTURE_MIN_FILTER, t.NEAREST), t.texParameteri(t.TEXTURE_2D, t.TEXTURE_MAG_FILTER, t.NEAREST), t.texParameteri(t.TEXTURE_2D, t.TEXTURE_WRAP_S, t.CLAMP_TO_EDGE), t.texParameteri(t.TEXTURE_2D, t.TEXTURE_WRAP_T, t.CLAMP_TO_EDGE), t.texImage2D(
    t.TEXTURE_2D,
    0,
    t.RG8,
    J,
    J,
    0,
    t.RG,
    t.UNSIGNED_BYTE,
    e.rgBytes
  ), o;
}
class ut {
  constructor() {
    v(this, "surface");
    v(this, "gl");
    v(this, "controller", new AbortController());
    v(this, "instances", /* @__PURE__ */ new Set());
    v(this, "atlasTextures", /* @__PURE__ */ new Map());
    v(this, "motifTextures", /* @__PURE__ */ new Map());
    v(this, "gradientProgram");
    v(this, "motifProgram", null);
    v(this, "particleProgram");
    v(this, "quadVao");
    v(this, "noiseTexture");
    v(this, "gradUniforms", {});
    v(this, "motifUniforms", {});
    v(this, "partUniforms", {});
    v(this, "contextLost", !1);
    v(this, "uploadedPresetId", "");
    v(this, "cornersData", new Float32Array(4 * 3));
    v(this, "lobeColorsData", new Float32Array(W * 3));
    v(this, "lobeCentersRadiiData", new Float32Array(
      W * 4
    ));
    v(this, "lobeCosSinData", new Float32Array(W * 2));
    v(this, "lobeParamsData", new Float32Array(W * 3));
    this.surface = document.createElement("canvas");
    const e = this.surface.getContext("webgl2", {
      alpha: !1,
      antialias: !1,
      depth: !1,
      stencil: !1
    });
    if (!e)
      throw new Error("WebGL2 is not available.");
    this.gl = e, this.setupContext();
    const { signal: o } = this.controller;
    this.surface.addEventListener(
      "webglcontextlost",
      (a) => {
        a.preventDefault(), this.contextLost = !0;
      },
      { signal: o }
    ), this.surface.addEventListener(
      "webglcontextrestored",
      () => {
        this.contextLost = !1, this.uploadedPresetId = "", this.atlasTextures.clear(), this.motifTextures.clear(), this.setupContext();
        for (const a of this.instances)
          a.handleContextRestored();
      },
      { signal: o }
    );
  }
  setupContext() {
    const e = this.gl;
    this.gradientProgram = ae(e, de, et), this.motifProgram = null, this.particleProgram = ae(e, nt, ot), this.quadVao = e.createVertexArray(), this.noiseTexture = ct(e), this.gradUniforms = {};
    for (const l of at)
      this.gradUniforms[l] = e.getUniformLocation(
        this.gradientProgram,
        l
      );
    this.motifUniforms = {}, this.partUniforms = {};
    for (const l of rt)
      this.partUniforms[l] = e.getUniformLocation(
        this.particleProgram,
        l
      );
    const o = this.gradUniforms;
    e.useProgram(this.gradientProgram), e.uniform1i(o.u_noiseMap, 0), e.uniform1f(o.u_energy, g.energy);
    const a = this.partUniforms, s = Math.max(0, Math.min(100, g.hoverReactivity)) / 65, n = Math.max(0, Math.min(100, g.hoverSize)) / 50, i = 0.07 + Math.max(0, Math.min(100, g.reach)) * 36e-4, r = 0.35 + 0.65 * Math.min(1.5, s);
    e.useProgram(this.particleProgram), e.uniform1i(a.u_glyph, 0), e.uniform1f(a.u_reach, i), e.uniform1f(a.u_reachSq, i * i), e.uniform1f(a.u_hoverSizeBoost, n), e.uniform1f(a.u_reactCoupling, r);
  }
  ensureMotifProgram() {
    if (this.motifProgram) return this.motifProgram;
    const e = this.gl;
    this.motifProgram = ae(e, de, tt), this.motifUniforms = {};
    for (const a of it)
      this.motifUniforms[a] = e.getUniformLocation(this.motifProgram, a);
    const o = this.motifUniforms;
    return e.useProgram(this.motifProgram), e.uniform1i(o.u_motifSdfMap, 1), e.uniform1f(o.u_softness, g.softness), e.uniform1f(o.u_energy, g.energy), this.uploadedPresetId = "", this.motifProgram;
  }
  getAtlasTexture(e) {
    let o = this.atlasTextures.get(e.key);
    return o || (o = st(this.gl, e), this.atlasTextures.set(e.key, o)), o;
  }
  getMotifTexture(e) {
    let o = this.motifTextures.get(e.key);
    return o || (o = ft(this.gl, e), this.motifTextures.set(e.key, o)), o;
  }
  syncPresetUniforms(e) {
    if (this.uploadedPresetId === e.id) return;
    this.uploadedPresetId = e.id;
    const { corners: o, lobes: a, softScale: s } = e.gradient, i = (0.76 + (1.34 - 0.76) * Math.max(0, Math.min(1, (g.softness - 15) / 65))) * s;
    this.cornersData.set(o.topLeft, 0), this.cornersData.set(o.topRight, 3), this.cornersData.set(o.bottomLeft, 6), this.cornersData.set(o.bottomRight, 9), this.lobeColorsData.fill(0), this.lobeCentersRadiiData.fill(0), this.lobeCosSinData.fill(0), this.lobeParamsData.fill(0);
    const r = Math.min(a.length, W);
    for (let m = 0; m < r; m++) {
      const d = a[m];
      this.lobeColorsData.set(d.color, m * 3), this.lobeCentersRadiiData[m * 4] = d.center[0], this.lobeCentersRadiiData[m * 4 + 1] = d.center[1], this.lobeCentersRadiiData[m * 4 + 2] = 1 / Math.max(1e-5, d.radii[0] * i), this.lobeCentersRadiiData[m * 4 + 3] = 1 / Math.max(1e-5, d.radii[1] * i), this.lobeCosSinData[m * 2] = Math.cos(d.angle), this.lobeCosSinData[m * 2 + 1] = Math.sin(d.angle), this.lobeParamsData[m * 3] = d.weight, this.lobeParamsData[m * 3 + 1] = d.beamModulated ? 1 : 0, this.lobeParamsData[m * 3 + 2] = d.maxAlpha;
    }
    const l = this.gl, h = this.gradUniforms;
    l.useProgram(this.gradientProgram), l.uniform2f(
      h.u_beamRange,
      e.gradient.beamRange[0],
      e.gradient.beamRange[1]
    ), l.uniform3fv(h["u_corners[0]"], this.cornersData), l.uniform1i(h.u_lobeCount, r), l.uniform3fv(h["u_lobeColors[0]"], this.lobeColorsData), l.uniform4fv(h["u_lobeCentersRadii[0]"], this.lobeCentersRadiiData), l.uniform2fv(h["u_lobeCosSin[0]"], this.lobeCosSinData), l.uniform3fv(h["u_lobeParams[0]"], this.lobeParamsData);
    const p = e.colorMode === "light";
    this.motifProgram && (l.useProgram(this.motifProgram), l.uniform1f(this.motifUniforms.u_isLight, p ? 1 : 0));
    const f = this.partUniforms;
    l.useProgram(this.particleProgram), l.uniform1f(f.u_isLight, p ? 1 : 0), l.uniform1f(f.u_lightBlurScale, p ? 0.56 : 1), l.uniform1f(f.u_darkMode, p ? 0 : 1), l.uniform3f(f.u_coreColor, ...e.particles.coreColor), l.uniform3f(f.u_haloColorA, ...e.particles.haloColorA), l.uniform3f(f.u_haloColorB, ...e.particles.haloColorB);
  }
  ensureSurfaceSize(e, o) {
    if (this.surface.width >= e && this.surface.height >= o) return;
    let a = e, s = o;
    for (const n of this.instances)
      n.width > a && (a = n.width), n.height > s && (s = n.height);
    this.surface.width = a, this.surface.height = s;
  }
  shrinkSurfaceToFit() {
    let e = 1, o = 1;
    for (const a of this.instances)
      a.width > e && (e = a.width), a.height > o && (o = a.height);
    (this.surface.width > e || this.surface.height > o) && (this.surface.width = e, this.surface.height = o);
  }
  register(e) {
    this.instances.add(e);
  }
  unregister(e) {
    return this.instances.delete(e), this.instances.size > 0 ? (this.shrinkSurfaceToFit(), !1) : (this.destroy(), !0);
  }
  destroy() {
    var o;
    this.controller.abort();
    const e = this.gl;
    if (!this.contextLost && !e.isContextLost()) {
      e.deleteTexture(this.noiseTexture);
      for (const a of this.atlasTextures.values())
        e.deleteTexture(a);
      this.atlasTextures.clear();
      for (const a of this.motifTextures.values())
        e.deleteTexture(a);
      this.motifTextures.clear(), e.deleteVertexArray(this.quadVao), e.deleteProgram(this.gradientProgram), this.motifProgram && e.deleteProgram(this.motifProgram), e.deleteProgram(this.particleProgram), (o = e.getExtension("WEBGL_lose_context")) == null || o.loseContext();
    }
    this.surface.width = 0, this.surface.height = 0;
  }
}
let N = null;
function mt(t) {
  return N || (N = new ut()), N.register(t), N;
}
function ht(t) {
  N && N.unregister(t) && (N = null);
}
class dt {
  constructor(e) {
    v(this, "options");
    v(this, "canvas");
    v(this, "ctx2d");
    v(this, "shared");
    v(this, "mesh1Cpu");
    v(this, "mesh2Cpu");
    v(this, "mesh1", null);
    v(this, "mesh2", null);
    v(this, "motifTexture", null);
    v(this, "variation");
    v(this, "grain");
    v(this, "animateGrain");
    v(this, "prominence");
    v(this, "heroMotif");
    v(this, "motifOffsetX");
    v(this, "motifOffsetY");
    v(this, "evapIdleStrength");
    v(this, "rotation");
    v(this, "rotCos");
    v(this, "rotSin");
    v(this, "zoom");
    v(this, "shift");
    v(this, "destroyed", !1);
    this.options = e, this.canvas = e.canvas, this.variation = e.variation, this.grain = e.grain, this.animateGrain = e.animateGrain, this.prominence = e.prominence, this.heroMotif = e.heroMotif, this.motifOffsetX = e.heroMotif.posX * 75e-4, this.motifOffsetY = e.heroMotif.posY * 75e-4, this.evapIdleStrength = Math.max(0, e.heroMotif.idleEvap / 58);
    const o = this.canvas.getContext("2d", { alpha: !1 }) ?? this.canvas.getContext("2d");
    if (!o)
      throw new Error("Canvas 2D context is not available.");
    this.ctx2d = o;
    const [a, s] = g.anchor, n = e.variation.curve[0] - e.variation.center[0], i = e.variation.curve[1] - e.variation.center[1], r = Math.atan2(
      s[1] - a[1],
      s[0] - a[0]
    );
    this.rotation = Math.atan2(i, n) - r, this.rotCos = Math.cos(this.rotation), this.rotSin = Math.sin(this.rotation);
    const l = Math.hypot(
      s[0] - a[0],
      s[1] - a[1]
    );
    this.zoom = 1 + (e.variation.spread - 80) * 6e-3 + (Math.hypot(n, i) - l) * 0.22, this.shift = [
      (e.variation.center[0] - a[0]) * 0.8,
      (e.variation.center[1] - a[1]) * 0.8
    ], this.mesh1Cpu = ge(
      e.mesh1Settings,
      e.mesh1Glyph,
      e.mesh1Atlas,
      e.variation.seed,
      e.heroMotif,
      e.motifSdf
    ), this.mesh2Cpu = ge(
      e.mesh2Settings,
      e.mesh2Glyph,
      e.mesh2Atlas,
      e.variation.seed,
      null,
      null
    ), this.shared = mt(this), this.buildInstanceGpuResources();
  }
  get width() {
    return this.canvas.width;
  }
  get height() {
    return this.canvas.height;
  }
  buildMeshGpu(e) {
    if (!e) return null;
    const o = this.shared.gl, a = this.shared.getAtlasTexture(e.atlasEntry), s = ve(o, e.backData), n = ve(o, e.frontData);
    return {
      cpu: e,
      texture: a,
      backVao: s.vao,
      backBuffer: s.buffer,
      backCount: e.backCount,
      frontVao: n.vao,
      frontBuffer: n.buffer,
      frontCount: e.frontCount
    };
  }
  buildInstanceGpuResources() {
    this.heroMotif.enabled ? (this.shared.ensureMotifProgram(), this.motifTexture = this.heroMotif.motifType === "symbol" && this.options.motifSdf ? this.shared.getMotifTexture(this.options.motifSdf) : null) : this.motifTexture = null, this.mesh1 = this.buildMeshGpu(this.mesh1Cpu), this.mesh2 = this.buildMeshGpu(this.mesh2Cpu);
  }
  releaseInstanceGpuResources() {
    const e = this.shared.gl;
    if (!this.shared.contextLost && !e.isContextLost())
      for (const o of [this.mesh1, this.mesh2])
        o && (e.deleteBuffer(o.backBuffer), e.deleteBuffer(o.frontBuffer), e.deleteVertexArray(o.backVao), e.deleteVertexArray(o.frontVao));
    this.mesh1 = null, this.mesh2 = null, this.motifTexture = null;
  }
  handleContextRestored() {
    var e, o;
    this.destroyed || (this.buildInstanceGpuResources(), (o = (e = this.options).onContextRestored) == null || o.call(e));
  }
  render(e, o, a) {
    if (this.destroyed) return;
    const s = this.shared, n = s.gl;
    if (s.contextLost || n.isContextLost()) return;
    const i = this.canvas.width, r = this.canvas.height;
    if (i <= 0 || r <= 0) return;
    s.ensureSurfaceSize(i, r), s.syncPresetUniforms(e);
    const l = Math.max(1, g.duration), h = (o % l + l) % l / l, p = oe.scale(i, r), f = [
      a.pointer[0] * p[0],
      a.pointer[1] * p[1]
    ], m = oe.toScene(a.tap, i, r), d = oe.toScene(a.hover, i, r);
    let y = 0, M = 0, E = 0, A = 0;
    if (this.animateGrain && this.grain > 0) {
      const R = Math.floor(o * g.grainFps);
      y = R * 197 & 511, M = R * 353 & 511, E = R * 269 + 71 & 511, A = R * 131 + 193 & 511;
    }
    n.viewport(0, 0, i, r);
    const S = s.gradUniforms;
    n.useProgram(s.gradientProgram), n.bindVertexArray(s.quadVao), n.disable(n.BLEND), n.activeTexture(n.TEXTURE0), n.bindTexture(n.TEXTURE_2D, s.noiseTexture), n.uniform2f(S.u_shift, this.shift[0], this.shift[1]), n.uniform1f(S.u_zoom, this.zoom), n.uniform2f(S.u_rotCosSin, this.rotCos, this.rotSin), n.uniform1f(S.u_beamIntensity, this.variation.beamIntensity), n.uniform1f(S.u_grain, this.grain), n.uniform2f(S.u_resolution, i, r), n.uniform2f(S.u_viewScale, p[0], p[1]), n.uniform2f(S.u_pointer, f[0], f[1]), n.uniform2f(S.u_tap, m[0], m[1]), n.uniform2f(S.u_focusPoint, d[0], d[1]), n.uniform1f(S.u_focusAmount, a.hoverAmount), n.uniform4f(
      S.u_grainOffset,
      y,
      M,
      E,
      A
    ), n.uniform1f(S.u_phase, h), n.uniform1f(S.u_tapAge, a.tapAge), n.drawArrays(n.TRIANGLES, 0, 3);
    const L = this.heroMotif.enabled && s.motifProgram !== null;
    if (this.mesh1 || this.mesh2 || L) {
      const R = e.colorMode === "light";
      n.enable(n.BLEND), n.blendFunc(n.SRC_ALPHA, n.ONE_MINUS_SRC_ALPHA);
      let T = null;
      const u = (c, _) => {
        if (!c) return;
        const I = _ === "back" ? c.backCount : c.frontCount;
        if (I === 0) return;
        const C = R ? c.cpu.settings.opacityLight : c.cpu.settings.opacityDark, b = Math.max(0, Math.min(100, C)) / 100;
        b <= 1e-3 || (T !== c && (T = c, this.uploadMeshUniforms(c, R, b, h)), n.bindVertexArray(_ === "back" ? c.backVao : c.frontVao), n.drawArraysInstanced(n.TRIANGLE_STRIP, 0, 4, I));
      };
      if (this.mesh1 || this.mesh2) {
        const c = s.partUniforms, _ = Math.min(i, r), I = i / _, C = r / _, b = Math.max(0, Math.min(100, g.hoverReactivity)) / 65, P = Math.max(0, Math.min(1, a.hoverAmount)), F = P * b, w = Math.max(0, Math.min(100, g.repulsion)) * 0.011 * F, D = P * Math.max(0, this.heroMotif.hoverEvap / 54), B = 0.5 + 0.5 * b, z = a.tapAge < 4, X = z ? a.tapAge * 2.2 > 12 ? 0 : Math.exp(-a.tapAge * 2.2) : 0;
        n.useProgram(s.particleProgram), n.uniform2f(c.u_rotCosSin, this.rotCos, this.rotSin), n.uniform1f(c.u_zoom, this.zoom), n.uniform1f(c.u_evapIdleStrength, this.evapIdleStrength), n.uniform2f(c.u_resolution, i, r), n.uniform2f(c.u_invViewScale, 1 / p[0], 1 / p[1]), n.uniform2f(c.u_metric, I, C), n.uniform2f(c.u_invMetric, 1 / I, 1 / C), n.uniform1f(c.u_phase, h), n.uniform2f(c.u_focusScene, d[0], d[1]), n.uniform1f(c.u_effectiveHoverAmt, F), n.uniform1f(c.u_rawHoverAmt, P), n.uniform1f(c.u_hasHover, P > 5e-3 ? 1 : 0), n.uniform2f(c.u_hover, a.hover[0], a.hover[1]), n.uniform1f(c.u_force, w), n.uniform1f(c.u_evapHoverStrength, D), n.uniform2f(
          c.u_ptr,
          f[0] * B,
          f[1] * B
        ), n.uniform1f(c.u_hasTap, z ? 1 : 0), n.uniform2f(c.u_tap, m[0], m[1]), n.uniform1f(c.u_tapAge, a.tapAge), n.uniform1f(c.u_tapDecay, X), n.uniform1f(c.u_radiusFactor, 1.6 * r / 2160), u(this.mesh1, "back"), u(this.mesh2, "back");
      }
      if (L && s.motifProgram) {
        const c = Math.max(0.4, Math.min(2, this.heroMotif.size / 100)), _ = 0.5 + this.motifOffsetX - f[0] * 0.028, I = 0.5 + this.motifOffsetY - f[1] * 0.028, C = 0.46 * c + 0.04, b = 0.78 * c + 0.04, P = (_ - C - 0.5) / p[0] + 0.5, F = (_ + C - 0.5) / p[0] + 0.5, w = 0.5 - (I + b - 0.5) / p[1], D = 0.5 - (I - b - 0.5) / p[1], B = Math.max(0, Math.floor(P * i)), z = Math.max(0, Math.floor(w * r)), X = Math.min(i, Math.ceil(F * i)) - B, q = Math.min(r, Math.ceil(D * r)) - z;
        if (X > 0 && q > 0) {
          const G = s.motifUniforms;
          n.useProgram(s.motifProgram), n.bindVertexArray(s.quadVao), this.motifTexture && (n.activeTexture(n.TEXTURE1), n.bindTexture(n.TEXTURE_2D, this.motifTexture)), n.uniform1f(G.u_motifMode, this.motifTexture ? 1 : 0), n.uniform1f(G.u_fourSize, this.heroMotif.size), n.uniform2f(G.u_fourOffset, this.motifOffsetX, this.motifOffsetY), n.uniform1f(G.u_fourOpacity, this.heroMotif.opacity), n.uniform1f(G.u_idleEvap, this.heroMotif.idleEvap), n.uniform1f(G.u_fourEvap, this.heroMotif.hoverEvap), n.uniform2f(G.u_viewScale, p[0], p[1]), n.uniform2f(G.u_pointer, f[0], f[1]), n.uniform2f(G.u_tap, m[0], m[1]), n.uniform2f(G.u_focusPoint, d[0], d[1]), n.uniform1f(G.u_focusAmount, a.hoverAmount), n.uniform1f(G.u_phase, h), n.uniform1f(G.u_tapAge, a.tapAge), n.enable(n.SCISSOR_TEST), n.scissor(B, z, X, q), n.drawArrays(n.TRIANGLES, 0, 3), n.disable(n.SCISSOR_TEST), (this.mesh1 || this.mesh2) && (n.useProgram(s.particleProgram), T = null);
        }
      }
      (this.mesh1 || this.mesh2) && (u(this.mesh1, "front"), u(this.mesh2, "front")), n.disable(n.BLEND);
    }
    n.bindVertexArray(null), this.ctx2d.drawImage(
      s.surface,
      0,
      s.surface.height - r,
      i,
      r,
      0,
      0,
      i,
      r
    );
  }
  uploadMeshUniforms(e, o, a, s) {
    const n = this.shared.gl, i = this.shared.partUniforms, { settings: r, glyphCfg: l, atlasEntry: h } = e.cpu, p = (s + r.phaseOffset) % 1, f = p * Math.PI * 2, m = 0.25 + g.energy / 36 * 0.75, d = r.meshDepth / 60, y = (o ? 0.62 : 0.54) + this.prominence * 68e-4, M = Math.max(15, Math.min(300, r.scale)) / 100, E = Math.max(0, Math.min(100, r.sizeFalloff)) / 100, A = a <= 0.75 ? a / 0.75 : 1 + (a - 0.75) / 0.25 * 0.9, S = ((o ? 1.28 : 0.82) + this.prominence * (o ? 9e-3 : 68e-4)) * A, L = 1 - 0.45 * Math.max(0, (a - 0.65) / 0.35), R = (o ? 0.72 : 1.28) * L, T = 0.5 + this.shift[0] * 0.55 - Math.sin(f) * g.energy * 22e-5, u = 0.5 + this.shift[1] * 0.55 - (Math.cos(f) - 1) * g.energy * 22e-5, c = Math.sin(f) * g.energy * 1e-4, _ = (Math.cos(f) - 1) * g.energy * 1e-4;
    n.activeTexture(n.TEXTURE0), n.bindTexture(n.TEXTURE_2D, e.texture), n.uniform1f(i.u_a, f), n.uniform1f(i.u_phaseSpeed, p * m), n.uniform1f(i.u_waveAmp, d), n.uniform1f(i.u_opacityGain, A), n.uniform1f(i.u_promAlpha, S), n.uniform1f(i.u_blurAtten, R), n.uniform1f(i.u_uniformBaseSize, 2.15 * y * M), n.uniform1f(i.u_falloffT, E), n.uniform2f(
      i.u_sparkOffset,
      r.posX * 75e-4,
      r.posY * 75e-4
    ), n.uniform2f(i.u_shiftOff, T, u), n.uniform2f(i.u_evapFlowOffset, c, _), n.uniform1f(i.u_spinScale, l.shapeType === "text" ? 0.18 : 1), n.uniform1f(i.u_glyphRows, h.atlasRows);
  }
  destroy() {
    this.destroyed || (this.destroyed = !0, this.releaseInstanceGpuResources(), ht(this));
  }
}
const pt = 1.5, vt = 2e6, gt = 12e5, _e = 30;
function _t(t, e, o) {
  const a = Math.min(window.devicePixelRatio || 1, pt), s = o ? gt : vt, n = t * e * a * a, i = n > s ? Math.sqrt(s / n) : 1, r = a * i;
  return [
    Math.max(1, Math.round(t * r)),
    Math.max(1, Math.round(e * r))
  ];
}
const bt = Fe({
  styles: [We],
  config: ze,
  async mount({ root: t, config: e, colorMode: o, signal: a }) {
    const s = Oe(e.show_hero_motif, e.hero_motif_symbol, {
      size: e.motif_scale,
      opacity: e.motif_opacity,
      posX: e.motif_pos_x,
      posY: e.motif_pos_y,
      idleEvap: e.motif_idle_evap,
      hoverEvap: e.motif_hover_evap
    }), n = fe(e.mesh_1_text), i = fe(e.mesh_2_text), r = {
      ...g.mesh1,
      meshDepth: e.mesh_1_depth,
      posX: e.mesh_1_pos_x,
      posY: e.mesh_1_pos_y
    }, l = {
      ...g.mesh2,
      meshDepth: e.mesh_2_depth,
      posX: e.mesh_2_pos_x,
      posY: e.mesh_2_pos_y
    }, [h, p, f] = await Promise.all([
      Je(s),
      n.enabled ? ue(n) : Promise.resolve(null),
      i.enabled ? ue(i) : Promise.resolve(null)
    ]);
    a.throwIfAborted();
    const m = Le(e.seed), d = document.createElement("canvas");
    d.setAttribute("aria-hidden", "true"), t.append(d);
    let y = (w, D) => {
    }, M;
    try {
      M = new dt({
        canvas: d,
        variation: m,
        grain: e.grain,
        animateGrain: e.animate_grain,
        prominence: e.spark_prominence,
        heroMotif: s,
        motifSdf: h,
        mesh1Settings: r,
        mesh1Glyph: n,
        mesh1Atlas: p,
        mesh2Settings: l,
        mesh2Glyph: i,
        mesh2Atlas: f,
        onContextRestored: () => y(0, performance.now())
      });
    } catch (w) {
      return d.width = 0, d.height = 0, xt(t, w);
    }
    let E = o, A = ce(
      E,
      e.dark_mode_preset,
      e.light_mode_preset
    );
    const S = (w) => {
      E = w, A = ce(
        E,
        e.dark_mode_preset,
        e.light_mode_preset
      ), d.style.background = A.background;
    };
    S(o);
    const L = typeof window.matchMedia == "function" && window.matchMedia("(pointer: coarse)").matches, R = new Qe(d, t.parentElement || t);
    let T = 0, u = 0, c = 0, _ = !1, I = !1;
    y = (w, D) => {
      const B = R.sample(
        w,
        D,
        g.parallax,
        g.ripple
      );
      M.render(A, T, B);
    };
    const C = () => {
      const w = Math.max(1, d.clientWidth), D = Math.max(1, d.clientHeight), [B, z] = _t(w, D, L), X = d.width !== B || d.height !== z;
      X && (d.width = B, d.height = z), (X || !I) && (I = !0, y(0, performance.now()));
    }, b = new ResizeObserver(C);
    b.observe(d), C();
    const P = (w) => {
      const D = u ? w - u : _e;
      if (!R.isActive(w) && D < _e) {
        c = requestAnimationFrame(P);
        return;
      }
      const B = u ? Math.min(Math.max(D, 0), 100) / 1e3 : 0;
      u = w, T = (T + B) % g.duration, y(B, w), c = requestAnimationFrame(P);
    }, F = () => {
      _ && !c ? (u = performance.now(), c = requestAnimationFrame(P)) : !_ && c && (cancelAnimationFrame(c), c = 0, u = 0);
    };
    return {
      resume() {
        _ = !0, F();
      },
      pause() {
        _ = !1, R.reset(), F();
      },
      update(w) {
        w.colorMode && (S(w.colorMode), c || y(0, performance.now()));
      },
      destroy() {
        _ = !1, F(), b.disconnect(), R.destroy(), d.width = 0, d.height = 0, M.destroy();
      }
    };
  }
});
function xt(t, e) {
  console.warn("[gemini-4-argon-gradient]", e);
  const o = document.createElement("pre");
  return o.className = "error", o.textContent = `gemini-4-argon-gradient: ${e instanceof Error ? e.message : e}`, t.replaceChildren(o), { resume() {
  }, pause() {
  }, destroy() {
  } };
}
window.AdSpaceStage = bt;

})();
