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

// Links the studio's ES modules (src/app) with V8's own module linker, without
// evaluating them, so no DOM is needed. There is no bundler: the files are
// served verbatim by the Rust server, and nothing else resolves their imports
// before a browser does. A named import that its target does not export fails
// here with the same `SyntaxError` the browser would raise, which otherwise
// stops the whole studio from loading.
//
// It also checks that the modules reachable from the entry points are exactly
// the ones registered in `app_modules!` in src/server.rs: a module that is
// imported but not registered is a 404 in production.
//
// Usage: node --experimental-vm-modules --no-warnings scripts/check-js-modules.mjs

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import vm from "node:vm";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const root = path.join(repo, "src", "app");
// `app.js` is loaded by studio.html; `ui-helper.js` is the `/ui/` entry point,
// also served as `/oauth2/helper.js`.
const entries = ["app.js", "ui-helper.js"];

if (typeof vm.SourceTextModule !== "function") {
  console.error("Run with: node --experimental-vm-modules scripts/check-js-modules.mjs");
  process.exit(2);
}

const context = vm.createContext({});
const modules = new Map();

const load = (file, importer) => {
  if (modules.has(file)) return modules.get(file);
  if (!existsSync(file)) {
    throw new Error(`${importer} imports ${path.relative(root, file)}, which does not exist`);
  }
  const mod = new vm.SourceTextModule(readFileSync(file, "utf8"), { identifier: file, context });
  modules.set(file, mod);
  return mod;
};

const linker = (specifier, referencing) => {
  const importer = path.relative(root, referencing.identifier);
  // The server mounts src/app at /app/; ui-helper.js imports by absolute URL.
  if (specifier.startsWith("/app/")) return load(path.join(root, specifier.slice(5)), importer);
  if (specifier.startsWith(".")) {
    return load(path.resolve(path.dirname(referencing.identifier), specifier), importer);
  }
  throw new Error(`${importer} imports '${specifier}', which is neither relative nor under /app/`);
};

const failures = [];
for (const entry of entries) {
  try {
    const mod = load(path.join(root, entry), "(entry point)");
    await mod.link(linker);
    if (mod.status === "unlinked" && typeof mod.instantiate === "function") mod.instantiate();
  } catch (err) {
    failures.push(`${entry}: ${err.name}: ${err.message}`);
  }
}

const reached = [...modules.keys()].map((f) => path.relative(root, f).split(path.sep).join("/"));
const serverSrc = readFileSync(path.join(repo, "src", "server.rs"), "utf8");
const block = serverSrc.match(/^app_modules!\s*\{([^}]*)\}/m);
if (!block) {
  failures.push("could not find the `app_modules! { ... }` list in src/server.rs");
} else {
  const registered = new Set([...block[1].matchAll(/"([^"]+)"/g)].map((m) => m[1]));
  for (const name of reached) {
    if (!registered.has(name)) {
      failures.push(`${name} is imported but not registered in app_modules! (404 when served)`);
    }
  }
  for (const name of registered) {
    if (!reached.includes(name)) {
      failures.push(`${name} is registered in app_modules! but no entry point reaches it`);
    }
  }
}

if (failures.length > 0) {
  console.error(`JS module check failed (${failures.length}):`);
  for (const failure of failures) console.error(`  ${failure}`);
  process.exit(1);
}
console.log(`JS module check passed: ${reached.length} modules link from ${entries.join(", ")}`);
