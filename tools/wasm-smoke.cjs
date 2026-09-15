#!/usr/bin/env node
// Drives the wasm engine the way a browser page does: load a *built bundle*, present each
// command, answer it. Prints what it presented, so a caller can diff it against the native run.
//
// Usage: wasm-smoke.cjs <glue-dir> <bundle-dir>
//
// The bundle is where the entry point and the module come from — the same `manifest.json` the
// native loader reads (`RUNTIME.md §8`) — so the web target runs the artifact `vela build`
// produced rather than a module named by hand. The point of the diff is that it is the *same
// engine* on both sides: if the browser build played a different story from the native one,
// every claim about determinism would stop at the edge of the platform, which is exactly what
// `RUNTIME.md §4.1` says cannot happen.

const fs = require("node:fs");
const path = require("node:path");

const [glue, bundle] = process.argv.slice(2);
if (!glue || !bundle) {
  process.stderr.write("usage: wasm-smoke.cjs <glue-dir> <bundle-dir>\n");
  process.exit(2);
}

// The entry point is `module.label`; the bundle mirrors the source tree, so the module is
// `scripts/<module>.velac`.
const manifest = JSON.parse(
  fs.readFileSync(path.join(bundle, "manifest.json"), "utf8"),
);

// The program is one image, named for the module its entry point is in: `main.start` is
// `scripts/main.velac`, and `chapters.forest.clearing` is `scripts/chapters/forest.velac`.
const dot = manifest.entry.lastIndexOf(".");
const module = manifest.entry.slice(0, dot).replaceAll(".", "/");
const modulePath = path.join(bundle, "scripts", `${module}.velac`);

// And the whole entry is the label: a linked program's labels are qualified, so `main.start` is a
// label name rather than a module and a label.
const { Player } = require(path.resolve(glue, "vela_web.js"));
const player = new Player(fs.readFileSync(modulePath), manifest.entry);

// `step` presents the first command; every later one comes back from the answer, which is why
// the loop answers before asking again rather than calling `step` twice.
let command = player.step();
let presented = 0;

while (command !== "") {
  if (command.startsWith("fault: ")) {
    process.stderr.write(`${command}\n`);
    process.exit(1);
  }
  process.stdout.write(`${command}\n`);
  presented += 1;

  // A menu is answered with a choice and dialogue with an acknowledgement. Reading the command
  // to decide is what a page does too, until the focus stack is wired up.
  command = command.startsWith("menu") ? player.choose(0) : player.line();
}

if (presented === 0) {
  process.stderr.write("the story presented nothing\n");
  process.exit(1);
}
