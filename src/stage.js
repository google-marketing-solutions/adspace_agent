(() => {
  "use strict";

  // AdSpace Agent — Interactive Canvas Dotted Grid Stage (WebGL2)
  // Argon color palette with smooth cursor repulsion, size bloom, glow, and tap ripples.

  const PRESETS = {
    "argon-dark": {
      id: "argon-dark",
      colorMode: "dark",
      background: "#040816",
      bgTopLeft: [0.035, 0.068, 0.165],
      bgTopRight: [0.055, 0.135, 0.285],
      bgBottomLeft: [0.016, 0.032, 0.082],
      bgBottomRight: [0.022, 0.058, 0.175],
      dotRestA: [48 / 255, 148 / 255, 252 / 255],
      dotRestB: [116 / 255, 207 / 255, 255 / 255],
      dotHover: [152 / 255, 222 / 255, 255 / 255],
      dotCore: [242 / 255, 253 / 255, 255 / 255],
      restAlpha: 0.28,
      hoverAlpha: 0.94,
    },
    "argon-light": {
      id: "argon-light",
      colorMode: "light",
      background: "#f2f7fe",
      bgTopLeft: [0.975, 0.988, 1.0],
      bgTopRight: [0.875, 0.938, 0.996],
      bgBottomLeft: [0.918, 0.956, 0.996],
      bgBottomRight: [0.865, 0.932, 0.996],
      dotRestA: [56 / 255, 128 / 255, 228 / 255],
      dotRestB: [98 / 255, 168 / 255, 238 / 255],
      dotHover: [26 / 255, 115 / 255, 232 / 255],
      dotCore: [64 / 255, 156 / 255, 255 / 255],
      restAlpha: 0.25,
      hoverAlpha: 0.9,
    },
  };

  const DEFAULTS = {
    gridStep: 24,
    dotRadius: 1.4,
    hoverReach: 155,
    hoverPush: 7.5,
    hoverSwirl: 2.2,
    hoverScale: 1.35,
    loopSeconds: 14,
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
        const rect = this.canvas.getBoundingClientRect();
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
        const rect = this.canvas.getBoundingClientRect();
        if (!rect.width || !rect.height) return;
        this.tap[0] = ev.clientX - rect.left;
        this.tap[1] = ev.clientY - rect.top;
        this.tapTime = performance.now();
      };
      const onBlur = () => {
        this.reset();
      };

      listenEl.addEventListener("pointermove", updateTarget, { signal, passive: true });
      listenEl.addEventListener("pointerenter", updateTarget, { signal, passive: true });
      listenEl.addEventListener("pointerleave", onLeave, { signal, passive: true });
      listenEl.addEventListener("pointercancel", onLeave, { signal, passive: true });
      listenEl.addEventListener("pointerdown", onDown, { signal, passive: true });
      window.addEventListener("blur", onBlur, { signal });
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
uniform float u_dpr;
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

  // Subtle pointer parallax
  vec2 pos = basePos - u_pointer * 4.0;

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
  float activeMix = clamp(hoverBell + tapBoost * 0.7, 0.0, 1.0);
  vec3 dotColor = mix(restCol, u_dotHover, activeMix);
  dotColor = mix(dotColor, u_dotCore, clamp(hoverCore * 0.85 + tapBoost * 0.35, 0.0, 1.0));

  float alpha = mix(
    u_restAlpha * (0.9 + 0.14 * ambientWave),
    u_hoverAlpha,
    activeMix
  );

  // Dot radius with hover bloom & tap pulse, plus extra quad padding for soft glow & AA
  float radiusCss = u_dotRadius * (1.0 + hoverBell * u_hoverScale + tapBoost * 0.65);
  float quadHalfCss = radiusCss + 2.2 + hoverBell * 2.4;

  // 4-vertex triangle strip quad
  vec2 corner = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1)) * 2.0 - 1.0;
  vec2 vertCss = pos + corner * quadHalfCss;

  // Convert CSS pixel coords to WebGL clip space
  vec2 clip = (vertCss / max(u_cssSize, vec2(1.0))) * 2.0 - 1.0;
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);

  v_localCoord = corner * quadHalfCss;
  // Pass radius in local CSS pixels in v_hoverMix's companion or normalize localCoord by radius
  v_localCoord /= max(radiusCss, 0.1);
  v_color = dotColor;
  v_alpha = clamp(alpha, 0.0, 1.0);
  v_hoverMix = activeMix;
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

  const AdSpaceStage = {
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
        antialias: true,
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

      let bgProg = null;
      let dotProg = null;
      let bgU = {};
      let dotU = {};
      let emptyVao = null;
      let contextLost = false;

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
        ]);
        dotU = getUniforms(gl, dotProg, [
          "u_cssSize",
          "u_dpr",
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
        ]);
      };

      try {
        setupGpu();
      } catch (err) {
        canvas.remove();
        throw err;
      }

      let currentMode = colorMode === "dark" ? "dark" : "light";
      let preset = selectPreset(currentMode, config.dark_mode_preset, config.light_mode_preset);
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
        return resized;
      };

      let phase = 0;
      let lastNow = 0;
      let rafId = 0;
      let running = false;

      const renderFrame = (dt, now) => {
        if (contextLost || gl.isContextLost()) return;
        if (canvas.width <= 0 || canvas.height <= 0) return;

        const s = tracker.sample(dt, now, cssWidth, cssHeight);
        const isLight = preset.colorMode === "light" ? 1 : 0;

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
        gl.uniform3fv(bgU.u_bgTopLeft, preset.bgTopLeft);
        gl.uniform3fv(bgU.u_bgTopRight, preset.bgTopRight);
        gl.uniform3fv(bgU.u_bgBottomLeft, preset.bgBottomLeft);
        gl.uniform3fv(bgU.u_bgBottomRight, preset.bgBottomRight);
        gl.uniform3fv(bgU.u_glowColor, preset.dotRestB);
        gl.uniform1f(bgU.u_isLight, isLight);
        gl.drawArrays(gl.TRIANGLES, 0, 3);

        // 2. Dotted grid with mouseover repulsion, bloom, and tap ripple
        gl.enable(gl.BLEND);
        gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
        gl.useProgram(dotProg);
        gl.uniform2f(dotU.u_cssSize, cssWidth, cssHeight);
        gl.uniform1f(dotU.u_dpr, dpr);
        gl.uniform1i(dotU.u_cols, cols);
        gl.uniform2f(dotU.u_gridOrigin, originX, originY);
        gl.uniform1f(dotU.u_gridStep, gridStep);
        gl.uniform1f(dotU.u_dotRadius, dotRadius);
        gl.uniform2f(dotU.u_pointer, s.pointer[0], s.pointer[1]);
        gl.uniform2f(dotU.u_hoverPos, s.hover[0], s.hover[1]);
        gl.uniform1f(dotU.u_hoverAmount, s.hoverAmount);
        gl.uniform1f(dotU.u_hoverReach, hoverReach);
        gl.uniform1f(dotU.u_hoverPush, hoverPush);
        gl.uniform1f(dotU.u_hoverSwirl, hoverSwirl);
        gl.uniform1f(dotU.u_hoverScale, hoverScale);
        gl.uniform2f(dotU.u_tapPos, s.tap[0], s.tap[1]);
        gl.uniform1f(dotU.u_tapAge, s.tapAge);
        gl.uniform1f(dotU.u_phase, phase);
        gl.uniform3fv(dotU.u_dotRestA, preset.dotRestA);
        gl.uniform3fv(dotU.u_dotRestB, preset.dotRestB);
        gl.uniform3fv(dotU.u_dotHover, preset.dotHover);
        gl.uniform3fv(dotU.u_dotCore, preset.dotCore);
        gl.uniform1f(dotU.u_restAlpha, preset.restAlpha);
        gl.uniform1f(dotU.u_hoverAlpha, preset.hoverAlpha);
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
        // Throttle idle background updates when pointer is inactive
        if (!tracker.isActive(now) && elapsedMs < 40) {
          rafId = requestAnimationFrame(tick);
          return;
        }
        const dt = lastNow ? Math.min(Math.max(elapsedMs, 0), 100) / 1000 : 0;
        lastNow = now;
        phase = (phase + dt / DEFAULTS.loopSeconds) % 1;
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
        canvas.remove();
        throw err;
      }
          renderFrame(0, performance.now());
        },
        { signal: ctxAbort.signal }
      );

      const destroy = () => {
        running = false;
        syncLoop();
        resizeObserver.disconnect();
        tracker.destroy();
        ctxAbort.abort();
        if (!contextLost && !gl.isContextLost()) {
          if (emptyVao) gl.deleteVertexArray(emptyVao);
          if (bgProg) gl.deleteProgram(bgProg);
          if (dotProg) gl.deleteProgram(dotProg);
        }
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
          renderFrame(0, performance.now());
        },
        update(next = {}) {
          if (next.colorMode) {
            currentMode = next.colorMode === "dark" ? "dark" : "light";
            preset = selectPreset(currentMode, config.dark_mode_preset, config.light_mode_preset);
            canvas.style.background = preset.background;
            if (!rafId) renderFrame(0, performance.now());
          }
        },
        destroy,
      };
    },
  };

  window.AdSpaceStage = AdSpaceStage;
})();
