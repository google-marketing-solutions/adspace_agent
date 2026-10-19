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

// Animated counters: pulse helper and the slot-machine (odometer) digit reels. Pure DOM, no app state.

export const prefersReducedMotion = () =>
  Boolean(window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches);

export const triggerPulseClass = (el, className) => {
  if (!el || prefersReducedMotion()) return;
  const now = performance.now();
  if (el._lastPulseAt && now - el._lastPulseAt < 450) return;
  el._lastPulseAt = now;
  el.classList.remove(className);
  void el.offsetWidth;
  el.classList.add(className);
  el.addEventListener(
    "animationend",
    () => {
      el.classList.remove(className);
    },
    { once: true }
  );
};

// 23-cell vertical reel: top guard '9', first cycle '0'..'9' (indices 1..10),
// second cycle '0'..'9' (indices 11..20), bottom guards '0', '1' (indices 21..22).
// Allows every digit to spin forward and overshoot/bounce without showing blank space.
export const SLOT_REEL_CELLS = [
  "9",
  "0",
  "1",
  "2",
  "3",
  "4",
  "5",
  "6",
  "7",
  "8",
  "9",
  "0",
  "1",
  "2",
  "3",
  "4",
  "5",
  "6",
  "7",
  "8",
  "9",
  "0",
  "1",
];

export const slotReelOffset = (cellIdx) => `calc(-${cellIdx} * var(--odometer-h))`;

export const resetDigitReelNow = (digitEl, digit) => {
  if (digitEl._resetTimer) {
    clearTimeout(digitEl._resetTimer);
    digitEl._resetTimer = null;
  }
  digitEl._digit = digit;
  digitEl._inSecondCycle = false;
  digitEl._ribbon.style.transition = "none";
  digitEl._ribbon.style.transform = `translateY(${slotReelOffset(1 + digit)})`;
};

export const createSlotDigitEl = (initialDigit) => {
  const digitEl = document.createElement("span");
  digitEl.className = "odometer-digit";
  const ribbon = document.createElement("span");
  ribbon.className = "odometer-ribbon";
  // The 23-cell reel is purely visual; the counter's accessible value is
  // the `aria-label` set by `applySlotMachineText`.
  ribbon.setAttribute("aria-hidden", "true");
  for (const ch of SLOT_REEL_CELLS) {
    const cell = document.createElement("span");
    cell.className = "odometer-cell";
    cell.textContent = ch;
    ribbon.appendChild(cell);
  }
  const d = Math.max(0, Math.min(9, Number(initialDigit) || 0));
  digitEl.appendChild(ribbon);
  digitEl._ribbon = ribbon;
  digitEl._digit = d;
  digitEl._inSecondCycle = false;
  digitEl._resetTimer = null;
  resetDigitReelNow(digitEl, d);
  return digitEl;
};

export const spinSlotDigit = (digitEl, nextDigit, staggerMs = 0, forceSpin = false) => {
  const ribbon = digitEl._ribbon;
  if (!ribbon) return;
  const targetDigit = Math.max(0, Math.min(9, Number(nextDigit) || 0));
  const prevDigit = typeof digitEl._digit === "number" ? digitEl._digit : 0;
  if (!forceSpin && prevDigit === targetDigit) return;

  if (digitEl._resetTimer) {
    clearTimeout(digitEl._resetTimer);
    digitEl._resetTimer = null;
  }

  if (digitEl._inSecondCycle) {
    ribbon.style.transition = "none";
    ribbon.style.transform = `translateY(${slotReelOffset(1 + prevDigit)})`;
    digitEl._inSecondCycle = false;
    void ribbon.offsetHeight;
  }

  digitEl._digit = targetDigit;
  const useSecondCycle = forceSpin || targetDigit <= prevDigit;
  const targetCellIdx = (useSecondCycle ? 11 : 1) + targetDigit;
  const durationMs = 560 + staggerMs;

  // Ease-out with a slot-machine spring overshoot & bounce back into place
  ribbon.style.transition = `transform ${durationMs}ms cubic-bezier(0.22, 1.42, 0.36, 1)`;
  ribbon.style.transform = `translateY(${slotReelOffset(targetCellIdx)})`;

  if (useSecondCycle) {
    digitEl._inSecondCycle = true;
    digitEl._resetTimer = setTimeout(() => {
      digitEl._resetTimer = null;
      if (!digitEl._inSecondCycle) return;
      ribbon.style.transition = "none";
      ribbon.style.transform = `translateY(${slotReelOffset(1 + targetDigit)})`;
      digitEl._inSecondCycle = false;
    }, durationMs + 30);
  }
};

export const applySlotMachineText = (el, formattedStr, animate) => {
  // `role="img"` makes the label authoritative so assistive tech reads the
  // formatted value once instead of every reel cell / static glyph.
  el.setAttribute("role", "img");
  el.setAttribute("aria-label", formattedStr);
  if (prefersReducedMotion()) {
    el.classList.remove("odometer");
    el._odometerTemplate = null;
    el._odometerDigits = null;
    el.textContent = formattedStr;
    return;
  }

  el.classList.add("odometer");
  const templateSig = formattedStr.replace(/\d/g, "#");
  const chars = Array.from(formattedStr);

  if (el._odometerTemplate === templateSig && el._odometerDigits) {
    let dIdx = 0;
    for (const ch of chars) {
      if (ch >= "0" && ch <= "9") {
        const digitEl = el._odometerDigits[dIdx];
        if (digitEl) {
          if (animate) {
            spinSlotDigit(digitEl, Number(ch), dIdx * 28, false);
          } else {
            resetDigitReelNow(digitEl, Number(ch));
          }
        }
        dIdx += 1;
      }
    }
    return;
  }

  // Preserve existing digit reels aligned from right-to-left (ones, tens, hundreds...)
  const prevDigits = Array.isArray(el._odometerDigits) ? el._odometerDigits : [];
  const totalNextDigits = chars.filter((c) => c >= "0" && c <= "9").length;
  const reuseOffset = totalNextDigits - prevDigits.length;

  const frag = document.createDocumentFragment();
  const nextDigits = [];
  const spinsToRun = [];
  let dIdx = 0;

  for (const ch of chars) {
    if (ch >= "0" && ch <= "9") {
      const targetDigit = Number(ch);
      const existingIdx = dIdx - reuseOffset;
      let digitEl;
      let isNewReel = false;
      if (existingIdx >= 0 && existingIdx < prevDigits.length) {
        digitEl = prevDigits[existingIdx];
      } else {
        digitEl = createSlotDigitEl(animate ? 0 : targetDigit);
        isNewReel = true;
      }
      nextDigits.push(digitEl);
      frag.appendChild(digitEl);
      if (animate) {
        spinsToRun.push({
          digitEl,
          targetDigit,
          stagger: dIdx * 28,
          forceSpin: isNewReel,
        });
      } else if (!isNewReel) {
        resetDigitReelNow(digitEl, targetDigit);
      }
      dIdx += 1;
    } else {
      const staticEl = document.createElement("span");
      staticEl.className = "odometer-static";
      staticEl.textContent = ch;
      frag.appendChild(staticEl);
    }
  }

  el.replaceChildren(frag);
  el._odometerTemplate = templateSig;
  el._odometerDigits = nextDigits;

  if (animate && spinsToRun.length > 0) {
    // Flush layout so newly inserted reels animate from 0 to their target digit
    void el.offsetHeight;
    for (const item of spinsToRun) {
      spinSlotDigit(item.digitEl, item.targetDigit, item.stagger, item.forceSpin);
    }
  }
};

export const animateCounter = (el, nextValue, formatter, options = {}) => {
  if (!el) return;
  const target = Math.max(0, Number(nextValue) || 0);
  const formatSig = options.formatSig ?? "";
  el._counterFormatFn = formatter;
  if (el._counterTarget === target && el._counterFormatSig === formatSig) {
    return;
  }

  const currentVal =
    typeof el._counterValue === "number" && Number.isFinite(el._counterValue)
      ? el._counterValue
      : 0;
  el._counterTarget = target;
  el._counterFormatSig = formatSig;

  if (prefersReducedMotion() || target === 0 || Math.abs(target - currentVal) < 1e-9) {
    if (el._slotThrottleTimer) {
      clearTimeout(el._slotThrottleTimer);
      el._slotThrottleTimer = null;
    }
    el._counterValue = target;
    applySlotMachineText(el, formatter(target), false);
    return;
  }

  const commitSlotUpdate = () => {
    el._slotLastUpdateAt = performance.now();
    const endVal = el._counterTarget ?? 0;
    el._counterValue = endVal;
    const fmt = el._counterFormatFn || formatter;
    applySlotMachineText(el, fmt(endVal), true);
  };

  const now = performance.now();
  const minIntervalMs = 150;
  const elapsedSinceLast = now - (el._slotLastUpdateAt || 0);

  if (elapsedSinceLast >= minIntervalMs) {
    if (el._slotThrottleTimer) {
      clearTimeout(el._slotThrottleTimer);
      el._slotThrottleTimer = null;
    }
    commitSlotUpdate();
  } else if (!el._slotThrottleTimer) {
    el._slotThrottleTimer = setTimeout(() => {
      el._slotThrottleTimer = null;
      commitSlotUpdate();
    }, minIntervalMs - elapsedSinceLast);
  }
};
