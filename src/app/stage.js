// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// AdSpace Agent — Interactive Canvas Dotted Grid Stage (WebGL2)
// Argon color palette with smooth cursor repulsion, size bloom, glow, tap
// ripples, and a slow accent-blue wave that rolls across the grid while the
// agent is streaming (`update({ streaming })`), settling back to the resting
// grid when the run completes.

const PRESETS = {
  "argon-dark": {
    id: "argon-dark",
    colorMode: "dark",
    background: "#121317",
    bgTopLeft: [0.082, 0.086, 0.102],
    bgTopRight: [0.098, 0.102, 0.122],
    bgBottomLeft: [0.067, 0.071, 0.086],
    bgBottomRight: [0.075, 0.078, 0.094],
    dotRestA: [120 / 255, 128 / 255, 142 / 255],
    dotRestB: [156 / 255, 166 / 255, 182 / 255],
    dotHover: [218 / 255, 224 / 255, 235 / 255],
    dotCore: [248 / 255, 249 / 255, 252 / 255],
    restAlpha: 0.22,
    hoverAlpha: 0.88,
    // Wave accents per activity. `tint` colours the field and the crest,
    // `hover` / `core` replace the grey hover palette while the wave is
    // up so the cursor bloom stays in the same hue family.
    accents: {
      // Google blue 300 / 400 / 100.
      streaming: {
        tint: [138 / 255, 180 / 255, 248 / 255],
        hover: [102 / 255, 157 / 255, 246 / 255],
        core: [211 / 255, 227 / 255, 253 / 255],
        bgGain: 0.1,
      },
      // Google yellow 300 / 400 / 100.
      authorizing: {
        tint: [253 / 255, 214 / 255, 99 / 255],
        hover: [252 / 255, 201 / 255, 52 / 255],
        core: [254 / 255, 239 / 255, 195 / 255],
        bgGain: 0.1,
      },
    },
  },
  "argon-light": {
    id: "argon-light",
    colorMode: "light",
    background: "#f8f9fc",
    bgTopLeft: [0.985, 0.988, 0.996],
    bgTopRight: [0.960, 0.965, 0.978],
    bgBottomLeft: [0.965, 0.969, 0.980],
    bgBottomRight: [0.953, 0.959, 0.973],
    dotRestA: [140 / 255, 148 / 255, 162 / 255],
    dotRestB: [110 / 255, 118 / 255, 132 / 255],
    dotHover: [69 / 255, 71 / 255, 77 / 255],
    dotCore: [18 / 255, 19 / 255, 23 / 255],
    restAlpha: 0.2,
    hoverAlpha: 0.85,
    accents: {
      // --accent-primary (#1a73e8), then Google blue 700 / 800.
      streaming: {
        tint: [26 / 255, 115 / 255, 232 / 255],
        hover: [11 / 255, 87 / 255, 208 / 255],
        core: [8 / 255, 66 / 255, 160 / 255],
        bgGain: 0.06,
      },
      // Google yellow 600 / 700 / 800.
      authorizing: {
        tint: [249 / 255, 171 / 255, 0 / 255],
        hover: [227 / 255, 116 / 255, 0 / 255],
        core: [176 / 255, 96 / 255, 0 / 255],
        bgGain: 0.07,
      },
    },
  },
};

/** Activities that put the wave up, in the order `update()` accepts them. */
const WAVE_ACTIVITIES = ["streaming", "authorizing"];

const DEFAULTS = {
  gridStep: 24,
  dotRadius: 1.4,
  hoverReach: 155,
  hoverPush: 7.5,
  hoverSwirl: 2.2,
  hoverScale: 1.35,
  loopSeconds: 14,
  // Streaming wave: the grid undulates with a primary swell of
  // `streamWavelength` CSS px that travels one wavelength every
  // `streamWaveSeconds`, displacing dots by up to `streamAmplitude` px;
  // the effect fades in / out over `streamFadeIn` / `streamFadeOut` s.
  streamWaveSeconds: 5.5,
  streamWavelength: 440,
  streamAmplitude: 6.5,
  streamFadeIn: 0.9,
  streamFadeOut: 1.6,
};

function selectPreset(colorMode, darkId, lightId) {
  if (colorMode === "dark") {
    return PRESETS[darkId] || PRESETS["argon-dark"];
  }
  return PRESETS[lightId] || PRESETS["argon-light"];
}

class PointerTracker {
  constructor(canvas, listenEl) {
    this.canvas = canvas;
    this.pointer = [0, 0];
    this.target = [0, 0];
    this.hover = [0, 0];
    this.inside = false;
    this.amount = 0;
    this.tap = [0, 0];
    this.tapTime = -10000;
    // Canvas rect in viewport coords, cached so pointermove does not force
    // layout; the host refreshes it from syncSize() and it is dropped on
    // scroll. `null` means "recompute on next use".
    this.rect = null;
    this.controller = new AbortController();
    this.sampled = {
      pointer: [0, 0],
      hover: [0, 0],
      hoverAmount: 0,
      tap: [0, 0],
      tapAge: 10,
    };

    const { signal } = this.controller;
    const updateTarget = (ev) => {
      if (ev.pointerType === "touch") return;
      const rect = this.getRect();
      if (!rect.width || !rect.height) return;
      const x = ev.clientX - rect.left;
      const y = ev.clientY - rect.top;
      this.target[0] = x;
      this.target[1] = y;
      if (!this.inside && this.amount < 0.01) {
        this.hover[0] = x;
        this.hover[1] = y;
      }
      this.inside = true;
    };
    const onLeave = () => {
      this.inside = false;
    };
    const onDown = (ev) => {
      const rect = this.getRect();
      if (!rect.width || !rect.height) return;
      this.tap[0] = ev.clientX - rect.left;
      this.tap[1] = ev.clientY - rect.top;
      this.tapTime = performance.now();
    };
    const onBlur = () => {
      this.reset();
    };
    const onScroll = () => {
      this.rect = null;
    };

    listenEl.addEventListener("pointermove", updateTarget, { signal, passive: true });
    listenEl.addEventListener("pointerenter", updateTarget, { signal, passive: true });
    listenEl.addEventListener("pointerleave", onLeave, { signal, passive: true });
    listenEl.addEventListener("pointercancel", onLeave, { signal, passive: true });
    listenEl.addEventListener("pointerdown", onDown, { signal, passive: true });
    window.addEventListener("blur", onBlur, { signal });
    // Capture phase so scrolls of any ancestor (not just the document)
    // invalidate the cached rect.
    window.addEventListener("scroll", onScroll, { signal, passive: true, capture: true });
  }

  /** Cached canvas bounding rect; recomputed only after syncRect(null) / scroll. */
  getRect() {
    if (!this.rect) this.rect = this.canvas.getBoundingClientRect();
    return this.rect;
  }

  /** Refresh (or, with `null`, drop) the cached canvas rect. */
  syncRect(rect = this.canvas.getBoundingClientRect()) {
    this.rect = rect;
  }

  isActive(now) {
    return (
      this.inside ||
      this.amount > 0.001 ||
      Math.abs(this.pointer[0]) > 0.001 ||
      Math.abs(this.pointer[1]) > 0.001 ||
      now - this.tapTime < 3200
    );
  }

  reset() {
    this.inside = false;
    this.amount = 0;
    this.pointer[0] = 0;
    this.pointer[1] = 0;
    this.tapTime = -10000;
    this.sampled.hoverAmount = 0;
    this.sampled.tapAge = 10;
  }

  sample(dt, now, widthCss, heightCss) {
    const clampedDt = Math.max(0, dt);
    const posEase = 1 - Math.exp(-clampedDt * 7.5);
    const amtEase = 1 - Math.exp(-clampedDt * (this.inside ? 6.0 : 2.4));

    this.hover[0] += (this.target[0] - this.hover[0]) * posEase;
    this.hover[1] += (this.target[1] - this.hover[1]) * posEase;
    this.amount += ((this.inside ? 1 : 0) - this.amount) * amtEase;
    if (this.amount < 0.0001) this.amount = 0;

    const normX = this.inside && widthCss > 0 ? this.target[0] / widthCss - 0.5 : 0;
    const normY = this.inside && heightCss > 0 ? this.target[1] / heightCss - 0.5 : 0;
    this.pointer[0] += (normX - this.pointer[0]) * posEase;
    this.pointer[1] += (normY - this.pointer[1]) * posEase;

    this.sampled.pointer[0] = this.pointer[0];
    this.sampled.pointer[1] = this.pointer[1];
    this.sampled.hover[0] = this.hover[0];
    this.sampled.hover[1] = this.hover[1];
    this.sampled.hoverAmount = this.amount;
    this.sampled.tap[0] = this.tap[0];
    this.sampled.tap[1] = this.tap[1];
    this.sampled.tapAge = (now - this.tapTime) / 1000;
    return this.sampled;
  }

  destroy() {
    this.controller.abort();
  }
}

const BG_VERT = `#version 300 es
out vec2 v_uv;
void main() {
  vec2 v = vec2((gl_VertexID << 1) & 2, gl_VertexID & 2);
  v_uv = v;
  gl_Position = vec4(v * 2.0 - 1.0, 0.0, 1.0);
}
`;

const BG_FRAG = `#version 300 es
precision highp float;

in vec2 v_uv;
out vec4 fragColor;

uniform vec2 u_pointer;
uniform vec2 u_hoverUv;
uniform float u_hoverAmount;
uniform vec3 u_bgTopLeft;
uniform vec3 u_bgTopRight;
uniform vec3 u_bgBottomLeft;
uniform vec3 u_bgBottomRight;
uniform vec3 u_glowColor;
uniform float u_isLight;
uniform float u_stream;
uniform float u_streamTime;
uniform vec3 u_streamColor;
uniform float u_streamBgGain;
uniform vec2 u_cssSize;
uniform float u_streamWavelength;

void main() {
  vec2 uv = vec2(v_uv.x, 1.0 - v_uv.y);
  vec2 p = clamp(uv + u_pointer * 0.025, 0.0, 1.0);

  vec3 topRow = mix(u_bgTopLeft, u_bgTopRight, p.x);
  vec3 botRow = mix(u_bgBottomLeft, u_bgBottomRight, p.x);
  vec3 col = mix(topRow, botRow, p.y);

  // Subtle ambient cursor halo on the background surface
  if (u_hoverAmount > 0.001) {
    vec2 d = uv - u_hoverUv;
    float glow = exp(-dot(d, d) / 0.085) * u_hoverAmount;
    float glowGain = u_isLight > 0.5 ? 0.055 : 0.085;
    col = mix(col, u_glowColor, glow * glowGain);
  }

  // Streaming: a soft swell of the accent hue riding the same primary wave
  // as the dots, so the surface appears to lift with the grid.
  if (u_stream > 0.001) {
    vec2 dir = normalize(vec2(0.8, 0.6));
    float along = dot(uv * u_cssSize, dir);
    float swell = 0.5 + 0.5 * sin((along / u_streamWavelength) * 6.2831853 - u_streamTime * 6.2831853);
    swell = swell * swell;
    col = mix(col, u_streamColor, u_stream * u_streamBgGain * (0.35 + 0.65 * swell));
  }

  // Fine interleaved gradient noise dither to prevent 8-bit banding
  float ign = fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(0.06711056, 0.00583715))));
  col += (ign - 0.5) * (1.2 / 255.0);

  fragColor = vec4(clamp(col, 0.0, 1.0), 1.0);
}
`;

const DOT_VERT = `#version 300 es
precision highp float;

out vec2 v_localCoord;
out vec3 v_color;
out float v_alpha;
out float v_hoverMix;

uniform vec2 u_cssSize;
uniform int u_cols;
uniform vec2 u_gridOrigin;
uniform float u_gridStep;
uniform float u_dotRadius;
uniform vec2 u_pointer;
uniform vec2 u_hoverPos;
uniform float u_hoverAmount;
uniform float u_hoverReach;
uniform float u_hoverPush;
uniform float u_hoverSwirl;
uniform float u_hoverScale;
uniform vec2 u_tapPos;
uniform float u_tapAge;
uniform float u_phase;
uniform vec3 u_dotRestA;
uniform vec3 u_dotRestB;
uniform vec3 u_dotHover;
uniform vec3 u_dotCore;
uniform float u_restAlpha;
uniform float u_hoverAlpha;
uniform float u_stream;
uniform float u_streamTime;
uniform vec3 u_streamColor;
uniform vec3 u_streamHover;
uniform vec3 u_streamCore;
uniform float u_streamWavelength;
uniform float u_streamAmplitude;

float smooth01(float a, float b, float x) {
  float t = clamp((x - a) / (b - a), 0.0, 1.0);
  return t * t * (3.0 - 2.0 * t);
}

void main() {
  int gridCol = gl_InstanceID % u_cols;
  int gridRow = gl_InstanceID / u_cols;

  vec2 gridIndex = vec2(float(gridCol), float(gridRow));
  vec2 basePos = u_gridOrigin + gridIndex * u_gridStep;
  vec2 normUv = clamp(basePos / max(u_cssSize, vec2(1.0)), 0.0, 1.0);

  // Subtle ambient wave across the grid
  float waveAngle = float(gridCol) * 0.22 + float(gridRow) * 0.16 - u_phase * 6.2831853;
  float ambientWave = 0.5 + 0.5 * sin(waveAngle);

  // Streaming wave: the grid itself undulates. Two travelling waves run
  // diagonally from the top-left to the bottom-right; each dot orbits a small
  // ellipse as a crest passes (like a water particle), so rows and columns
  // visibly bunch and stretch instead of just changing colour. The accent hue
  // follows the crest as a secondary cue. streamCrest is 0 in the trough, 1
  // on the ridge; the whole effect is scaled by u_stream (0 -> 1 fade).
  float streamCrest = 0.0;
  vec2 streamShift = vec2(0.0);
  if (u_stream > 0.001) {
    vec2 dir = normalize(vec2(0.8, 0.6));
    vec2 perp = vec2(-dir.y, dir.x);
    float along = dot(basePos, dir);
    float across = dot(basePos, perp);
    // Primary swell: wavelength u_streamWavelength px, one wavelength per
    // period. Secondary chop: shorter, faster, slightly skewed.
    float p1 = (along / u_streamWavelength) * 6.2831853 - u_streamTime * 6.2831853;
    float p2 = (along / (u_streamWavelength * 0.5) + across / (u_streamWavelength * 1.7)) * 6.2831853
             - u_streamTime * 6.2831853 * 1.6;
    float w1 = sin(p1);
    float w2 = sin(p2);
    // Trochoidal orbit: lift across the wave on the crest, drift along the
    // travel direction on the way up (cos), giving a rolling motion.
    streamShift = (perp * w1 + dir * cos(p1) * 0.55) * u_streamAmplitude
                + perp * w2 * u_streamAmplitude * 0.2;
    streamShift *= u_stream;
    float c1 = 0.5 + 0.5 * w1;
    streamCrest = (c1 * c1 * 0.8 + (0.5 + 0.5 * w2) * 0.2) * u_stream;
  }

  // Subtle pointer parallax, plus the streaming displacement.
  vec2 pos = basePos - u_pointer * 4.0 + streamShift;

  float hoverBell = 0.0;
  float hoverCore = 0.0;
  if (u_hoverAmount > 0.001) {
    vec2 toDot = pos - u_hoverPos;
    float dist = length(toDot);
    if (dist < u_hoverReach) {
      float falloff = 1.0 - smooth01(0.0, u_hoverReach, dist);
      hoverBell = falloff * falloff * u_hoverAmount;
      hoverCore = smooth01(u_hoverReach * 0.45, 0.0, dist) * u_hoverAmount;

      vec2 dir = dist > 0.001 ? toDot / dist : vec2(1.0, 0.0);
      vec2 perp = vec2(-dir.y, dir.x);
      float rippleHarmonic = sin(dist * 0.075 - u_phase * 6.2831853 * 2.0) * 1.35 * hoverBell;
      float push = u_hoverPush * hoverBell + rippleHarmonic;
      float swirl = u_hoverSwirl * hoverBell;
      pos += dir * push + perp * swirl;
    }
  }

  // Click / tap radial ripple wave
  float tapBoost = 0.0;
  if (u_tapAge >= 0.0 && u_tapAge < 2.8) {
    vec2 fromTap = pos - u_tapPos;
    float tapDist = length(fromTap);
    float waveRadius = u_tapAge * 290.0;
    float ringWidth = 34.0 + u_tapAge * 26.0;
    float ringDelta = (tapDist - waveRadius) / ringWidth;
    float ring = exp(-ringDelta * ringDelta);
    float decay = exp(-u_tapAge * 1.45) * smooth01(0.0, 0.04, u_tapAge);
    float wave = sin((tapDist - waveRadius) * 0.09) * ring * decay;
    vec2 tapDir = tapDist > 0.001 ? fromTap / tapDist : vec2(0.0, 1.0);
    pos += tapDir * wave * 5.5;
    tapBoost = ring * decay * 0.75;
  }

  // Color & alpha blending using Argon palette
  float paletteMix = clamp(normUv.x * 0.65 + (1.0 - normUv.y) * 0.35, 0.0, 1.0);
  vec3 restCol = mix(u_dotRestA, u_dotRestB, paletteMix);
  // Streaming tint: a soft base hue everywhere, deepening on the crest. The
  // cursor bloom follows the accent too (deeper shade for the hover ring,
  // the accent's core shade at the centre) instead of the grey rest palette.
  restCol = mix(restCol, u_streamColor, u_stream * 0.25 + streamCrest * 0.55);
  vec3 hoverCol = mix(u_dotHover, u_streamHover, u_stream);
  vec3 coreCol = mix(u_dotCore, u_streamCore, u_stream);
  float activeMix = clamp(hoverBell + tapBoost * 0.7, 0.0, 1.0);
  vec3 dotColor = mix(restCol, hoverCol, activeMix);
  dotColor = mix(dotColor, coreCol, clamp(hoverCore * 0.85 + tapBoost * 0.35, 0.0, 1.0));

  float alpha = mix(
    u_restAlpha * (0.9 + 0.14 * ambientWave) + streamCrest * 0.42 + u_stream * 0.06,
    u_hoverAlpha,
    activeMix
  );

  // Dot radius with hover bloom, tap pulse and streaming crest swell, plus
  // extra quad padding for soft glow & AA
  float radiusCss = u_dotRadius * (1.0 + hoverBell * u_hoverScale + tapBoost * 0.65 + streamCrest * 0.45);
  float quadHalfCss = radiusCss + 2.2 + hoverBell * 2.4 + streamCrest * 1.6;

  // 4-vertex triangle strip quad
  vec2 corner = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1)) * 2.0 - 1.0;
  vec2 vertCss = pos + corner * quadHalfCss;

  // Convert CSS pixel coords to WebGL clip space
  vec2 clip = (vertCss / max(u_cssSize, vec2(1.0))) * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);

  v_localCoord = corner * quadHalfCss;
  // Normalise by radius so the fragment shader sees the dot edge at dist == 1
  v_localCoord /= max(radiusCss, 0.1);
  v_color = dotColor;
  v_alpha = clamp(alpha, 0.0, 1.0);
  v_hoverMix = clamp(activeMix + streamCrest * 0.6, 0.0, 1.0);
}
`;

const DOT_FRAG = `#version 300 es
precision highp float;

in vec2 v_localCoord;
in vec3 v_color;
in float v_alpha;
in float v_hoverMix;

out vec4 fragColor;

void main() {
  float dist = length(v_localCoord);

  // Crisp anti-aliased circular core
  float aa = max(fwidth(dist), 0.08);
  float core = 1.0 - smoothstep(1.0 - aa, 1.0 + aa, dist);

  // Soft outer halo on hovered / rippled dots
  float halo = exp(-dist * dist * 0.85) * 0.38 * v_hoverMix;
  float shape = clamp(core + halo, 0.0, 1.0);

  if (shape < 0.004) {
    discard;
  }

  fragColor = vec4(v_color, shape * v_alpha);
}
`;

function createProgram(gl, vertSrc, fragSrc) {
  const prog = gl.createProgram();
  if (!prog) throw new Error("Failed to create WebGLProgram.");

  const compile = (type, src) => {
    const sh = gl.createShader(type);
    if (!sh) throw new Error("Failed to create WebGLShader.");
    gl.shaderSource(sh, src);
    gl.compileShader(sh);
    if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
      const info = gl.getShaderInfoLog(sh) || "Shader compile failed";
      gl.deleteShader(sh);
      throw new Error(info);
    }
    return sh;
  };

  const vs = compile(gl.VERTEX_SHADER, vertSrc);
  const fs = compile(gl.FRAGMENT_SHADER, fragSrc);
  gl.attachShader(prog, vs);
  gl.attachShader(prog, fs);
  gl.linkProgram(prog);
  gl.deleteShader(vs);
  gl.deleteShader(fs);

  if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
    const info = gl.getProgramInfoLog(prog) || "Program link failed";
    gl.deleteProgram(prog);
    throw new Error(info);
  }
  return prog;
}

function getUniforms(gl, prog, names) {
  const map = {};
  for (const name of names) {
    map[name] = gl.getUniformLocation(prog, name);
  }
  return map;
}

export const AdSpaceStage = {
  async mount({ root, config = {}, colorMode = "light", signal }) {
    if (signal?.aborted) {
      throw new DOMException("Aborted", "AbortError");
    }

    const canvas = document.createElement("canvas");
    canvas.setAttribute("aria-hidden", "true");
    canvas.style.cssText = "display:block;width:100%;height:100%;";
    root.replaceChildren(canvas);

    const gl = canvas.getContext("webgl2", {
      alpha: false,
      // Dots are analytically anti-aliased in DOT_FRAG (fwidth) and the
      // background is a single full-screen triangle, so MSAA buys nothing.
      antialias: false,
      depth: false,
      stencil: false,
      powerPreference: "low-power",
    });
    if (!gl) {
      canvas.remove();
      throw new Error("WebGL2 is not available.");
    }

    const gridStep = Math.max(12, Number(config.grid_step) || DEFAULTS.gridStep);
    const dotRadius = Math.max(0.8, Number(config.dot_radius) || DEFAULTS.dotRadius);
    const hoverReach = Math.max(40, Number(config.hover_reach) || DEFAULTS.hoverReach);
    const hoverPush = Math.max(0, Number(config.hover_push ?? DEFAULTS.hoverPush));
    const hoverSwirl = Math.max(0, Number(config.hover_swirl ?? DEFAULTS.hoverSwirl));
    const hoverScale = Math.max(0, Number(config.hover_scale ?? DEFAULTS.hoverScale));
    const streamWavelength = Math.max(
      80,
      Number(config.stream_wavelength) || DEFAULTS.streamWavelength
    );
    const streamAmplitude = Math.max(
      0,
      Number(config.stream_amplitude ?? DEFAULTS.streamAmplitude)
    );

    let preset = selectPreset(
      colorMode === "dark" ? "dark" : "light",
      config.dark_mode_preset,
      config.light_mode_preset
    );

    let bgProg = null;
    let dotProg = null;
    let bgU = {};
    let dotU = {};
    let emptyVao = null;
    let contextLost = false;

    // Uniforms that only change with the preset (theme) or never: uploaded
    // once per program (setupGpu) and again on update({ colorMode }).
    // Uniform values live with the program object, so they survive between
    // frames. Everything that eases per frame (pointer, hover, tap, phase,
    // stream, the accent colours and the size) stays in renderFrame.
    const uploadStaticUniforms = () => {
      const isLight = preset.colorMode === "light" ? 1 : 0;

      gl.useProgram(bgProg);
      gl.uniform3fv(bgU.u_bgTopLeft, preset.bgTopLeft);
      gl.uniform3fv(bgU.u_bgTopRight, preset.bgTopRight);
      gl.uniform3fv(bgU.u_bgBottomLeft, preset.bgBottomLeft);
      gl.uniform3fv(bgU.u_bgBottomRight, preset.bgBottomRight);
      gl.uniform3fv(bgU.u_glowColor, preset.dotRestB);
      gl.uniform1f(bgU.u_isLight, isLight);
      gl.uniform1f(bgU.u_streamWavelength, streamWavelength);

      gl.useProgram(dotProg);
      gl.uniform1f(dotU.u_gridStep, gridStep);
      gl.uniform1f(dotU.u_dotRadius, dotRadius);
      gl.uniform1f(dotU.u_hoverReach, hoverReach);
      gl.uniform1f(dotU.u_hoverPush, hoverPush);
      gl.uniform1f(dotU.u_hoverSwirl, hoverSwirl);
      gl.uniform1f(dotU.u_hoverScale, hoverScale);
      gl.uniform3fv(dotU.u_dotRestA, preset.dotRestA);
      gl.uniform3fv(dotU.u_dotRestB, preset.dotRestB);
      gl.uniform3fv(dotU.u_dotHover, preset.dotHover);
      gl.uniform3fv(dotU.u_dotCore, preset.dotCore);
      gl.uniform1f(dotU.u_restAlpha, preset.restAlpha);
      gl.uniform1f(dotU.u_hoverAlpha, preset.hoverAlpha);
      gl.uniform1f(dotU.u_streamWavelength, streamWavelength);
      gl.uniform1f(dotU.u_streamAmplitude, streamAmplitude);
      gl.useProgram(null);
    };

    const setupGpu = () => {
      bgProg = createProgram(gl, BG_VERT, BG_FRAG);
      dotProg = createProgram(gl, DOT_VERT, DOT_FRAG);
      emptyVao = gl.createVertexArray();
      bgU = getUniforms(gl, bgProg, [
        "u_pointer",
        "u_hoverUv",
        "u_hoverAmount",
        "u_bgTopLeft",
        "u_bgTopRight",
        "u_bgBottomLeft",
        "u_bgBottomRight",
        "u_glowColor",
        "u_isLight",
        "u_stream",
        "u_streamTime",
        "u_streamColor",
        "u_streamBgGain",
        "u_cssSize",
        "u_streamWavelength",
      ]);
      dotU = getUniforms(gl, dotProg, [
        "u_cssSize",
        "u_cols",
        "u_gridOrigin",
        "u_gridStep",
        "u_dotRadius",
        "u_pointer",
        "u_hoverPos",
        "u_hoverAmount",
        "u_hoverReach",
        "u_hoverPush",
        "u_hoverSwirl",
        "u_hoverScale",
        "u_tapPos",
        "u_tapAge",
        "u_phase",
        "u_dotRestA",
        "u_dotRestB",
        "u_dotHover",
        "u_dotCore",
        "u_restAlpha",
        "u_hoverAlpha",
        "u_stream",
        "u_streamTime",
        "u_streamColor",
        "u_streamHover",
        "u_streamCore",
        "u_streamWavelength",
        "u_streamAmplitude",
      ]);
      uploadStaticUniforms();
    };

    try {
      setupGpu();
    } catch (err) {
      canvas.remove();
      throw err;
    }

    canvas.style.background = preset.background;

    const tracker = new PointerTracker(canvas, root.parentElement || root);
    let cssWidth = 1;
    let cssHeight = 1;
    let dpr = 1;
    let cols = 1;
    let rows = 1;
    let originX = 0;
    let originY = 0;

    const syncSize = () => {
      cssWidth = Math.max(1, canvas.clientWidth);
      cssHeight = Math.max(1, canvas.clientHeight);
      dpr = Math.min(window.devicePixelRatio || 1, 2);
      const pxW = Math.max(1, Math.round(cssWidth * dpr));
      const pxH = Math.max(1, Math.round(cssHeight * dpr));
      const resized = canvas.width !== pxW || canvas.height !== pxH;
      if (resized) {
        canvas.width = pxW;
        canvas.height = pxH;
      }
      cols = Math.ceil(cssWidth / gridStep) + 2;
      rows = Math.ceil(cssHeight / gridStep) + 2;
      const spanW = (cols - 1) * gridStep;
      const spanH = (rows - 1) * gridStep;
      originX = (cssWidth - spanW) * 0.5;
      originY = (cssHeight - spanH) * 0.5;
      // Layout just changed (ResizeObserver runs post-layout), so this is a
      // cheap moment to refresh the rect that pointer events map against.
      tracker.syncRect();
      return resized;
    };

    let phase = 0;
    let lastNow = 0;
    let rafId = 0;
    let running = false;

    // Wave state. `activity` is what the host asked for ("idle" or one of
    // WAVE_ACTIVITIES); `stream` eases towards 1 while an activity is set
    // (so the hue fades in and, when it ends, the grid settles back to rest
    // instead of snapping) and `streamTime` only advances while the wave is
    // visible. The accent colours ease too, so switching from streaming to
    // authorizing cross-fades blue -> amber without the wave stopping.
    let activity = "idle";
    let streamTarget = 0;
    let stream = 0;
    let streamTime = 0;
    const accentFor = (name) =>
      preset.accents[name] || preset.accents[WAVE_ACTIVITIES[0]];
    const accent = { tint: [], hover: [], core: [], bgGain: 0 };
    let accentTargetName = WAVE_ACTIVITIES[0];

    const streamActive = () => streamTarget > 0 || stream > 0.002;

    const snapAccent = () => {
      const target = accentFor(accentTargetName);
      accent.tint = target.tint.slice();
      accent.hover = target.hover.slice();
      accent.core = target.core.slice();
      accent.bgGain = target.bgGain;
    };
    snapAccent();

    const stepStream = (dt) => {
      if (dt <= 0) return;
      const seconds = streamTarget > stream ? DEFAULTS.streamFadeIn : DEFAULTS.streamFadeOut;
      // Exponential ease: ~95% of the way after `seconds`.
      const ease = 1 - Math.exp((-dt * 3) / Math.max(seconds, 0.05));
      stream += (streamTarget - stream) * ease;
      if (streamTarget === 0 && stream < 0.002) {
        stream = 0;
        streamTime = 0;
      }
      if (stream > 0) {
        streamTime = (streamTime + dt / DEFAULTS.streamWaveSeconds) % 1;
      }
      // Cross-fade the accent palette towards the active activity's.
      const target = accentFor(accentTargetName);
      const colorEase = 1 - Math.exp((-dt * 3) / DEFAULTS.streamFadeIn);
      for (const key of ["tint", "hover", "core"]) {
        for (let i = 0; i < 3; i += 1) {
          accent[key][i] += (target[key][i] - accent[key][i]) * colorEase;
        }
      }
      accent.bgGain += (target.bgGain - accent.bgGain) * colorEase;
    };

    const renderFrame = (dt, now) => {
      if (contextLost || gl.isContextLost()) return;
      if (canvas.width <= 0 || canvas.height <= 0) return;

      const s = tracker.sample(dt, now, cssWidth, cssHeight);

      gl.viewport(0, 0, canvas.width, canvas.height);
      gl.bindVertexArray(emptyVao);

      // 1. Subtle Argon atmospheric background wash
      gl.disable(gl.BLEND);
      gl.useProgram(bgProg);
      gl.uniform2f(bgU.u_pointer, s.pointer[0], s.pointer[1]);
      gl.uniform2f(
        bgU.u_hoverUv,
        cssWidth > 0 ? s.hover[0] / cssWidth : 0.5,
        cssHeight > 0 ? s.hover[1] / cssHeight : 0.5
      );
      gl.uniform1f(bgU.u_hoverAmount, s.hoverAmount);
      gl.uniform1f(bgU.u_stream, stream);
      gl.uniform1f(bgU.u_streamTime, streamTime);
      gl.uniform3fv(bgU.u_streamColor, accent.tint);
      gl.uniform1f(bgU.u_streamBgGain, accent.bgGain);
      gl.uniform2f(bgU.u_cssSize, cssWidth, cssHeight);
      gl.drawArrays(gl.TRIANGLES, 0, 3);

      // 2. Dotted grid with mouseover repulsion, bloom, and tap ripple
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      gl.useProgram(dotProg);
      gl.uniform2f(dotU.u_cssSize, cssWidth, cssHeight);
      gl.uniform1i(dotU.u_cols, cols);
      gl.uniform2f(dotU.u_gridOrigin, originX, originY);
      gl.uniform2f(dotU.u_pointer, s.pointer[0], s.pointer[1]);
      gl.uniform2f(dotU.u_hoverPos, s.hover[0], s.hover[1]);
      gl.uniform1f(dotU.u_hoverAmount, s.hoverAmount);
      gl.uniform2f(dotU.u_tapPos, s.tap[0], s.tap[1]);
      gl.uniform1f(dotU.u_tapAge, s.tapAge);
      gl.uniform1f(dotU.u_phase, phase);
      gl.uniform1f(dotU.u_stream, stream);
      gl.uniform1f(dotU.u_streamTime, streamTime);
      gl.uniform3fv(dotU.u_streamColor, accent.tint);
      gl.uniform3fv(dotU.u_streamHover, accent.hover);
      gl.uniform3fv(dotU.u_streamCore, accent.core);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, cols * rows);

      gl.disable(gl.BLEND);
      gl.bindVertexArray(null);
    };

    const resizeObserver = new ResizeObserver(() => {
      if (syncSize() || !rafId) {
        renderFrame(0, performance.now());
      }
    });
    resizeObserver.observe(canvas);
    syncSize();
    renderFrame(0, performance.now());

    const tick = (now) => {
      const elapsedMs = lastNow ? now - lastNow : 16;
      // Throttle idle background updates when pointer is inactive and no
      // streaming wave is running
      if (!tracker.isActive(now) && !streamActive() && elapsedMs < 40) {
        rafId = requestAnimationFrame(tick);
        return;
      }
      // Moving the window to a monitor with a different DPR does not change
      // clientWidth/Height, so the ResizeObserver never fires; re-sync the
      // backing store here (right before a render, so the cleared canvas is
      // repainted in the same frame).
      if (Math.min(window.devicePixelRatio || 1, 2) !== dpr) syncSize();
      const dt = lastNow ? Math.min(Math.max(elapsedMs, 0), 100) / 1000 : 0;
      lastNow = now;
      phase = (phase + dt / DEFAULTS.loopSeconds) % 1;
      stepStream(dt);
      renderFrame(dt, now);
      rafId = requestAnimationFrame(tick);
    };

    const syncLoop = () => {
      if (running && !rafId) {
        lastNow = performance.now();
        rafId = requestAnimationFrame(tick);
      } else if (!running && rafId) {
        cancelAnimationFrame(rafId);
        rafId = 0;
        lastNow = 0;
      }
    };

    const ctxAbort = new AbortController();
    canvas.addEventListener(
      "webglcontextlost",
      (ev) => {
        ev.preventDefault();
        contextLost = true;
      },
      { signal: ctxAbort.signal }
    );
    canvas.addEventListener(
      "webglcontextrestored",
      () => {
        contextLost = false;
        try {
          setupGpu();
        } catch (err) {
          // Throwing from a listener would only reach window.onerror and
          // leave the rAF loop, ResizeObserver and pointer tracker running
          // against a dead canvas; tear everything down instead.
          console.warn("[stage] GPU re-init failed after context restore", err);
          destroy();
          return;
        }
        renderFrame(0, performance.now());
      },
      { signal: ctxAbort.signal }
    );

    const destroy = () => {
      if (signal) signal.removeEventListener("abort", destroy);
      running = false;
      syncLoop();
      resizeObserver.disconnect();
      tracker.destroy();
      ctxAbort.abort();
      if (!contextLost && !gl.isContextLost()) {
        if (emptyVao) gl.deleteVertexArray(emptyVao);
        if (bgProg) gl.deleteProgram(bgProg);
        if (dotProg) gl.deleteProgram(dotProg);
        // Release the context eagerly; browsers cap live WebGL contexts and
        // only reclaim them on GC otherwise.
        gl.getExtension("WEBGL_lose_context")?.loseContext();
      }
      emptyVao = bgProg = dotProg = null;
      canvas.width = 0;
      canvas.height = 0;
      canvas.remove();
    };

    if (signal) {
      signal.addEventListener("abort", destroy, { once: true });
    }

    return {
      resume() {
        running = true;
        syncLoop();
      },
      pause() {
        running = false;
        tracker.reset();
        syncLoop();
        // Settle the wave instead of freezing it mid-fade; mirrors the
        // paused branch of update().
        stream = streamTarget;
        streamTime = 0;
        snapAccent();
        renderFrame(0, performance.now());
      },
      update(next = {}) {
        if (next.colorMode) {
          preset = selectPreset(
            next.colorMode === "dark" ? "dark" : "light",
            config.dark_mode_preset,
            config.light_mode_preset
          );
          canvas.style.background = preset.background;
          // The accent palette belongs to the preset: jump to the new
          // theme's colours rather than fading from the old theme's.
          snapAccent();
          // While the context is lost there is nothing to upload to;
          // setupGpu() re-uploads from the current preset on restore.
          if (!contextLost && !gl.isContextLost()) uploadStaticUniforms();
          if (!rafId) renderFrame(0, performance.now());
        }
        // `activity` ("idle" | "streaming" | "authorizing") or the older
        // `streaming` boolean (true == "streaming").
        let nextActivity = null;
        if (typeof next.activity === "string") {
          nextActivity = WAVE_ACTIVITIES.includes(next.activity) ? next.activity : "idle";
        } else if (typeof next.streaming === "boolean") {
          nextActivity = next.streaming ? "streaming" : "idle";
        }
        if (nextActivity !== null && nextActivity !== activity) {
          activity = nextActivity;
          streamTarget = activity === "idle" ? 0 : 1;
          if (activity !== "idle") {
            // Keep the last accent as the fade-out colour when going idle.
            accentTargetName = activity;
            if (stream <= 0.002) snapAccent();
          }
          if (!rafId) {
            // Paused (hidden pane or reduced motion): no wave, but still
            // show / clear the resting tint in a single static frame.
            stream = streamTarget;
            streamTime = 0;
            snapAccent();
            renderFrame(0, performance.now());
          }
        }
      },
      /** Whether the wave is currently requested (any activity). */
      get streaming() {
        return activity !== "idle";
      },
      /** The current activity: "idle", "streaming" or "authorizing". */
      get activity() {
        return activity;
      },
      destroy,
    };
  },
};
